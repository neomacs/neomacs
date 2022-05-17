use anyhow::Result;
use tokio::{signal, sync::mpsc};

use crate::{events, renderer, state::{StateManager, AppState}};

pub struct App {
    event_handler: events::EventHandler,
    renderer: renderer::Renderer,
    shutdown_from_event_rx: mpsc::Receiver<bool>,
}

impl App {
    pub fn new() -> Self {
        let state_mgr = StateManager::managed_state(AppState::new());
        let (shutdown_from_event_tx, shutdown_from_event_rx) = mpsc::channel(16);
        let event_handler = events::EventHandler::new(shutdown_from_event_tx);
        let renderer = renderer::Renderer::new(state_mgr.state());
        Self {
            event_handler,
            renderer,
            shutdown_from_event_rx,
        }
    }

    pub async fn start(&mut self) {
        self.event_handler.start();
        self.renderer.start();
        tokio::select! {
            _ = signal::ctrl_c() => {},
            _ = self.shutdown_from_event_rx.recv() => {}
        };
        self.shutdown().await.expect("Failed to shut down cleanly");
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.event_handler.shutdown().await?;
        self.renderer.shutdown().await?;
        Ok(())
    }
}
