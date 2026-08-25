use crate::paths::ServicePaths;
use crate::protocol::{ClientMessage, ServiceMessage};
use crate::{read_message, write_message};
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::sync::Mutex;

pub struct ServiceConnection {
    reader: Mutex<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: Mutex<tokio::net::unix::OwnedWriteHalf>,
}

impl ServiceConnection {
    pub async fn connect() -> std::io::Result<Self> {
        let sock = ServicePaths::current().socket();
        let stream = UnixStream::connect(&sock).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: Mutex::new(BufReader::new(reader)),
            writer: Mutex::new(writer),
        })
    }

    pub async fn send(&self, message: &ClientMessage) -> std::io::Result<()> {
        let mut writer = self.writer.lock().await;
        write_message!(&mut *writer, message)
    }

    pub async fn recv(&self) -> std::io::Result<Option<ServiceMessage>> {
        let mut reader = self.reader.lock().await;
        read_message!(&mut *reader, ServiceMessage)
    }
}
