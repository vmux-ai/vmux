use std::future::Future;
use std::pin::Pin;

use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{Mutex, broadcast, mpsc};
use vmux_api::conversation::ClientOpId;
use vmux_api::protocol::{
    ClientMessage, ServiceMessage, SharedEvent, SharedMessage, SharedResponse,
};
use vmux_profile::ServicePaths;

use crate::framing::LengthPrefixed;

const CODEC: LengthPrefixed = LengthPrefixed::new(64 * 1024 * 1024);

pub type RemoteFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait ServiceProtocolDriver: Send + Sync {
    fn connect(
        &self,
        outbound: mpsc::UnboundedSender<ServiceMessage>,
    ) -> Box<dyn ServiceProtocolConnection>;
}

pub trait ServiceProtocolConnection: Send {
    fn dispatch(&mut self, message: ClientMessage) -> RemoteFuture<'_, Result<(), ClientMessage>>;

    fn disconnect(&mut self) -> RemoteFuture<'_, ()> {
        Box::pin(async {})
    }
}

pub trait RemoteDriver: Send + Sync {
    fn dispatch(&self, request: SharedMessage) -> RemoteFuture<'_, SharedResponse>;
    fn subscription(&self, request: &SharedMessage) -> Option<String>;
    fn subscribe(
        &self,
        sid: String,
    ) -> RemoteFuture<'_, Option<broadcast::Receiver<ServiceMessage>>>;
    fn resolve(&self, sid: String, event: SharedEvent) -> RemoteFuture<'_, Option<SharedEvent>>;
    fn snapshot(&self, sid: String) -> RemoteFuture<'_, Option<SharedEvent>>;
}

pub trait RemoteOperationStore: Send + Sync {
    fn claim(&self, id: ClientOpId) -> RemoteFuture<'_, bool>;
    fn release(&self, id: ClientOpId) -> RemoteFuture<'_, ()>;
}

pub struct ServiceConnection {
    reader: Mutex<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: Mutex<tokio::net::unix::OwnedWriteHalf>,
}

impl ServiceConnection {
    pub async fn connect() -> std::io::Result<Self> {
        let stream = UnixStream::connect(ServicePaths::current().socket()).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: Mutex::new(BufReader::new(reader)),
            writer: Mutex::new(writer),
        })
    }

    pub async fn send(&self, message: &ClientMessage) -> std::io::Result<()> {
        let mut writer = self.writer.lock().await;
        ServiceCodec::write_client(&mut *writer, message).await
    }

    pub async fn recv(&self) -> std::io::Result<Option<ServiceMessage>> {
        let mut reader = self.reader.lock().await;
        ServiceCodec::read_service(&mut *reader).await
    }
}

pub struct ServiceCodec;

impl ServiceCodec {
    pub async fn write_raw<W>(writer: &mut W, data: &[u8]) -> std::io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        CODEC.write(writer, data).await
    }

    pub async fn read_raw<R>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>>
    where
        R: AsyncRead + Unpin,
    {
        CODEC.read(reader).await
    }

    pub fn write_raw_blocking<W: std::io::Write>(
        writer: &mut W,
        data: &[u8],
    ) -> std::io::Result<()> {
        CODEC.write_blocking(writer, data)
    }

    pub fn read_raw_blocking<R: std::io::Read>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>> {
        CODEC.read_blocking(reader)
    }

    pub async fn write_client<W>(writer: &mut W, message: &ClientMessage) -> std::io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(message)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        Self::write_raw(writer, &bytes).await
    }

    pub async fn write_service<W>(writer: &mut W, message: &ServiceMessage) -> std::io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(message)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        Self::write_raw(writer, &bytes).await
    }

    pub async fn read_client<R>(reader: &mut R) -> std::io::Result<Option<ClientMessage>>
    where
        R: AsyncRead + Unpin,
    {
        let Some(bytes) = Self::read_raw(reader).await? else {
            return Ok(None);
        };
        rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes)
            .map(Some)
            .map_err(|error| std::io::Error::other(error.to_string()))
    }

    pub async fn read_service<R>(reader: &mut R) -> std::io::Result<Option<ServiceMessage>>
    where
        R: AsyncRead + Unpin,
    {
        let Some(bytes) = Self::read_raw(reader).await? else {
            return Ok(None);
        };
        rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes)
            .map(Some)
            .map_err(|error| std::io::Error::other(error.to_string()))
    }

    pub fn write_client_blocking<W: std::io::Write>(
        writer: &mut W,
        message: &ClientMessage,
    ) -> std::io::Result<()> {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(message)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        Self::write_raw_blocking(writer, &bytes)
    }

    pub fn read_service_blocking<R: std::io::Read>(
        reader: &mut R,
    ) -> std::io::Result<Option<ServiceMessage>> {
        let Some(bytes) = Self::read_raw_blocking(reader)? else {
            return Ok(None);
        };
        rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes)
            .map(Some)
            .map_err(|error| std::io::Error::other(error.to_string()))
    }
}
