use async_trait::async_trait;
use rmpv::Value;

use crate::{
    error::Result,
    rpc::{
        codec::{ErrorResponse, ErrorType, Request, Response},
        handler::RequestHandler,
    },
};

pub struct PingHandler;

impl PingHandler {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl RequestHandler for PingHandler {
    fn handled_methods() -> Vec<&'static str> {
        vec!["ping"]
    }

    async fn handle(&mut self, request: &Request) -> Result<Response> {
        let response = match &request.params[..] {
            [Value::String(data)] => Response {
                msg_id: request.msg_id,
                error: None,
                result: Some(Value::String(format!("Pong! data: {}", data).into())),
            },
            _ => Response {
                msg_id: request.msg_id,
                error: Some(
                    ErrorResponse::new(ErrorType::InvalidRequest, "Ping requires a string param")
                        .into(),
                ),
                result: None,
            },
        };
        Ok(response)
    }
}
