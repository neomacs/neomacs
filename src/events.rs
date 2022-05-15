use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, oneshot, Mutex};

pub enum Event {
    Shutdown,
}

pub struct EventHandler {
    shutdown_event_tx: mpsc::Sender<bool>,
    shutdown_rx: Arc<Mutex<mpsc::Receiver<oneshot::Sender<bool>>>>,
    sender: mpsc::Sender<Event>,
    receiver: Arc<Mutex<mpsc::Receiver<Event>>>,
}

impl EventHandler {
    pub fn new(
        shutdown_event_tx: mpsc::Sender<bool>,
        shutdown_rx: mpsc::Receiver<oneshot::Sender<bool>>,
    ) -> Self {
        let (rx, tx) = mpsc::channel(256);
        Self {
            sender: rx,
            receiver: Arc::new(Mutex::new(tx)),
            shutdown_event_tx,
            shutdown_rx: Arc::new(Mutex::new(shutdown_rx)),
        }
    }

    pub fn sender(&self) -> mpsc::Sender<Event> {
        self.sender.clone()
    }

    pub fn start_loop(self) {
        let receiver = self.receiver.clone();
        let shutdown = self.shutdown_rx.clone();
        tokio::spawn(async move {
            loop {
                let mut event_rx = receiver.lock().await;
                let mut shutdown_rx = shutdown.lock().await;
                tokio::select! {
                    Some(event) = event_rx.recv() => {
                        if let Err(_e) = self.handle_event(&event).await {
                            // TODO how should error here be handled?
                        }
                    },
                    Some(confirm_tx) = shutdown_rx.recv() => {
                        confirm_tx.send(true).expect("Unable to send shutdown confirmation");
                        break;
                    },
                    else => panic!("Unexpected async event")
                }
            }
        });
    }

    async fn handle_event(&self, event: &Event) -> Result<()> {
        match event {
            Event::Shutdown => self.shutdown_event_tx.send(true).await?,
        };
        Ok(())
    }
}
