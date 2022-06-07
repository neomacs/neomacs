use std::error::Error as StdError;
use std::io;
use std::result::Result as StdResult;
use thiserror::Error;

use crate::command::{Type, Value};

#[derive(Debug, Error)]
pub enum NeomacsError {
    #[error("Invalid request: {0}")]
    RequestError(String),
    #[error("{0} does not exist")]
    DoesNotExist(String),
    #[error("{0} already exists")]
    AlreadyExists(String),
    #[error("Wrong params passed to {command}: expected {expected_input:?}, got {input:?}")]
    InvalidCommandInput {
        command: String,
        expected_input: &'static [Type],
        input: Vec<Value>,
    },
    #[error("Invalid MessagePack RPC message")]
    InvalidRPCMessage,
    #[error("Invalid split value: {0}")]
    InvalidSplit(f64),
    #[error("Timeout error")]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[error("IO error")]
    IO(#[from] io::Error),
    #[error("Error parsing MessagePack value: {0}")]
    MessagePackParse(String),
    #[error("MessagePack decode error")]
    MessagePackDecode(#[from] rmpv::decode::Error),
    #[error("MessagePack encode error")]
    MessagePackEncode(#[from] rmpv::encode::Error),
    #[error(transparent)]
    Unhandled(#[from] anyhow::Error),
}

impl NeomacsError {
    pub fn invalid_command_input<S: Into<String>>(
        command: S,
        expected_input: &'static [Type],
        input: Vec<Value>,
    ) -> Self {
        Self::InvalidCommandInput {
            command: command.into(),
            expected_input,
            input,
        }
    }
}

pub type Result<T> = StdResult<T, NeomacsError>;

/// Wraps an arbitrary std::error::Error into a NeomacsError::Unhandled.
pub fn wrap_err<T, E: StdError + Send + Sync + 'static>(result: StdResult<T, E>) -> Result<T> {
    match result {
        Ok(v) => Ok(v),
        Err(e) => Err(anyhow::Error::new(e).into()),
    }
}
