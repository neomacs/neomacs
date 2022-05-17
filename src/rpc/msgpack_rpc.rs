use std::io::Cursor;

use rmpv::Value;

use crate::error::{NeomacsError, Result};

pub struct RpcRequest {
    pub msg_id: u64,
    pub method: String,
    pub params: Vec<Value>,
}

impl RpcRequest {
    pub fn from_bytes(bytes: &mut [u8]) -> Result<Self> {
        let raw_req = rmpv::decode::read_value(&mut Cursor::new(bytes))?;
        let req_array = raw_req.as_array().ok_or_else(invalid_request)?;
        if req_array.len() != 4 {
            return Err(invalid_request());
        }
        let is_request = match req_array[0].as_u64() {
            None => false,
            Some(0) => true,
            Some(_) => false,
        };
        if !is_request {
            return Err(invalid_request());
        }
        let msg_id = req_array[0].as_u64().ok_or_else(invalid_request)?;
        let method = req_array[1].as_str().ok_or_else(invalid_request)?;
        let params = req_array[2].as_array().ok_or_else(invalid_request)?;
        Ok(Self {
            msg_id,
            method: method.to_string(),
            params: params.to_vec(),
        })
    }
}

pub struct RpcResponse {
    msg_id: u32,
    error: Option<Value>,
    result: Option<Value>,
}

impl RpcResponse {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let val = Value::Array(vec![
            Value::Integer(1.into()),
            Value::Integer(self.msg_id.into()),
            match &self.error {
                None => Value::Nil,
                Some(v) => v.clone(),
            },
            match &self.result {
                None => Value::Nil,
                Some(v) => v.clone(),
            },
        ]);
        let mut writer = Cursor::new(Vec::new());
        rmpv::encode::write_value(&mut writer, &val)?;
        Ok(writer.into_inner())
    }
}

fn invalid_request() -> NeomacsError {
    NeomacsError::RequestError("Invalid RPC request".to_string())
}
