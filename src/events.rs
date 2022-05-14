use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

pub enum Event {}

pub struct EventLoop {
    sender: mpsc::Sender<Event>,
    receiver: Arc<Mutex<mpsc::Receiver<Event>>>,
}

impl EventLoop {
    pub fn new() -> Self {
        let (rx, tx) = mpsc::channel(256);
        Self {
            sender: rx,
            receiver: Arc::new(Mutex::new(tx)),
        }
    }

    pub fn sender(&self) -> mpsc::Sender<Event> {
        self.sender.clone()
    }

    pub fn start_loop(&mut self) {
        let receiver = self.receiver.clone();
        tokio::spawn(async move {
            let mut receiver = receiver.lock().await;
            while let Some(event) = receiver.recv().await {
                // TODO what arguments should the handler recieve?
                handle_event(&event);
            }
        });
    }
}

fn handle_event(event: &Event) {
    // TODO
}
