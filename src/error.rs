use std::error::Error as StdError;
use std::result::Result as StdResult;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NeomacsError {
    #[error("{0} does not exist")]
    DoesNotExist(String),
    #[error(transparent)]
    Unhandled(#[from] anyhow::Error),
}

pub type Result<T> = StdResult<T, NeomacsError>;

/// Wraps an arbitrary std::error::Error into a NeomacsError::Unhandled.
pub fn wrap_err<T, E: StdError + Send + Sync + 'static>(result: StdResult<T, E>) -> Result<T> {
    match result {
        Ok(v) => Ok(v),
        Err(e) => Err(anyhow::Error::new(e).into()),
    }
}
