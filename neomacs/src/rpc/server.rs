use anyhow::anyhow;
use atomic_counter::{AtomicCounter, RelaxedCounter};
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
    sync::{broadcast, mpsc, oneshot, RwLock},
    time,
};
use tokio_util::codec::Framed;

use crate::{
    error::{wrap_err, NeomacsError, Result},
    rpc::handler::RequestContext,
};

use super::{
    codec::{ErrorResponse, ErrorType, Message, MessageCodec, Notification, Request, Response},
    handler::{
        NotificationHandleSender, NotificationHandler, NotificationService, RequestHandleSender,
        RequestHandler, RequestService,
    },
};

#[async_trait]
pub trait RpcSocket<C: AsyncRead + AsyncWrite> {
    /// Accepts a new client, returning the connection
    async fn accept(&self) -> Result<C>;
}

/// The main entry for handling RPC requests.
///
/// Handlers should be added by defining a new `RequestService` or
/// `NotificationService` and calling the `register_request_handler`
/// or `register_notification_handler` functions.
pub struct RpcServer<
    C: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    S: RpcSocket<C> + Send + Sync + 'static,
> {
    socket: Arc<tokio::sync::Mutex<S>>,
    is_shutdown: Arc<Mutex<bool>>,
    connections: Arc<RwLock<HashMap<u64, Connection<C>>>>,
    request_handlers: Arc<RwLock<HashMap<String, RequestHandleSender>>>,
    notification_handlers: Arc<RwLock<HashMap<String, NotificationHandleSender>>>,
    id_counter: Arc<RelaxedCounter>,
    request_tx: mpsc::Sender<(u64, Request, oneshot::Sender<Result<Response>>)>,
    request_rx:
        Arc<tokio::sync::Mutex<mpsc::Receiver<(u64, Request, oneshot::Sender<Result<Response>>)>>>,
    notification_tx: mpsc::Sender<(u64, Notification, oneshot::Sender<Result<()>>)>,
    notification_rx:
        Arc<tokio::sync::Mutex<mpsc::Receiver<(u64, Notification, oneshot::Sender<Result<()>>)>>>,
}

impl<C: AsyncRead + AsyncWrite + Send + Sync + Unpin, S: RpcSocket<C> + Send + Sync>
    RpcServer<C, S>
{
    pub fn new(socket: S) -> Self {
        let (request_tx, request_rx) = mpsc::channel(128);
        let (notification_tx, notification_rx) = mpsc::channel(128);
        Self {
            socket: Arc::new(tokio::sync::Mutex::new(socket)),
            is_shutdown: Arc::new(Mutex::new(false)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            request_handlers: Arc::new(RwLock::new(HashMap::new())),
            notification_handlers: Arc::new(RwLock::new(HashMap::new())),
            id_counter: Arc::new(RelaxedCounter::new(0)),
            request_tx,
            request_rx: Arc::new(tokio::sync::Mutex::new(request_rx)),
            notification_tx,
            notification_rx: Arc::new(tokio::sync::Mutex::new(notification_rx)),
        }
    }

    pub async fn register_request_handler<H: RequestHandler + Send + Sync>(
        &mut self,
        service: &RequestService<H>,
    ) -> Result<()> {
        let mut handlers = self.request_handlers.write().await;
        let notif_handlers = self.notification_handlers.read().await;
        for method in RequestService::<H>::handled_methods().await {
            let owned_method = method.to_string();
            if handlers.contains_key(&owned_method) || notif_handlers.contains_key(&owned_method) {
                return Err(NeomacsError::Unhandled(anyhow!(
                    "Handler for RPC method {} already defined",
                    method
                )));
            } else {
                handlers.insert(owned_method, service.sender());
            }
        }
        Ok(())
    }

    pub async fn register_notification_handler<H: NotificationHandler + Send + Sync>(
        &mut self,
        service: &NotificationService<H>,
    ) -> Result<()> {
        let mut handlers = self.notification_handlers.write().await;
        let req_handlers = self.request_handlers.read().await;
        for method in NotificationService::<H>::handled_methods().await {
            let owned_method = method.to_string();
            if handlers.contains_key(&owned_method) || req_handlers.contains_key(&owned_method) {
                return Err(NeomacsError::Unhandled(anyhow!(
                    "Handler for RPC method {} already defined",
                    method
                )));
            } else {
                handlers.insert(owned_method, service.sender());
            }
        }
        Ok(())
    }
    pub fn start(&self) {
        let is_shutdown = self.is_shutdown.clone();
        let request_rx = self.request_rx.clone();
        let connections = self.connections.clone();
        tokio::spawn(async move {
            loop {
                if *is_shutdown.lock() {
                    break;
                }
                if let Some((connection_id, request, tx)) = request_rx.lock().await.recv().await {
                    let connections = connections.read().await;
                    let res =
                        async {
                            let conn = connections.get(&connection_id).ok_or(
                                NeomacsError::DoesNotExist(format!("Connection {}", connection_id)),
                            )?;
                            conn.request(request).await
                        }
                        .await;
                    if let Err(_) = tx.send(res) {
                        error!("Error returning response from connection {}", connection_id)
                    }
                }
            }
        });
        let is_shutdown = self.is_shutdown.clone();
        let notification_rx = self.notification_rx.clone();
        let connections = self.connections.clone();
        tokio::spawn(async move {
            loop {
                if *is_shutdown.lock() {
                    break;
                }
                if let Some((connection_id, notification, tx)) =
                    notification_rx.lock().await.recv().await
                {
                    let connections = connections.read().await;
                    let res =
                        async {
                            let conn = connections.get(&connection_id).ok_or(
                                NeomacsError::DoesNotExist(format!("Connection {}", connection_id)),
                            )?;
                            conn.notify(notification).await
                        }
                        .await;
                    if let Err(_) = tx.send(res) {
                        error!("Error returning response from connection {}", connection_id)
                    }
                }
            }
        });
        let socket = self.socket.clone();
        let is_shutdown = self.is_shutdown.clone();
        let connections = self.connections.clone();
        let request_handlers = self.request_handlers.clone();
        let notification_handlers = self.notification_handlers.clone();
        let id_counter = self.id_counter.clone();
        tokio::spawn(async move {
            loop {
                if *is_shutdown.lock() {
                    break;
                }
                match socket.lock().await.accept().await {
                    Ok(conn) => {
                        let codec = MessageCodec::new();
                        let framed = Framed::new(conn, codec);
                        let id = id_counter.inc() as u64;
                        let connection = Connection::new(
                            id,
                            framed,
                            request_handlers.clone(),
                            notification_handlers.clone(),
                        );
                        connection.start();
                        connections.write().await.insert(id, connection);
                    }
                    Err(e) => {
                        error!("Error accepting socket connection: {}", e);
                    }
                }
            }
        });
    }

    pub fn comms(&self) -> ClientComms {
        ClientComms {
            request_tx: self.request_tx.clone(),
            notification_tx: self.notification_tx.clone(),
        }
    }

    pub async fn terminate(&mut self) {
        *self.is_shutdown.lock() = true;
        let mut connections = self.connections.write().await;
        for conn in connections.values_mut() {
            conn.terminate().await;
        }
    }
}

/// A handle to send requests and notifications to connected clients
#[derive(Clone)]
pub struct ClientComms {
    request_tx: mpsc::Sender<(u64, Request, oneshot::Sender<Result<Response>>)>,
    notification_tx: mpsc::Sender<(u64, Notification, oneshot::Sender<Result<()>>)>,
}

impl ClientComms {
    pub async fn request(&self, connection_id: u64, request: Request) -> Result<Response> {
        let (tx, rx) = oneshot::channel();
        wrap_err(self.request_tx.send((connection_id, request, tx)).await)?;
        wrap_err(rx.await)?
    }

    pub async fn notify(&self, connection_id: u64, notification: Notification) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        wrap_err(
            self.notification_tx
                .send((connection_id, notification, tx))
                .await,
        )?;
        wrap_err(rx.await)?
    }
}

struct Connection<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    id: u64,
    message_stream: broadcast::Sender<Message>,
    framed_read: Arc<tokio::sync::Mutex<SplitStream<Framed<C, MessageCodec>>>>,
    framed_write: Arc<tokio::sync::Mutex<SplitSink<Framed<C, MessageCodec>, Message>>>,
    is_shutdown: Arc<Mutex<bool>>,
    request_handlers: Arc<RwLock<HashMap<String, RequestHandleSender>>>,
    notification_handlers: Arc<RwLock<HashMap<String, NotificationHandleSender>>>,
}

impl<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> Connection<C> {
    pub fn new(
        id: u64,
        framed: Framed<C, MessageCodec>,
        request_handlers: Arc<RwLock<HashMap<String, RequestHandleSender>>>,
        notification_handlers: Arc<RwLock<HashMap<String, NotificationHandleSender>>>,
    ) -> Self {
        let (write, read) = framed.split();
        let (message_stream, _) = broadcast::channel(256);
        Self {
            id,
            message_stream,
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
        let message_stream = self.message_stream.clone();
        let mut message_stream_rx = message_stream.subscribe();
        let id = self.id;
        tokio::spawn(async move {
            loop {
                if *is_shutdown.lock() {
                    break;
                }
                if let Some(msg_res) = Self::get_next_message(framed_read.clone()).await {
                    match msg_res {
                        Ok(message) => {
                            if let Err(e) = message_stream.send(message) {
                                error!("Error broadcasting incoming RPC message: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Error reading data from open connection: {}", e);
                        }
                    }
                }
            }
        });
        tokio::spawn(async move {
            while let Ok(message) = message_stream_rx.recv().await {
                let request_handlers = request_handlers.clone();
                let notification_handlers = notification_handlers.clone();
                let framed_write = framed_write.clone();
                tokio::spawn(async move {
                    let result = match message {
                        Message::Request(request) => {
                            info!(
                                "Incoming request, method: {}, msg_id: {}",
                                request.method.as_str(),
                                request.msg_id
                            );
                            let request_handlers = request_handlers.read().await;
                            if let Some(handler) = request_handlers.get(&request.method) {
                                async {
                                    let (tx, rx) = oneshot::channel();
                                    wrap_err(
                                        handler.send((RequestContext::new(id), request, tx)).await,
                                    )?;
                                    match time::timeout(Duration::from_secs(5), rx).await? {
                                        Ok(result) => match result {
                                            Ok(response) => {
                                                Self::send_response(framed_write.clone(), response)
                                                    .await?;
                                                Ok(())
                                            }
                                            Err(e) => Err(e),
                                        },
                                        Err(e) => {
                                            Err(NeomacsError::Unhandled(anyhow::Error::new(e)))
                                        }
                                    }
                                }
                                .await
                            } else {
                                let err_response = ErrorResponse::new(
                                    ErrorType::UnknownMethod,
                                    format!("Unknown method {}", request.method.as_str()),
                                )
                                .into_response(request.msg_id);
                                async {
                                    Self::send_response(framed_write.clone(), err_response).await?;
                                    Err(NeomacsError::RequestError(format!(
                                        "No request handler registered for notification {}",
                                        request.method.as_str()
                                    )))
                                }
                                .await
                            }
                        }
                        Message::Notification(notification) => {
                            info!(
                                "Incoming notification, method: {}",
                                notification.method.as_str()
                            );
                            let notification_handlers = notification_handlers.read().await;
                            if let Some(handler) = notification_handlers.get(&notification.method) {
                                async {
                                    let (tx, rx) = oneshot::channel();
                                    wrap_err(
                                        handler
                                            .send((RequestContext::new(id), notification, tx))
                                            .await,
                                    )?;
                                    match time::timeout(Duration::from_secs(5), rx).await? {
                                        Ok(result) => result,
                                        Err(e) => {
                                            Err(NeomacsError::Unhandled(anyhow::Error::new(e)))
                                        }
                                    }
                                }
                                .await
                            } else {
                                Err(NeomacsError::RequestError(format!(
                                    "No notification handler registered for notification {}",
                                    notification.method.as_str()
                                )))
                            }
                        }
                        Message::Response(response) => Err(NeomacsError::RequestError(format!(
                            "Received an unexpected response message with message id: {}",
                            response.msg_id
                        ))),
                    };
                    if let Err(e) = result {
                        error!("Unhandled error processing RPC message: {}", e);
                    };
                });
            }
        });
    }

    pub async fn request(&self, request: Request) -> Result<Response> {
        let req_msg_id = request.msg_id;
        {
            let mut framed_write = self.framed_write.lock().await;
            framed_write.send(Message::Request(request)).await?;
        }
        let mut message_rx = self.message_stream.subscribe();
        time::timeout(Duration::from_secs(3), async {
            let mut res = None;
            while let Ok(message) = message_rx.recv().await {
                match message {
                    Message::Response(response) if response.msg_id == req_msg_id => {
                        res = Some(response);
                    }
                    _ => {}
                }
            }
            Ok(res.unwrap())
        })
        .await?
    }

    pub async fn notify(&self, notification: Notification) -> Result<()> {
        let mut framed_write = self.framed_write.lock().await;
        framed_write
            .send(Message::Notification(notification))
            .await?;
        Ok(())
    }

    pub async fn terminate(&mut self) {
        *self.is_shutdown.lock() = true;
        self.framed_write
            .lock()
            .await
            .close()
            .await
            .expect("Error closing connection");
        info!("Terminated open connection with id {}", self.id)
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
