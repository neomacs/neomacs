use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use log::{error, info};
use parking_lot::Mutex;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{oneshot, RwLock},
    time,
};
use tokio_util::codec::Framed;

use crate::error::{wrap_err, NeomacsError, Result};

use super::{
    codec::{ErrorResponse, ErrorType, Message, MessageCodec, Response},
    handler::{NotificationHandleSender, RequestHandleSender},
};

#[async_trait]
pub trait RpcSocket<C: AsyncRead + AsyncWrite> {
    /// Accepts a new client, returning the connection
    async fn accept(&self) -> Result<C>;
}

pub struct RpcServer<
    C: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    S: RpcSocket<C> + Send + Sync + 'static,
> {
    socket: Arc<tokio::sync::Mutex<S>>,
    is_shutdown: Arc<Mutex<bool>>,
    connections: Arc<Mutex<Vec<Connection<C>>>>,
    request_handlers: Arc<RwLock<HashMap<String, RequestHandleSender>>>,
    notification_handlers: Arc<RwLock<HashMap<String, NotificationHandleSender>>>,
}

impl<C: AsyncRead + AsyncWrite + Send + Sync + Unpin, S: RpcSocket<C> + Send + Sync>
    RpcServer<C, S>
{
    pub fn new(socket: S) -> Self {
        Self {
            socket: Arc::new(tokio::sync::Mutex::new(socket)),
            is_shutdown: Arc::new(Mutex::new(false)),
            connections: Arc::new(Mutex::new(Vec::new())),
            request_handlers: Arc::new(RwLock::new(HashMap::new())),
            notification_handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_request_handler<M: Into<String>>(
        &mut self,
        method: M,
        handler: RequestHandleSender,
    ) {
        let mut handlers = self.request_handlers.write().await;
        handlers.insert(method.into(), handler);
    }

    pub async fn register_notification_handler<M: Into<String>>(
        &mut self,
        method: M,
        handler: NotificationHandleSender,
    ) {
        let mut handlers = self.notification_handlers.write().await;
        handlers.insert(method.into(), handler);
    }

    pub fn start(&self) {
        let socket = self.socket.clone();
        let is_shutdown = self.is_shutdown.clone();
        let connections = self.connections.clone();
        let request_handlers = self.request_handlers.clone();
        let notification_handlers = self.notification_handlers.clone();
        tokio::spawn(async move {
            loop {
                if *is_shutdown.lock() {
                    break;
                }
                match socket.lock().await.accept().await {
                    Ok(conn) => {
                        let codec = MessageCodec::new();
                        let framed = Framed::new(conn, codec);
                        let connection = Connection::new(
                            framed,
                            request_handlers.clone(),
                            notification_handlers.clone(),
                        );
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
    framed_read: Arc<tokio::sync::Mutex<SplitStream<Framed<C, MessageCodec>>>>,
    framed_write: Arc<tokio::sync::Mutex<SplitSink<Framed<C, MessageCodec>, Message>>>,
    is_shutdown: Arc<Mutex<bool>>,
    request_handlers: Arc<RwLock<HashMap<String, RequestHandleSender>>>,
    notification_handlers: Arc<RwLock<HashMap<String, NotificationHandleSender>>>,
}

impl<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> Connection<C> {
    pub fn new(
        framed: Framed<C, MessageCodec>,
        request_handlers: Arc<RwLock<HashMap<String, RequestHandleSender>>>,
        notification_handlers: Arc<RwLock<HashMap<String, NotificationHandleSender>>>,
    ) -> Self {
        let (write, read) = framed.split();
        Self {
            framed_read: Arc::new(tokio::sync::Mutex::new(read)),
            framed_write: Arc::new(tokio::sync::Mutex::new(write)),
            is_shutdown: Arc::new(Mutex::new(false)),
            request_handlers,
            notification_handlers,
        }
    }

    pub fn start(&self) {
        let framed_read = self.framed_read.clone();
        let framed_write = self.framed_write.clone();
        let is_shutdown = self.is_shutdown.clone();
        let request_handlers = self.request_handlers.clone();
        let notification_handlers = self.notification_handlers.clone();
        tokio::spawn(async move {
            loop {
                if *is_shutdown.lock() {
                    break;
                }
                if let Some(msg_res) = Self::get_next_message(framed_read.clone()).await {
                    match msg_res {
                        Ok(message) => {
                            let request_handlers = request_handlers.clone();
                            let notification_handlers = notification_handlers.clone();
                            let framed_write = framed_write.clone();
                            tokio::spawn(async move {
                                let result = match message {
                                    Message::Request(request) => {
                                        info!("Incoming request, method: {}, msg_id: {}", request.method.as_str(), request.msg_id);
                                        let request_handlers = request_handlers.read().await;
                                        if let Some(handler) = request_handlers.get(&request.method) {
                                            async {
                                                let (tx, rx) = oneshot::channel();
                                                wrap_err(handler.send((request, tx)).await)?;
                                                match time::timeout(Duration::from_secs(5), rx).await? {
                                                    Ok(result) => match result {
                                                        Ok(response) => {
                                                            Self::send_response(framed_write.clone(), response).await?;
                                                            Ok(())
                                                        }
                                                        Err(e) => Err(e)
                                                    },
                                                    Err(e) => Err(NeomacsError::Unhandled(anyhow::Error::new(e)))
                                                }
                                            }.await
                                        } else {
                                            let err_response = Response {
                                                msg_id: request.msg_id,
                                                error: Some(
                                                    ErrorResponse::new(
                                                        ErrorType::UnknownMethod,
                                                        format!("Unknown method {}", request.method.as_str())
                                                    ).into()
                                                ),
                                                result: None
                                            };
                                            async {
                                                Self::send_response(framed_write.clone(), err_response).await?;
                                                Err(NeomacsError::RequestError(format!(
                                                    "No request handler registered for notification {}",
                                                    request.method.as_str())
                                                ))
                                            }.await
                                        }
                                    }
                                    Message::Notification(notification) => {
                                        info!("Incoming notification, method: {}", notification.method.as_str());
                                        let notification_handlers = notification_handlers.read().await;
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
        self.framed_write
            .lock()
            .await
            .close()
            .await
            .expect("Error closing connection");
    }

    async fn get_next_message(
        framed_read: Arc<tokio::sync::Mutex<SplitStream<Framed<C, MessageCodec>>>>,
    ) -> Option<Result<Message>> {
        framed_read.lock().await.next().await
    }

    async fn send_response(
        framed_write: Arc<tokio::sync::Mutex<SplitSink<Framed<C, MessageCodec>, Message>>>,
        response: Response,
    ) -> Result<()> {
        info!("Sending response, msg_id: {}", response.msg_id);
        framed_write
            .lock()
            .await
            .send(Message::Response(response))
            .await
    }
}
