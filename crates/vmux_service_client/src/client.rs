use crate::protocol::{ClientMessage, ServiceMessage};
use crate::{read_message, socket_path, write_message};
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::sync::Mutex;

/// Async client connection to the vmux service.
/// Wraps the Unix socket with framing/serialization.
pub struct ServiceConnection {
    reader: Mutex<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: Mutex<tokio::net::unix::OwnedWriteHalf>,
}

impl ServiceConnection {
    /// Connect to the service socket.
    pub async fn connect() -> std::io::Result<Self> {
        let sock = socket_path();
        let stream = UnixStream::connect(&sock).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: Mutex::new(BufReader::new(reader)),
            writer: Mutex::new(writer),
        })
    }

    /// Send a message to the service.
    pub async fn send(&self, message: &ClientMessage) -> std::io::Result<()> {
        let mut writer = self.writer.lock().await;
        write_message!(&mut *writer, message)
    }

    /// Receive a message from the service. Returns `None` on disconnect.
    pub async fn recv(&self) -> std::io::Result<Option<ServiceMessage>> {
        let mut reader = self.reader.lock().await;
        read_message!(&mut *reader, ServiceMessage)
    }
}
