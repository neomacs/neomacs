use crate::error::Result;
use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use neomacs_convert::TryFromValue;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::rpc::{
    codec::{Request, Response},
    handler::RequestHandler,
    server::{RpcServer, RpcSocket},
};

const SUBSCRIBE: &'static str = "SUBSCRIBE";

#[derive(TryFromValue)]
struct SubscribeRequest {
    client_id: u64,
    event_name: String,
}

impl Into<rmpv::Value> for SubscribeRequest {
    fn into(self) -> rmpv::Value {
        rmpv::Value::Map(vec![
            (
                rmpv::Value::String("client_id".into()),
                self.client_id.into(),
            ),
            (
                rmpv::Value::String("event_name".into()),
                self.event_name.into(),
            ),
        ])
    }
}

struct EventEmitter<'a, C, S>
where
    C: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    S: RpcSocket<C> + Send + Sync + 'static,
{
    rpc_server: &'a RpcServer<C, S>,
    subscriptions: Arc<parking_lot::RwLock<HashMap<usize, String>>>,
}

#[async_trait]
impl<'a, C, S> RequestHandler for EventEmitter<'a, C, S>
where
    C: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    S: RpcSocket<C> + Send + Sync + 'static,
{
    fn handled_methods() -> Vec<&'static str> {
        vec![SUBSCRIBE]
    }

    async fn handle(&mut self, request: &Request) -> Result<Response> {
        let req: SubscribeRequest = request.params.get(0).unwrap().clone().try_into()?;
        todo!()
    }
}
