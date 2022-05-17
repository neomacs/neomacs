use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;
use tokio::sync::{mpsc, oneshot};

use crate::{
    buffer::Buffer,
    error::{wrap_err, NeomacsError, Result},
};

/// An instruction to change some piece of application state.
#[derive(Debug)]
pub enum Mutation {
    /// Make the named buffer the current buffer
    SwitchBuffer { name: String },
}

/// Global application state
pub struct AppState {
    buffers: HashMap<String, Buffer>,
    current_buffer: String,
}

pub struct StateManager {
    mutate_tx: mpsc::Sender<(Mutation, oneshot::Sender<Result<()>>)>,
    state: Arc<RwLock<AppState>>,
}

impl StateManager {
    pub fn managed_state(state: AppState) -> Self {
        let (mutate_tx, mut mutate_rx) = mpsc::channel(128);
        let state = Arc::new(RwLock::new(state));
        let instance = Self {
            mutate_tx,
            state: state.clone(),
        };
        tokio::spawn(async move {
            while let Some((mutation, result_tx)) = mutate_rx.recv().await {
                let state = state.clone();
                let res = match mutation {
                    Mutation::SwitchBuffer { name } => {
                        let mut state = state.write();
                        if state.buffers.contains_key(&name) {
                            state.current_buffer = name;
                            Ok(())
                        } else {
                            Err(NeomacsError::DoesNotExist(format!("Buffer {}", name)))
                        }
                    }
                };
                result_tx
                    .send(res)
                    .expect("Failed to send mutation result!");
            }
        });
        instance
    }

    pub fn state(&self) -> Arc<RwLock<AppState>> {
        self.state.clone()
    }

    pub async fn mutate(&self, mutation: Mutation) -> Result<()> {
        let (result_tx, result_rx) = oneshot::channel();
        wrap_err(self.mutate_tx.send((mutation, result_tx)).await)?;
        wrap_err(result_rx.await)?
    }
}
