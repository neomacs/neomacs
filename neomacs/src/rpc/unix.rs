use std::path::Path;

use crate::error::Result;
use async_trait::async_trait;
use log::info;
use tokio::net::{UnixListener, UnixStream};

use super::server::RpcSocket;

pub struct UnixRpcSocket {
    listener: UnixListener,
}

impl UnixRpcSocket {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let listener = UnixListener::bind(path.as_ref())?;
        info!("Listening on Unix socket {:?}", path.as_ref());
        Ok(Self { listener })
    }
}

#[async_trait]
impl RpcSocket<UnixStream> for UnixRpcSocket {
    async fn accept(&self) -> Result<UnixStream> {
        let (stream, addr) = self.listener.accept().await?;
        info!("Accepted new Unix socket connection on {:?}", addr);
        Ok(stream)
    }
}
