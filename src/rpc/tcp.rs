use async_trait::async_trait;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};

use super::server::RpcSocket;
use crate::error::Result;

pub struct TcpRpcSocket {
    listener: TcpListener,
}

impl TcpRpcSocket {
    pub async fn new<A: ToSocketAddrs>(addr: A) -> Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self { listener })
    }
}

#[async_trait]
impl RpcSocket<TcpStream> for TcpRpcSocket {
    async fn accept(&self) -> Result<TcpStream> {
        let (stream, _) = self.listener.accept().await?;
        Ok(stream)
    }
}
