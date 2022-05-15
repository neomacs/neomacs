use std::{sync::Arc, time::Duration};

use tokio::{
    signal,
    sync::{mpsc, oneshot, Mutex},
    time::timeout,
};

pub mod buffer;
pub mod events;
pub mod redraw;

pub struct App {
    event_loop: events::EventLoop,
    event_loop_shutdown_tx: Arc<Mutex<mpsc::Sender<oneshot::Sender<bool>>>>,
    shutdown_from_event_rx: Arc<Mutex<mpsc::Receiver<bool>>>,
}

impl App {
    pub fn new() -> Self {
        let (event_loop_shutdown_tx, event_loop_shutdown_rx) = mpsc::channel(16);
        let (shutdown_from_event_tx, shutdown_from_event_rx) = mpsc::channel(16);
        let event_loop = events::EventLoop::new(shutdown_from_event_tx, event_loop_shutdown_rx);
        Self {
            event_loop,
            event_loop_shutdown_tx: Arc::new(Mutex::new(event_loop_shutdown_tx)),
            shutdown_from_event_rx: Arc::new(Mutex::new(shutdown_from_event_rx)),
        }
    }

    pub fn start_main_loop(&'static mut self) {
        self.event_loop.start_loop();
        redraw::start_redraw_loop();
        let shutdown_from_event_rx = self.shutdown_from_event_rx.clone();
        let event_loop_shutdown_tx = self.event_loop_shutdown_tx.clone();
        tokio::spawn(async move {
            let mut shutdown_from_event = shutdown_from_event_rx.lock().await;
            tokio::select! {
                _ = signal::ctrl_c() => {},
                _ = shutdown_from_event.recv() => {}
            };
            let event_loop_shutdown = event_loop_shutdown_tx.lock().await;
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            event_loop_shutdown
                .send(shutdown_tx)
                .await
                .expect("Unable to shut down event loop.");
            timeout(Duration::from_millis(300), shutdown_rx)
                .await
                .expect("Timed out waiting for event loop to shut down.")
                .expect("Error receiving shutdown confirmation from event loop.");
        });
    }
}
