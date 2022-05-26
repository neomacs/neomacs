use crate::{
    error::Result,
    ping::PingHandler,
    rpc::{
        handler::{NotificationHandler, NotificationService, RequestHandler, RequestService},
        tcp::TcpRpcSocket,
    },
};
use anyhow::anyhow;
use log::info;
use std::path::{Path, PathBuf};

use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpStream, UnixStream},
};

use crate::rpc::{
    server::{RpcServer, RpcSocket},
    unix::UnixRpcSocket,
};

pub struct App<C, S>
where
    C: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    S: RpcSocket<C> + Send + Sync + 'static,
{
    server: RpcServer<C, S>,
}

impl App<UnixStream, UnixRpcSocket> {
    pub async fn init_unix() -> Result<Self> {
        let path = unix_socket_path().await?;
        App::bind_unix(path)
    }

    fn bind_unix<P: AsRef<Path>>(path: P) -> Result<Self> {
        let socket = UnixRpcSocket::new(path)?;
        Ok(Self {
            server: RpcServer::new(socket),
        })
    }
}

impl App<TcpStream, TcpRpcSocket> {
    pub async fn init_tcp() -> Result<Self> {
        let addr = tcp_addr();
        App::bind_tcp(addr).await
    }

    async fn bind_tcp<A: Into<String> + Clone>(addr: A) -> Result<Self> {
        let socket = TcpRpcSocket::new(addr).await?;
        Ok(Self {
            server: RpcServer::new(socket),
        })
    }
}

impl<C, S> App<C, S>
where
    C: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    S: RpcSocket<C> + Send + Sync + 'static,
{
    pub async fn start(&mut self) -> Result<()> {
        self.register_request_service(PingHandler::new()).await?;
        self.server.start();
        tokio::signal::ctrl_c().await?;
        self.server.terminate().await;
        info!("Goodbye!");
        Ok(())
    }

    async fn register_request_service<H: RequestHandler + Send + Sync + 'static>(
        &mut self,
        handler: H,
    ) -> Result<RequestService<H>> {
        let service = RequestService::new(handler);
        service.start().await;
        self.server.register_request_handler(&service).await?;
        Ok(service)
    }

    async fn register_notification_service<H: NotificationHandler + Send + Sync + 'static>(
        &mut self,
        handler: H,
    ) -> Result<NotificationService<H>> {
        let service = NotificationService::new(handler);
        service.start().await;
        self.server.register_notification_handler(&service).await?;
        Ok(service)
    }
}

async fn unix_socket_path() -> Result<PathBuf> {
    // TODO make the socket path configurable
    let home = dirs::home_dir().ok_or(anyhow!("Could not find home directory!"))?;
    let socket_path = home.join(".neomacs.sock");
    if tokio::fs::metadata(socket_path.as_path()).await.is_ok() {
        info!(
            "Found existing Unix socket {:?}, deleting",
            socket_path.as_path()
        );
        tokio::fs::remove_file(socket_path.as_path()).await?;
    }
    Ok(socket_path)
}

fn tcp_addr() -> String {
    // TODO make the TCP address configurable
    "127.0.0.1:554433".to_string()
}
