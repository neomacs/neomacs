use std::io::{self, Cursor};

use bytes::{Buf, BufMut, BytesMut};
use neomacs_proc_macros::EncodeValue;
use rmpv::{decode, encode, Value};
use tokio_util::codec::{Decoder, Encoder};

use crate::error::NeomacsError;
use crate::rpc::convert::EncodeValue;

const REQUEST_TYPE: u64 = 0;
const RESPONSE_TYPE: u64 = 1;
const NOTIFICATION_TYPE: u64 = 2;

#[derive(Debug, PartialEq, Clone)]
pub struct Request {
    pub msg_id: u64,
    pub method: String,
    pub params: Vec<Value>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Response {
    pub msg_id: u64,
    pub error: Option<Value>,
    pub result: Option<Value>,
}

impl Response {
    pub fn success(msg_id: u64, result: Option<Value>) -> Self {
        Self {
            msg_id,
            error: None,
            result,
        }
    }

    pub fn error(msg_id: u64, error: Option<Value>) -> Self {
        Self {
            msg_id,
            error,
            result: None,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Notification {
    pub method: String,
    pub params: Vec<Value>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

impl TryFrom<Value> for Message {
    type Error = NeomacsError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let arr = value.as_array().ok_or(NeomacsError::InvalidRPCMessage)?;
        if !(3..=4).contains(&arr.len()) {
            return Err(NeomacsError::InvalidRPCMessage);
        }
        let msg_type = arr[0].as_u64().ok_or(NeomacsError::InvalidRPCMessage)?;
        match msg_type {
            0 => {
                let msg_id = arr[1].as_u64().ok_or(NeomacsError::InvalidRPCMessage)?;
                let method = arr[2].as_str().ok_or(NeomacsError::InvalidRPCMessage)?;
                let params = arr[3].as_array().ok_or(NeomacsError::InvalidRPCMessage)?;
                Ok(Message::Request(Request {
                    msg_id,
                    method: method.to_string(),
                    params: params.to_vec(),
                }))
            }
            1 => {
                let msg_id = arr[1].as_u64().ok_or(NeomacsError::InvalidRPCMessage)?;
                let error = if arr[2].is_nil() {
                    None
                } else {
                    Some(arr[2].clone())
                };
                let result = if arr[3].is_nil() {
                    None
                } else {
                    Some(arr[3].clone())
                };
                Ok(Message::Response(Response {
                    msg_id,
                    error,
                    result,
                }))
            }
            2 => {
                let method = arr[1].as_str().ok_or(NeomacsError::InvalidRPCMessage)?;
                let params = arr[2].as_array().ok_or(NeomacsError::InvalidRPCMessage)?;
                Ok(Message::Notification(Notification {
                    method: method.to_string(),
                    params: params.to_vec(),
                }))
            }
            _ => Err(NeomacsError::InvalidRPCMessage)?,
        }
    }
}

impl From<Message> for Value {
    fn from(message: Message) -> Self {
        match message {
            Message::Request(Request {
                msg_id,
                method,
                params,
            }) => Value::Array(vec![
                Value::Integer(REQUEST_TYPE.into()),
                Value::Integer(msg_id.into()),
                Value::String(method.into()),
                Value::Array(params),
            ]),
            Message::Response(Response {
                msg_id,
                error,
                result,
            }) => Value::Array(vec![
                Value::Integer(RESPONSE_TYPE.into()),
                Value::Integer(msg_id.into()),
                match error {
                    None => Value::Nil,
                    Some(val) => val,
                },
                match result {
                    None => Value::Nil,
                    Some(val) => val,
                },
            ]),
            Message::Notification(Notification { method, params }) => Value::Array(vec![
                Value::Integer(NOTIFICATION_TYPE.into()),
                Value::String(method.into()),
                Value::Array(params),
            ]),
        }
    }
}

pub enum ErrorType {
    InvalidRequest,
    UnknownMethod,
}

impl EncodeValue for ErrorType {
    fn encode_value(&self) -> Value {
        Value::String(String::from(self).into())
    }
}

impl From<ErrorType> for String {
    fn from(err_type: ErrorType) -> Self {
        let ptr = &err_type;
        ptr.into()
    }
}

impl From<&ErrorType> for String {
    fn from(err_type: &ErrorType) -> Self {
        match err_type {
            ErrorType::InvalidRequest => "INVALID_REQUEST".to_string(),
            ErrorType::UnknownMethod => "UNKNOWN_METHOD".to_string(),
        }
    }
}

#[derive(EncodeValue)]
pub struct ErrorResponse {
    code: ErrorType,
    message: String,
}

impl ErrorResponse {
    pub fn new<M: Into<String>>(code: ErrorType, message: M) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn into_response(self, msg_id: u64) -> Response {
        Response::error(msg_id, Some(self.encode_value()))
    }
}

pub struct MessageCodec;

impl MessageCodec {
    pub fn new() -> Self {
        Self {}
    }
}

impl Decoder for MessageCodec {
    type Item = Message;
    type Error = NeomacsError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let res: Result<Option<Self::Item>, Self::Error>;
        let position = {
            let mut buf = Cursor::new(&src);
            let raw = decode::read_value(&mut buf);
            match raw {
                Ok(val) if val.is_array() => {
                    let message: Message = val.try_into()?;
                    res = Ok(Some(message));
                    buf.position().try_into().unwrap()
                }
                Ok(_) => {
                    res = Err(NeomacsError::InvalidRPCMessage);
                    0
                }
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    res = Ok(None);
                    0
                }
                Err(e) => {
                    res = Err(e.into());
                    0
                }
            }
        };
        src.advance(position);
        res
    }
}

impl Encoder<Message> for MessageCodec {
    type Error = NeomacsError;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let mut buf = Vec::new().writer();
        encode::write_value(&mut buf, &item.into())?;
        dst.put(buf.into_inner().as_ref());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, Write},
        time::Duration,
    };

    use assert_matches::assert_matches;
    use rmpv::Value;
    use tokio_test::io::Builder;
    use tokio_util::codec::{FramedRead, FramedWrite};

    use super::{Message, MessageCodec};
    use crate::{
        error::{NeomacsError, Result},
        rpc::codec::Request,
    };
    use futures::{sink::SinkExt, StreamExt};

    #[tokio::test]
    async fn test_encoder() -> Result<()> {
        let codec = MessageCodec::new();
        let write_buf: Cursor<Vec<u8>> = Cursor::new(Vec::new());
        let mut framed_write = FramedWrite::new(write_buf, codec);
        let msg = Message::Request(Request {
            msg_id: 123,
            method: "foobar".to_string(),
            params: vec![Value::String("baz".into())],
        });
        framed_write.send(msg.clone()).await?;
        let mut written_bytes = framed_write.get_ref().clone();
        written_bytes.set_position(0);
        assert_eq!(
            rmpv::decode::read_value(&mut written_bytes).unwrap(),
            msg.into()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_decode_valid_data() -> Result<()> {
        let buf = Vec::new();
        let mut cursor = Cursor::new(buf);
        let msg = Message::Request(Request {
            msg_id: 123,
            method: "foobar".to_string(),
            params: vec![Value::String("baz".into())],
        });
        rmpv::encode::write_value(&mut cursor, &msg.clone().into())?;
        let io = Builder::new().read(cursor.get_ref().as_slice()).build();
        let codec = MessageCodec::new();
        let mut framed_read = FramedRead::new(io, codec);
        let message = framed_read.next().await.unwrap().unwrap();
        assert_eq!(message, msg);
        Ok(())
    }

    #[tokio::test]
    async fn test_decode_partial_data() -> Result<()> {
        let buf = Vec::new();
        let mut cursor = Cursor::new(buf);
        let msg = Message::Request(Request {
            msg_id: 123,
            method: "foobar".to_string(),
            params: vec![Value::String("baz".into())],
        });
        rmpv::encode::write_value(&mut cursor, &msg.clone().into())?;
        let io = Builder::new()
            .read(&cursor.get_ref().as_slice()[0..2])
            .wait(Duration::from_millis(200))
            .read(&cursor.get_ref().as_slice()[2..])
            .build();
        let codec = MessageCodec::new();
        let mut framed_read = FramedRead::new(io, codec);
        let message = framed_read.next().await.unwrap().unwrap();
        assert_eq!(message, msg);
        Ok(())
    }

    #[tokio::test]
    async fn test_decode_overflowing_data() -> Result<()> {
        let buf = Vec::new();
        let mut cursor = Cursor::new(buf);
        let msg = Message::Request(Request {
            msg_id: 123,
            method: "foobar".to_string(),
            params: vec![Value::String("baz".into())],
        });
        rmpv::encode::write_value(&mut cursor, &msg.clone().into())?;
        cursor.write(&b"more bytes"[..])?;
        let io = Builder::new().read(cursor.get_ref().as_slice()).build();
        let codec = MessageCodec::new();
        let mut framed_read = FramedRead::new(io, codec);
        let message = framed_read.next().await.unwrap().unwrap();
        assert_eq!(message, msg);
        Ok(())
    }

    #[tokio::test]
    async fn test_decode_invalid_data() -> Result<()> {
        let buf = Vec::new();
        let mut cursor = Cursor::new(buf);
        cursor.write(&b"more bytes"[..])?;
        let io = Builder::new().read(cursor.get_ref().as_slice()).build();
        let codec = MessageCodec::new();
        let mut framed_read = FramedRead::new(io, codec);
        let message = framed_read.next().await.unwrap();
        assert_matches!(message, Err(NeomacsError::InvalidRPCMessage));
        Ok(())
    }
}
