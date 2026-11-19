use std::future::Future;
use std::pin::Pin;

use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{Mutex, broadcast, mpsc};
use vmux_api::protocol::{
    ClientMessage, ServiceMessage, SharedEvent, SharedMessage, SharedResponse,
};
use vmux_api::session::ClientOpId;
use vmux_profile::ServicePaths;
use zeroize::{Zeroize, Zeroizing};

use crate::framing::LengthPrefixed;
use crate::service_peer::LocalPeer;

pub type RemoteFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait ServiceProtocolDriver: Send + Sync {
    fn connect_local(
        &self,
        client: u64,
        peer: &LocalPeer,
        admitted: bool,
        outbound: mpsc::UnboundedSender<ServiceMessage>,
    ) -> Box<dyn ServiceProtocolConnection> {
        let _ = (client, peer, admitted);
        self.connect(outbound)
    }

    fn connect(
        &self,
        outbound: mpsc::UnboundedSender<ServiceMessage>,
    ) -> Box<dyn ServiceProtocolConnection>;
}

pub struct LocalProtocol {
    client: u64,
    driver: std::sync::Arc<dyn ServiceProtocolDriver>,
    peer: LocalPeer,
    admitted: bool,
}

impl LocalProtocol {
    pub fn new(
        client: u64,
        driver: std::sync::Arc<dyn ServiceProtocolDriver>,
        peer: LocalPeer,
        admitted: bool,
    ) -> Self {
        Self {
            client,
            driver,
            peer,
            admitted,
        }
    }
}

impl ServiceProtocolDriver for LocalProtocol {
    fn connect(
        &self,
        outbound: mpsc::UnboundedSender<ServiceMessage>,
    ) -> Box<dyn ServiceProtocolConnection> {
        self.driver
            .connect_local(self.client, &self.peer, self.admitted, outbound)
    }
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
    peer: LocalPeer,
    reader: Mutex<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: Mutex<tokio::net::unix::OwnedWriteHalf>,
}

impl ServiceConnection {
    pub async fn connect_checked(
        path: &std::path::Path,
        requirement: &crate::service_peer::CodeRequirement,
        uid: u32,
    ) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path).await?;
        let peer = crate::service_peer::LocalPeer::inspect(&stream, Some(requirement))?;
        if peer.uid() != uid || !peer.verified_code() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "local service signing identity does not match",
            ));
        }
        let (reader, writer) = stream.into_split();
        Ok(Self {
            peer,
            reader: Mutex::new(BufReader::new(reader)),
            writer: Mutex::new(writer),
        })
    }

    pub async fn connect() -> std::io::Result<Self> {
        let stream = UnixStream::connect(ServicePaths::current().socket()).await?;
        let peer = LocalPeer::inspect(&stream, None)?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            peer,
            reader: Mutex::new(BufReader::new(reader)),
            writer: Mutex::new(writer),
        })
    }

    pub fn peer(&self) -> &LocalPeer {
        &self.peer
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

#[cfg(all(test, target_os = "macos"))]
mod peer_tests {
    use super::*;
    use crate::service_peer::CodeRequirement;

    #[tokio::test]
    async fn checked_connection_authenticates_the_actual_server_before_any_client_payload() {
        let directory =
            std::env::temp_dir().join(format!("vmx235-checked-peer-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("service.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let expected = CodeRequirement::for_binary(&std::env::current_exe().unwrap()).unwrap();
        let (release, held) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ = held.await;
            drop(stream);
        });
        let connection =
            ServiceConnection::connect_checked(&path, &expected, unsafe { libc::geteuid() })
                .await
                .unwrap();
        drop(connection);
        release.send(()).unwrap();
        server.await.unwrap();
        std::fs::remove_file(&path).unwrap();
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let foreign = CodeRequirement::for_binary(std::path::Path::new("/bin/ls")).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            drop(stream);
        });
        let refused =
            ServiceConnection::connect_checked(&path, &foreign, unsafe { libc::geteuid() })
                .await
                .err()
                .unwrap();
        assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
        server.await.unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}

pub struct ServiceCodec;

impl ServiceCodec {
    const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;
    const CODEC: LengthPrefixed = LengthPrefixed::new(Self::MAX_FRAME_BYTES);

    pub async fn write_raw<W>(writer: &mut W, data: &[u8]) -> std::io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        Self::CODEC.write(writer, data).await
    }

    pub async fn read_raw<R>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>>
    where
        R: AsyncRead + Unpin,
    {
        Self::CODEC.read(reader).await
    }

    pub fn write_raw_blocking<W: std::io::Write>(
        writer: &mut W,
        data: &[u8],
    ) -> std::io::Result<()> {
        Self::CODEC.write_blocking(writer, data)
    }

    pub fn read_raw_blocking<R: std::io::Read>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>> {
        Self::CODEC.read_blocking(reader)
    }

    pub async fn write_client<W>(writer: &mut W, message: &ClientMessage) -> std::io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut bytes = EncodedFrame::default();
        rkyv::api::high::to_bytes_in::<_, rkyv::rancor::Error>(message, &mut bytes)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        Self::write_raw(writer, &bytes.0).await
    }

    pub async fn write_service<W>(writer: &mut W, message: &ServiceMessage) -> std::io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut bytes = EncodedFrame::default();
        rkyv::api::high::to_bytes_in::<_, rkyv::rancor::Error>(message, &mut bytes)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        Self::write_raw(writer, &bytes.0).await
    }

    pub async fn read_client<R>(reader: &mut R) -> std::io::Result<Option<ClientMessage>>
    where
        R: AsyncRead + Unpin,
    {
        let mut bytes = Zeroizing::new(Vec::new());
        if !Self::CODEC.read_into(reader, &mut bytes).await? {
            return Ok(None);
        }
        rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes)
            .map(Some)
            .map_err(|error| std::io::Error::other(error.to_string()))
    }

    pub async fn read_service<R>(reader: &mut R) -> std::io::Result<Option<ServiceMessage>>
    where
        R: AsyncRead + Unpin,
    {
        let mut bytes = Zeroizing::new(Vec::new());
        if !Self::CODEC.read_into(reader, &mut bytes).await? {
            return Ok(None);
        }
        rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes)
            .map(Some)
            .map_err(|error| std::io::Error::other(error.to_string()))
    }

    pub fn write_client_blocking<W: std::io::Write>(
        writer: &mut W,
        message: &ClientMessage,
    ) -> std::io::Result<()> {
        let mut bytes = EncodedFrame::default();
        rkyv::api::high::to_bytes_in::<_, rkyv::rancor::Error>(message, &mut bytes)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        Self::write_raw_blocking(writer, &bytes.0)
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

#[derive(Default)]
struct EncodedFrame(rkyv::util::AlignedVec);

impl rkyv::ser::Positional for EncodedFrame {
    fn pos(&self) -> usize {
        self.0.len()
    }
}

impl<E: rkyv::rancor::Source> rkyv::ser::Writer<E> for EncodedFrame {
    fn write(&mut self, bytes: &[u8]) -> Result<(), E> {
        let maximum = ServiceCodec::MAX_FRAME_BYTES;
        let Some(required) = self.0.len().checked_add(bytes.len()) else {
            return Err(E::new(std::io::Error::other(
                "service frame length overflow",
            )));
        };
        if required > maximum {
            return Err(E::new(std::io::Error::other(
                "service frame exceeds byte limit",
            )));
        }
        if required > self.0.capacity() {
            let capacity = required
                .max(self.0.capacity().saturating_mul(2))
                .min(maximum);
            let mut replacement = rkyv::util::AlignedVec::with_capacity(capacity);
            replacement.extend_from_slice(&self.0);
            self.0.as_mut_slice().zeroize();
            self.0 = replacement;
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
}

impl Drop for EncodedFrame {
    fn drop(&mut self) {
        self.0.as_mut_slice().zeroize();
    }
}

#[cfg(test)]
mod buffer_tests {
    use super::*;
    use vmux_api::ProcessId;

    #[tokio::test]
    async fn serialized_payload_survives_aligned_buffer_growth_and_service_decode() {
        let id = ProcessId::new();
        let data: Vec<u8> = (0..=255).cycle().take(8193).collect();
        let message = ClientMessage::ProcessInput {
            process_id: id,
            data: data.clone(),
        };
        let mut stream = Vec::new();
        ServiceCodec::write_client(&mut stream, &message)
            .await
            .unwrap();
        let length = u32::from_le_bytes(stream[..4].try_into().unwrap()) as usize;
        assert_eq!(length, stream.len() - 4);
        let mut reader = stream.as_slice();
        let decoded = ServiceCodec::read_client(&mut reader)
            .await
            .unwrap()
            .unwrap();
        match decoded {
            ClientMessage::ProcessInput {
                process_id,
                data: actual,
            } => {
                assert_eq!(process_id, id);
                assert_eq!(actual, data);
            }
            other => panic!("unexpected service input: {other:?}"),
        }
        assert!(reader.is_empty());
    }

    #[tokio::test]
    async fn a_serialized_message_over_the_wire_limit_emits_no_partial_frame() {
        let message = ClientMessage::ProcessInput {
            process_id: ProcessId::new(),
            data: vec![1; 64 * 1024 * 1024],
        };
        let mut stream = Vec::new();
        let result = ServiceCodec::write_client(&mut stream, &message).await;
        assert!(result.is_err());
        assert!(stream.is_empty());
    }
}
