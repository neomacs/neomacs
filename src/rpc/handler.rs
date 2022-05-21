use std::sync::Arc;

use async_trait::async_trait;
use log::error;
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::error::Result;

use super::codec::{Notification, Request, Response};

pub type RequestHandleReceiver = mpsc::Receiver<(Request, oneshot::Sender<Result<Response>>)>;
pub type RequestHandleSender = mpsc::Sender<(Request, oneshot::Sender<Result<Response>>)>;
pub type NotificationHandleReceiver = mpsc::Receiver<(Notification, oneshot::Sender<Result<()>>)>;
pub type NotificationHandleSender = mpsc::Sender<(Notification, oneshot::Sender<Result<()>>)>;

#[async_trait]
pub trait RequestHandler {
    fn handled_methods() -> Vec<&'static str>;
    async fn handle(&mut self, request: &Request) -> Result<Response>;
}

pub struct RequestService<H: RequestHandler + Send + Sync + 'static> {
    handler: Arc<Mutex<H>>,
    receiver: Arc<Mutex<RequestHandleReceiver>>,
    sender: RequestHandleSender,
}

impl<H: RequestHandler + Send + Sync + 'static> RequestService<H> {
    pub fn new(handler: H) -> Self {
        let (tx, rx) = mpsc::channel(64);
        Self {
            handler: Arc::new(Mutex::new(handler)),
            receiver: Arc::new(Mutex::new(rx)),
            sender: tx,
        }
    }

    pub async fn handled_methods() -> Vec<&'static str> {
        H::handled_methods()
    }

    pub async fn start(&self) {
        let handler = self.handler.clone();
        let receiver = self.receiver.clone();
        tokio::spawn(async move {
            while let Some((req, respond)) = Self::get_next_request(receiver.clone()).await {
                let res = handler.lock().await.handle(&req).await;
                if let Err(_) = respond.send(res) {
                    error!("Error sending response, msg_id: {}", req.msg_id);
                }
            }
        });
    }

    async fn get_next_request(
        receiver: Arc<Mutex<RequestHandleReceiver>>,
    ) -> Option<(Request, oneshot::Sender<Result<Response>>)> {
        receiver.lock().await.recv().await
    }

    pub fn sender(&self) -> RequestHandleSender {
        self.sender.clone()
    }
}

#[async_trait]
pub trait NotificationHandler {
    fn handled_methods() -> Vec<&'static str>;
    async fn handle(&mut self, notification: &Notification) -> Result<()>;
}

pub struct NotificationService<H: NotificationHandler + Send + Sync + 'static> {
    handler: Arc<Mutex<H>>,
    receiver: Arc<Mutex<NotificationHandleReceiver>>,
    sender: NotificationHandleSender,
}

impl<H: NotificationHandler + Send + Sync + 'static> NotificationService<H> {
    pub fn new(handler: H) -> Self {
        let (tx, rx) = mpsc::channel(64);
        Self {
            handler: Arc::new(Mutex::new(handler)),
            receiver: Arc::new(Mutex::new(rx)),
            sender: tx,
        }
    }

    pub async fn handled_methods() -> Vec<&'static str> {
        H::handled_methods()
    }

    pub async fn start(&self) {
        let handler = self.handler.clone();
        let receiver = self.receiver.clone();
        tokio::spawn(async move {
            while let Some((req, respond)) = Self::get_next_notification(receiver.clone()).await {
                let res = handler.lock().await.handle(&req).await;
                if let Err(_) = respond.send(res) {
                    error!(
                        "Error sending notification response, method: {}",
                        req.method.as_str()
                    );
                }
            }
        });
    }

    async fn get_next_notification(
        receiver: Arc<Mutex<NotificationHandleReceiver>>,
    ) -> Option<(Notification, oneshot::Sender<Result<()>>)> {
        receiver.lock().await.recv().await
    }

    pub fn sender(&self) -> NotificationHandleSender {
        self.sender.clone()
    }
}
