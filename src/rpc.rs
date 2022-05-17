use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use tokio::{net::UnixDatagram, sync::mpsc};

use crate::{error::Result, events::Event};

use self::msgpack_rpc::RpcRequest;

pub mod msgpack_rpc;

#[async_trait]
trait RpcSocket {
    async fn recv(&self) -> Result<Vec<u8>>;
    async fn send(&self, buf: &[u8]) -> Result<()>;
}

struct UnixRpcSocket {
    socket: UnixDatagram,
}

impl UnixRpcSocket {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let socket = UnixDatagram::bind(path)?;
        Ok(Self { socket })
    }
}

#[async_trait]
impl RpcSocket for UnixRpcSocket {
    async fn recv(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.socket.recv(buf.as_mut()).await?;
        Ok(buf)
    }

    async fn send(&self, buf: &[u8]) -> Result<()> {
        self.socket.send(buf).await?;
        Ok(())
    }
}

// TODO add a TcpRpcSocket implementation for non-Unix platforms

struct RpcServer<S: RpcSocket + Send + Sync + 'static> {
    socket: Arc<S>,
    is_shutdown: Arc<parking_lot::Mutex<bool>>,
    event_sender: mpsc::Sender<Event>,
}

impl<S: RpcSocket + Send + Sync + 'static> RpcServer<S> {
    pub fn new(socket: S, event_sender: mpsc::Sender<Event>) -> Self {
        Self {
            socket: Arc::new(socket),
            is_shutdown: Arc::new(parking_lot::Mutex::new(false)),
            event_sender,
        }
    }

    pub fn start(&self) {
        let is_shutdown = self.is_shutdown.clone();
        let socket = self.socket.clone();
        tokio::spawn(async move {
            loop {
                if *is_shutdown.lock() {
                    break;
                }
                let res: Result<()> = async {
                    let mut request = socket.recv().await?;
                    let rpc_request = RpcRequest::from_bytes(request.as_mut())?;
                    let event = Event::from_rpc_request(&rpc_request)?;
                    // send it to the event handler
                    // listen for event response
                    // serialize to RPC response bytes
                    // send on socket
                    Ok(())
                }.await;
                if let Err(e) = res {
                    // TODO handle this error
                    // If we have a request id, construct an RPC response
                    // Otherwise... ?
                }
            }
        });
    }

    pub fn shutdown(&mut self) {
        let mut is_shutdown = self.is_shutdown.lock();
        *is_shutdown = true;
    }
}
