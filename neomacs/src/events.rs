use crate::{
    error::{wrap_err, Result},
    rpc::{
        codec::{ErrorResponse, ErrorType, Notification},
        convert::DecodeValue,
        handler::RequestContext,
        server::ClientComms,
    },
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use futures::future;
use log::error;
use maplit::hashmap;
use neomacs_proc_macros::DecodeValue;
use rmpv::Value;
use tokio::sync::{self, mpsc};

use crate::rpc::{
    codec::{Request, Response},
    convert::EncodeValue,
    handler::RequestHandler,
};

const SUBSCRIBE: &str = "SUBSCRIBE";

#[derive(DecodeValue)]
struct SubscribeRequest {
    event_name: String,
}

#[derive(Clone)]
pub struct EventEmitter {
    event_tx: mpsc::Sender<(String, Value)>,
}

impl EventEmitter {
    pub async fn emit(&self, event_name: String, event_data: Value) -> Result<()> {
        wrap_err(self.event_tx.send((event_name, event_data)).await)?;
        Ok(())
    }
}

pub struct EventHandler {
    client_comms: ClientComms,
    event_tx: mpsc::Sender<(String, Value)>,
    event_rx: Arc<sync::Mutex<mpsc::Receiver<(String, Value)>>>,
    subscriptions: Arc<sync::RwLock<HashMap<String, HashSet<u64>>>>,
}

impl EventHandler {
    pub fn new(client_comms: ClientComms) -> Self {
        let (event_tx, event_rx) = mpsc::channel(256);
        Self {
            client_comms,
            event_tx,
            event_rx: Arc::new(sync::Mutex::new(event_rx)),
            subscriptions: Arc::new(sync::RwLock::new(HashMap::new())),
        }
    }

    /// Get a handle that can be used to emit events to connected clients
    pub fn emitter(&self) -> EventEmitter {
        EventEmitter {
            event_tx: self.event_tx.clone(),
        }
    }

    pub fn start(&self) {
        let event_rx = self.event_rx.clone();
        let subscriptions = self.subscriptions.clone();
        let client_comms = self.client_comms.clone();
        tokio::spawn(async move {
            while let Some((event_name, event_data)) = Self::get_next_event(event_rx.clone()).await
            {
                let subs = subscriptions.read().await;
                if let Some(client_ids) = subs.get(&event_name.to_string()) {
                    let results = future::join_all(client_ids.iter().map(|client_id| {
                        client_comms.notify(
                            *client_id,
                            Notification {
                                method: event_name.clone(),
                                params: vec![event_data.clone()],
                            },
                        )
                    }))
                    .await;
                    for result in results {
                        if let Err(e) = result {
                            error!("Error sending event notification to client: {}", e);
                        }
                    }
                }
            }
        });
    }

    async fn get_next_event(
        event_rx: Arc<sync::Mutex<mpsc::Receiver<(String, Value)>>>,
    ) -> Option<(String, Value)> {
        event_rx.lock().await.recv().await
    }

    pub async fn subscribe(&mut self, client_id: u64, event_name: &str) -> Result<()> {
        let mut subs = self.subscriptions.write().await;
        let clients = match subs.get_mut(&event_name.to_string()) {
            Some(c) => c,
            None => {
                let c = HashSet::new();
                subs.insert(event_name.to_string(), c);
                subs.get_mut(&event_name.to_string()).unwrap()
            }
        };
        clients.insert(client_id);
        Ok(())
    }
}

#[async_trait]
impl RequestHandler for EventHandler {
    fn handled_methods() -> Vec<&'static str> {
        vec![SUBSCRIBE]
    }

    async fn handle(&mut self, context: RequestContext, request: &Request) -> Result<Response> {
        if request.params.len() != 1 {
            return Ok(ErrorResponse::new(
                ErrorType::InvalidRequest,
                "subscribe only takes 1 parameter",
            )
            .into_response(request.msg_id));
        }
        let decoded: Result<SubscribeRequest> =
            DecodeValue::decode_value(request.params[0].clone());
        match decoded {
            Ok(req) => {
                self.subscribe(context.connection_id, req.event_name.as_str())
                    .await?;
                Ok(Response::success(
                    request.msg_id,
                    Some(hashmap! { "status".to_string() => "OK".to_string() }.encode_value()),
                ))
            }
            Err(_) => Ok(ErrorResponse::new(
                ErrorType::InvalidRequest,
                "Invalid subscribe request",
            )
            .into_response(request.msg_id)),
        }
    }
}
