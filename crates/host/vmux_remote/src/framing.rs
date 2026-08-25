use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::quic::{FRAME_VERSION, MessageType};

#[derive(Clone, Copy, Debug)]
pub struct LengthPrefixed {
    max_frame_bytes: usize,
}

impl LengthPrefixed {
    pub const fn new(max_frame_bytes: usize) -> Self {
        Self { max_frame_bytes }
    }

    pub async fn write<W>(self, writer: &mut W, body: &[u8]) -> std::io::Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        writer.write_all(&(body.len() as u32).to_le_bytes()).await?;
        writer.write_all(body).await?;
        writer.flush().await
    }

    pub async fn read<R>(self, reader: &mut R) -> std::io::Result<Option<Vec<u8>>>
    where
        R: AsyncReadExt + Unpin,
    {
        let mut length = [0u8; 4];
        if !Self::fill_or_end(reader, &mut length).await? {
            return Ok(None);
        }
        let mut body = vec![0u8; self.length_of(length)?];
        reader.read_exact(&mut body).await?;
        Ok(Some(body))
    }

    async fn fill_or_end<R>(reader: &mut R, buffer: &mut [u8]) -> std::io::Result<bool>
    where
        R: AsyncReadExt + Unpin,
    {
        match reader.read(&mut buffer[..1]).await {
            Ok(0) => return Ok(false),
            Ok(_) => {}
            Err(error) if Self::peer_gone(&error) => return Ok(false),
            Err(error) => return Err(error),
        }
        reader.read_exact(&mut buffer[1..]).await?;
        Ok(true)
    }

    fn fill_or_end_blocking<R: std::io::Read>(
        reader: &mut R,
        buffer: &mut [u8],
    ) -> std::io::Result<bool> {
        match reader.read(&mut buffer[..1]) {
            Ok(0) => return Ok(false),
            Ok(_) => {}
            Err(error) if Self::peer_gone(&error) => return Ok(false),
            Err(error) => return Err(error),
        }
        reader.read_exact(&mut buffer[1..])?;
        Ok(true)
    }

    pub fn write_blocking<W: std::io::Write>(
        self,
        writer: &mut W,
        body: &[u8],
    ) -> std::io::Result<()> {
        writer.write_all(&(body.len() as u32).to_le_bytes())?;
        writer.write_all(body)?;
        writer.flush()
    }

    pub fn read_blocking<R: std::io::Read>(
        self,
        reader: &mut R,
    ) -> std::io::Result<Option<Vec<u8>>> {
        let mut length = [0u8; 4];
        if !Self::fill_or_end_blocking(reader, &mut length)? {
            return Ok(None);
        }
        let mut body = vec![0u8; self.length_of(length)?];
        reader.read_exact(&mut body)?;
        Ok(Some(body))
    }

    fn length_of(self, prefix: [u8; 4]) -> std::io::Result<usize> {
        let length = u32::from_le_bytes(prefix) as usize;
        if length > self.max_frame_bytes {
            return Err(std::io::Error::other(format!(
                "frame of {length} bytes over the {} byte limit",
                self.max_frame_bytes
            )));
        }
        Ok(length)
    }

    fn peer_gone(error: &std::io::Error) -> bool {
        matches!(
            error.kind(),
            std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::BrokenPipe
        )
    }
}

#[derive(Debug)]
pub enum FrameError {
    UnsupportedVersion(u8),
    UnexpectedType(MessageType),
    Truncated,
    Malformed,
    Io(std::io::Error),
}

impl From<std::io::Error> for FrameError {
    fn from(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            return Self::Truncated;
        }
        Self::Io(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub message_type: MessageType,
    pub body: Vec<u8>,
}

impl Frame {
    pub fn new(message_type: MessageType, body: Vec<u8>) -> Self {
        Self { message_type, body }
    }

    pub fn json<T: serde::Serialize>(
        message_type: MessageType,
        value: &T,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self::new(message_type, serde_json::to_vec(value)?))
    }

    pub fn body_of(&self, want: MessageType) -> Result<&[u8], FrameError> {
        if self.message_type != want {
            return Err(FrameError::UnexpectedType(self.message_type));
        }
        Ok(&self.body)
    }

    pub fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        want: MessageType,
    ) -> Result<T, FrameError> {
        serde_json::from_slice(self.body_of(want)?).map_err(|_| FrameError::Malformed)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FrameStream {
    codec: LengthPrefixed,
}

impl FrameStream {
    pub const fn new(max_body_bytes: usize) -> Self {
        Self {
            codec: LengthPrefixed::new(max_body_bytes),
        }
    }

    pub async fn open<W>(self, writer: &mut W, frame: &Frame) -> std::io::Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        writer.write_all(&[FRAME_VERSION]).await?;
        self.send(writer, frame).await
    }

    pub async fn send<W>(self, writer: &mut W, frame: &Frame) -> std::io::Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        writer
            .write_all(&frame.message_type.0.to_le_bytes())
            .await?;
        self.codec.write(writer, &frame.body).await
    }

    pub async fn accept<R>(self, reader: &mut R) -> Result<Frame, FrameError>
    where
        R: AsyncReadExt + Unpin,
    {
        let mut version = [0u8; 1];
        reader.read_exact(&mut version).await?;
        if version[0] != FRAME_VERSION {
            return Err(FrameError::UnsupportedVersion(version[0]));
        }
        match self.next(reader).await? {
            Some(frame) => Ok(frame),
            None => Err(FrameError::Truncated),
        }
    }

    pub async fn next<R>(self, reader: &mut R) -> Result<Option<Frame>, FrameError>
    where
        R: AsyncReadExt + Unpin,
    {
        let mut message_type = [0u8; 2];
        if !LengthPrefixed::fill_or_end(reader, &mut message_type).await? {
            return Ok(None);
        }
        let Some(body) = self.codec.read(reader).await? else {
            return Err(FrameError::Truncated);
        };
        Ok(Some(Frame::new(
            MessageType(u16::from_le_bytes(message_type)),
            body,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, ReadBuf};

    const CODEC: LengthPrefixed = LengthPrefixed::new(64 * 1024);

    struct FailsWith(Option<ErrorKind>);

    impl AsyncRead for FailsWith {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            let kind = self.0.take().expect("reader polled again after error");
            Poll::Ready(Err(std::io::Error::from(kind)))
        }
    }

    impl std::io::Read for FailsWith {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            let kind = self.0.take().expect("reader read again after error");
            Err(std::io::Error::from(kind))
        }
    }

    #[tokio::test]
    async fn a_frame_survives_the_round_trip() {
        let mut wire = Vec::new();
        CODEC
            .write(&mut wire, b"\x00opaque\xff")
            .await
            .expect("write");

        let read = CODEC.read(&mut wire.as_slice()).await.expect("read");

        assert_eq!(read.as_deref(), Some(&b"\x00opaque\xff"[..]));
    }

    #[tokio::test]
    async fn a_frame_cut_short_is_not_a_stream_that_ended() {
        let mut whole = Vec::new();
        CODEC
            .write(&mut whole, b"twelve bytes")
            .await
            .expect("write");

        let ended = CODEC.read(&mut &b""[..]).await;
        let body_cut = CODEC.read(&mut &whole[..whole.len() - 1]).await;
        let body_absent = CODEC.read(&mut &whole[..4]).await;
        let prefix_cut = CODEC.read(&mut &whole[..3]).await;

        assert!(
            matches!(ended, Ok(None)),
            "nothing at all is a stream that ended, got {ended:?}"
        );
        assert!(
            body_cut.is_err() && body_absent.is_err(),
            "a promised body that did not arrive must not read as a stream that ended, \
             got {body_cut:?} and {body_absent:?}"
        );
        assert!(
            prefix_cut.is_err(),
            "a length prefix cut mid-way lost bytes too — `read_exact` reports an empty prefix \
             and a half-written one alike, and folding them together is how a peer that vanished \
             comes back as a clean end. Got {prefix_cut:?}"
        );
    }

    #[tokio::test]
    async fn a_message_type_cut_in_half_is_not_a_stream_that_ended() {
        let stream = FrameStream::new(64 * 1024);
        let mut wire = Vec::new();
        stream
            .open(
                &mut wire,
                &Frame::new(MessageType::SESSION_EVENT, b"first".to_vec()),
            )
            .await
            .expect("open");
        wire.push(0x05);

        let mut reader = wire.as_slice();
        stream.accept(&mut reader).await.expect("first frame");
        let after = stream.next(&mut reader).await;

        assert!(
            matches!(after, Err(FrameError::Truncated)),
            "half a message type is a truncated frame, not the end of the stream, got {after:?}"
        );
    }

    #[tokio::test]
    async fn a_peer_that_went_away_between_frames_ends_the_stream() {
        for kind in [
            ErrorKind::UnexpectedEof,
            ErrorKind::ConnectionReset,
            ErrorKind::ConnectionAborted,
            ErrorKind::BrokenPipe,
        ] {
            let read = CODEC.read(&mut FailsWith(Some(kind))).await;
            assert!(matches!(read, Ok(None)), "{kind:?} should end the stream");
        }

        let genuine = CODEC
            .read(&mut FailsWith(Some(ErrorKind::InvalidData)))
            .await;
        assert_eq!(
            genuine.expect_err("a real I/O error must surface").kind(),
            ErrorKind::InvalidData
        );
    }

    #[tokio::test]
    async fn a_frame_over_the_limit_is_refused_before_it_is_allocated() {
        let tiny = LengthPrefixed::new(16);
        let mut wire = Vec::new();
        CODEC.write(&mut wire, &[0u8; 17]).await.expect("write");

        assert!(tiny.read(&mut wire.as_slice()).await.is_err());
        assert!(
            tiny.read(&mut &wire[..4]).await.is_err(),
            "the prefix alone is enough to refuse; the body need never arrive"
        );
    }

    #[tokio::test]
    async fn one_legs_setup_is_refused_by_the_other_legs_reader() {
        use crate::quic::{PeerRole, RelaySetup};

        let stream = FrameStream::new(64 * 1024);
        let relay_setup = RelaySetup {
            device_id: crate::DeviceId::new("alpha"),
            role: PeerRole::Client,
            token: "a-relay-credential".into(),
        };
        let mut wire = Vec::new();
        stream
            .open(
                &mut wire,
                &Frame::json(MessageType::RELAY_SETUP, &relay_setup).expect("encode"),
            )
            .await
            .expect("write");

        let frame = stream.accept(&mut wire.as_slice()).await.expect("read");
        let as_session = frame.read_json::<crate::quic::ClientSetup>(MessageType::CLIENT_SETUP);

        assert!(
            matches!(
                as_session,
                Err(FrameError::UnexpectedType(MessageType::RELAY_SETUP))
            ),
            "a relay setup must not satisfy a session reader, got {as_session:?}"
        );
        assert!(
            frame
                .read_json::<RelaySetup>(MessageType::RELAY_SETUP)
                .is_ok(),
            "and the reader it was addressed to must still accept it"
        );
    }

    #[tokio::test]
    async fn a_stale_framing_is_refused_as_a_version_rather_than_as_rubbish() {
        let stream = FrameStream::new(64 * 1024);
        let mut wire = Vec::new();
        stream
            .open(&mut wire, &Frame::new(MessageType::CLIENT_SETUP, vec![]))
            .await
            .expect("write");
        wire[0] = FRAME_VERSION - 1;

        let accepted = stream.accept(&mut wire.as_slice()).await;

        assert!(
            matches!(accepted, Err(FrameError::UnsupportedVersion(v)) if v == FRAME_VERSION - 1),
            "got {accepted:?}"
        );
    }

    #[tokio::test]
    async fn a_stream_announces_itself_once_and_then_carries_many_frames() {
        let stream = FrameStream::new(64 * 1024);
        let mut wire = Vec::new();
        stream
            .open(
                &mut wire,
                &Frame::new(MessageType::SESSION_EVENTS, b"attach".to_vec()),
            )
            .await
            .expect("open");
        for event in [b"first".as_slice(), b"second".as_slice()] {
            stream
                .send(
                    &mut wire,
                    &Frame::new(MessageType::SESSION_EVENT, event.to_vec()),
                )
                .await
                .expect("send");
        }

        let mut reader = wire.as_slice();
        let opened = stream.accept(&mut reader).await.expect("accept");
        let mut bodies = Vec::new();
        while let Some(frame) = stream.next(&mut reader).await.expect("next") {
            bodies.push(
                frame
                    .body_of(MessageType::SESSION_EVENT)
                    .expect("typed")
                    .to_vec(),
            );
        }

        assert_eq!(opened.message_type, MessageType::SESSION_EVENTS);
        assert_eq!(bodies, vec![b"first".to_vec(), b"second".to_vec()]);
    }

    #[tokio::test]
    async fn an_unknown_field_degrades_instead_of_failing() {
        let stream = FrameStream::new(64 * 1024);
        let mut wire = Vec::new();
        stream
            .open(
                &mut wire,
                &Frame::new(
                    MessageType::CLIENT_SETUP,
                    br#"{"device_id":"d","token":"t","teleportation":true}"#.to_vec(),
                ),
            )
            .await
            .expect("write");

        let setup = stream
            .accept(&mut wire.as_slice())
            .await
            .expect("read")
            .read_json::<crate::quic::ClientSetup>(MessageType::CLIENT_SETUP)
            .expect("an unknown field must not stop it parsing");

        assert_eq!(setup.device_id, crate::DeviceId::new("d"));
    }

    #[test]
    fn the_blocking_twin_draws_the_same_line() {
        let mut whole = Vec::new();
        CODEC
            .write_blocking(&mut whole, b"twelve bytes")
            .expect("write");

        assert_eq!(
            CODEC
                .read_blocking(&mut whole.as_slice())
                .expect("read")
                .as_deref(),
            Some(&b"twelve bytes"[..])
        );
        assert!(matches!(
            CODEC.read_blocking(&mut FailsWith(Some(ErrorKind::ConnectionReset))),
            Ok(None)
        ));
        assert!(
            CODEC.read_blocking(&mut &whole[..whole.len() - 1]).is_err(),
            "a frame cut short must not read as a stream that ended"
        );
    }
}
