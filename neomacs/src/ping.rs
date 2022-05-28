use async_trait::async_trait;
use rmpv::Value;

use crate::{
    error::Result,
    rpc::{
        codec::{ErrorResponse, ErrorType, Request, Response},
        handler::{RequestContext, RequestHandler},
    },
};

pub struct PingHandler;

impl PingHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for PingHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RequestHandler for PingHandler {
    fn handled_methods() -> Vec<&'static str> {
        vec!["ping"]
    }

    async fn handle(&mut self, context: RequestContext, request: &Request) -> Result<Response> {
        let response = match &request.params[..] {
            [Value::String(data)] => Response {
                msg_id: request.msg_id,
                error: None,
                result: Some(Value::String(
                    format!(
                        "Pong! connection id: {}, data: {}",
                        context.connection_id, data
                    )
                    .into(),
                )),
            },
            _ => ErrorResponse::new(ErrorType::InvalidRequest, "Ping requires a string param")
                .into_response(request.msg_id),
        };
        Ok(response)
    }
}
