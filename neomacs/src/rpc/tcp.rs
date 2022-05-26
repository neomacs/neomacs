use async_trait::async_trait;
use log::info;
use tokio::net::{TcpListener, TcpStream};

use super::server::RpcSocket;
use crate::error::Result;

pub struct TcpRpcSocket {
    listener: TcpListener,
}

impl TcpRpcSocket {
    pub async fn new<A: Into<String> + Clone>(addr: A) -> Result<Self> {
        let listener = TcpListener::bind(addr.clone().into()).await?;
        info!("Listening on TCP address {}", addr.into());
        Ok(Self { listener })
    }
}

#[async_trait]
impl RpcSocket<TcpStream> for TcpRpcSocket {
    async fn accept(&self) -> Result<TcpStream> {
        let (stream, addr) = self.listener.accept().await?;
        info!("Accepted new TCP connection on {}", addr);
        Ok(stream)
    }
}
