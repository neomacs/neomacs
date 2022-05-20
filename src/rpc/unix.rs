use std::path::Path;

use crate::error::Result;
use async_trait::async_trait;
use tokio::net::{UnixListener, UnixStream};

use super::server::RpcSocket;

pub struct UnixRpcSocket {
    listener: UnixListener,
}

impl UnixRpcSocket {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let listener = UnixListener::bind(path)?;
        Ok(Self { listener })
    }
}

#[async_trait]
impl RpcSocket<UnixStream> for UnixRpcSocket {
    async fn accept(&self) -> Result<UnixStream> {
        let (stream, _) = self.listener.accept().await?;
        Ok(stream)
    }
}
