use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use log::error;
use parking_lot::Mutex;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{mpsc, oneshot},
    time,
};
use tokio_util::codec::Framed;

use crate::error::{wrap_err, NeomacsError, Result};

use super::codec::{ErrorResponse, Message, MessageCodec, Notification, Request, Response};

type RequestHandler = mpsc::Sender<(Request, oneshot::Sender<Result<Response>>)>;
type NotificationHandler = mpsc::Sender<(Notification, oneshot::Sender<Result<()>>)>;

#[async_trait]
pub trait RpcSocket<C: AsyncRead + AsyncWrite> {
    /// Accepts a new client, returning the connection
    async fn accept(&self) -> Result<C>;
}

struct RpcServer<
    C: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    S: RpcSocket<C> + Send + Sync + 'static,
> {
    socket: Arc<tokio::sync::Mutex<S>>,
    is_shutdown: Arc<Mutex<bool>>,
    connections: Arc<Mutex<Vec<Connection<C>>>>,
}

impl<C: AsyncRead + AsyncWrite + Send + Sync + Unpin, S: RpcSocket<C> + Send + Sync>
    RpcServer<C, S>
{
    pub fn new(socket: S) -> Self {
        Self {
            socket: Arc::new(tokio::sync::Mutex::new(socket)),
            is_shutdown: Arc::new(Mutex::new(false)),
            connections: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn start(&self) {
        let socket = self.socket.clone();
        let is_shutdown = self.is_shutdown.clone();
        let connections = self.connections.clone();
        tokio::spawn(async move {
            loop {
                if *is_shutdown.lock() {
                    break;
                }
                match socket.lock().await.accept().await {
                    Ok(conn) => {
                        let codec = MessageCodec::new();
                        let framed = Framed::new(conn, codec);
                        // TODO gather handlers
                        let connection = Connection::new(framed, HashMap::new(), HashMap::new());
                        connection.start();
                        connections.lock().push(connection);
                    }
                    Err(e) => {
                        error!("Error accepting socket connection: {}", e);
                    }
                }
            }
        });
    }

    pub async fn terminate(&mut self) {
        *self.is_shutdown.lock() = true;
        let mut connections = self.connections.lock();
        for conn in connections.as_mut_slice() {
            conn.terminate().await;
        }
    }
}

struct Connection<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    framed: Arc<tokio::sync::Mutex<Framed<C, MessageCodec>>>,
    is_shutdown: Arc<Mutex<bool>>,
    request_handlers: Arc<HashMap<String, RequestHandler>>,
    notification_handlers: Arc<HashMap<String, NotificationHandler>>,
}

impl<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> Connection<C> {
    pub fn new(
        framed: Framed<C, MessageCodec>,
        request_handlers: HashMap<String, RequestHandler>,
        notification_handlers: HashMap<String, NotificationHandler>,
    ) -> Self {
        Self {
            framed: Arc::new(tokio::sync::Mutex::new(framed)),
            is_shutdown: Arc::new(Mutex::new(false)),
            request_handlers: Arc::new(request_handlers),
            notification_handlers: Arc::new(notification_handlers),
        }
    }

    pub fn start(&self) {
        let framed = self.framed.clone();
        let is_shutdown = self.is_shutdown.clone();
        let request_handlers = self.request_handlers.clone();
        let notification_handlers = self.notification_handlers.clone();
        tokio::spawn(async move {
            loop {
                if *is_shutdown.lock() {
                    break;
                }
                while let Some(msg_res) = framed.lock().await.next().await {
                    match msg_res {
                        Ok(message) => {
                            let request_handlers = request_handlers.clone();
                            let notification_handlers = notification_handlers.clone();
                            let framed = framed.clone();
                            tokio::spawn(async move {
                                let result = match message {
                                    Message::Request(request) => {
                                        if let Some(handler) = request_handlers.get(&request.method) {
                                            async {
                                                let (tx, rx) = oneshot::channel();
                                                wrap_err(handler.send((request, tx)).await)?;
                                                match time::timeout(Duration::from_secs(5), rx).await? {
                                                    Ok(result) => match result {
                                                        Ok(response) => {
                                                            let mut framed = framed.lock().await;
                                                            framed.send(Message::Response(response)).await?;
                                                            Ok(())
                                                        }
                                                        Err(e) => Err(e)
                                                    },
                                                    Err(e) => Err(NeomacsError::Unhandled(anyhow::Error::new(e)))
                                                }
                                            }.await
                                        } else {
                                            let err_response = Message::Response(Response {
                                                msg_id: request.msg_id,
                                                error: Some(
                                                    ErrorResponse::new(
                                                        "UNKNOWN_METHOD",
                                                        format!("Unknown method {}", request.method.as_str())
                                                    ).into()
                                                ),
                                                result: None
                                            });
                                            async {
                                                let mut framed = framed.lock().await;
                                                framed.send(err_response).await?;
                                                Err(NeomacsError::RequestError(format!(
                                                    "No request handler registered for notification {}",
                                                    request.method.as_str())
                                                ))
                                            }.await
                                        }
                                    }
                                    Message::Notification(notification) => {
                                        if let Some(handler) = notification_handlers.get(&notification.method) {
                                            async {
                                                let (tx, rx) = oneshot::channel();
                                                wrap_err(handler.send((notification, tx)).await)?;
                                                match time::timeout(Duration::from_secs(5), rx).await? {
                                                    Ok(result) => result,
                                                    Err(e) => Err(NeomacsError::Unhandled(anyhow::Error::new(e)))
                                                }
                                            }.await
                                        } else {
                                            Err(NeomacsError::RequestError(format!(
                                                "No notification handler registered for notification {}",
                                                notification.method.as_str())))
                                        }
                                    }
                                    Message::Response(response) => {
                                        Err(NeomacsError::RequestError(format!(
                                            "Received an unexpected response message with message id: {}",
                                            response.msg_id)
                                        ))
                                    }
                                };
                                if let Err(e) = result {
                                    error!("Unhandled error processing RPC message: {}", e);
                                };
                            });
                        }
                        Err(e) => {
                            error!("Error reading data from open connection: {}", e);
                        }
                    }
                }
            }
        });
    }

    pub async fn terminate(&mut self) {
        *self.is_shutdown.lock() = true;
        self.framed
            .lock()
            .await
            .close()
            .await
            .expect("Error closing connection");
    }
}
