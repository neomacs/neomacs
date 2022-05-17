use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{self, mpsc};

use crate::{error::NeomacsError, rpc::msgpack_rpc::RpcRequest};

pub enum Event {
    Shutdown,
}

impl Event {
    pub fn from_rpc_request(request: &RpcRequest) -> crate::error::Result<Self> {
        match request.method.as_str() {
            "Shutdown" => Ok(Event::Shutdown),
            unknown => Err(NeomacsError::RequestError(format!(
                "Unknown RPC method {}",
                unknown
            ))),
        }
    }
}

pub struct EventHandler {
    is_shutdown: Arc<parking_lot::Mutex<bool>>,
    shutdown_event_tx: Arc<parking_lot::Mutex<mpsc::Sender<bool>>>,
    sender: mpsc::Sender<Event>,
    receiver: Arc<sync::Mutex<mpsc::Receiver<Event>>>,
}

impl EventHandler {
    pub fn new(shutdown_event_tx: mpsc::Sender<bool>) -> Self {
        let (rx, tx) = mpsc::channel(256);
        Self {
            is_shutdown: Arc::new(parking_lot::Mutex::new(false)),
            sender: rx,
            receiver: Arc::new(sync::Mutex::new(tx)),
            shutdown_event_tx: Arc::new(parking_lot::Mutex::new(shutdown_event_tx)),
        }
    }

    pub fn sender(&self) -> mpsc::Sender<Event> {
        self.sender.clone()
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        let mut is_shutdown = self.is_shutdown.lock();
        *is_shutdown = true;
        Ok(())
    }

    pub fn start(&self) {
        let receiver = self.receiver.clone();
        let is_shutdown = self.is_shutdown.clone();
        let shutdown_event_tx = self.shutdown_event_tx.clone();
        tokio::spawn(async move {
            loop {
                if *is_shutdown.lock() {
                    break;
                };
                let mut event_rx = receiver.lock().await;
                if let Some(event) = event_rx.recv().await {
                    let shutdown_event_tx = shutdown_event_tx.lock().clone();
                    if let Err(_e) = Self::handle_event(&event, &shutdown_event_tx).await {
                        // TODO how should error here be handled?
                    }
                }
            }
        });
    }

    async fn handle_event(event: &Event, shutdown_event_tx: &mpsc::Sender<bool>) -> Result<()> {
        match event {
            Event::Shutdown => shutdown_event_tx.send(true).await?,
        };
        Ok(())
    }
}
