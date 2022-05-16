use std::{sync::Arc, time::Duration};

use anyhow::Result;
use tokio::time;

const REDRAW_FPS: f32 = 60.0;

pub struct Renderer {
    is_shutdown: Arc<parking_lot::Mutex<bool>>,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            is_shutdown: Arc::new(parking_lot::Mutex::new(false)),
        }
    }

    pub fn start(&self) {
        let mut interval = time::interval(Duration::from_secs(1).div_f32(REDRAW_FPS));
        let is_shutdown = self.is_shutdown.clone();
        tokio::spawn(async move {
            loop {
                if *is_shutdown.lock() {
                    break;
                };
                interval.tick().await;
                Self::render();
            }
        });
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        let mut is_shutdown = self.is_shutdown.lock();
        *is_shutdown = true;
        Ok(())
    }

    fn render() {
        // TODO
    }
}
