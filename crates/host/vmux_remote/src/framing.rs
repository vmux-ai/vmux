//! Length-prefixed frames, for a stream that carries more than one message.
//!
//! `u32` little-endian length, then that many bytes. Nothing here interprets the body — the same
//! framing carries rkyv over a local unix socket and over a QUIC stream, and neither transport is
//! named in this module.
//!
//! It lives in this crate rather than beside its first caller because the relay links
//! `vmux_remote` precisely for being free of domain types. A codec in `vmux_client` would drag
//! `vmux_wire` and `vmux_profile` behind it, and the relay must stay unable to decode a payload.
//!
//! Contrast the *control* frame in [`crate::quic`], which is one message per stream delimited by
//! the stream's own finish. This is the other shape: many messages, one long-lived stream, so a
//! boundary has to be written down.

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A stream of length-prefixed frames, bounded by what one frame may claim.
///
/// The bound is a field rather than a constant because one framing runs over two transports with
/// very different threat models: a local unix socket, where an oversized frame is a memory copy,
/// and a remote QUIC stream, where it is a peer's licence to make this process allocate. A single
/// constant would have to be the looser of the two.
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

    /// Read one frame, or `None` once the peer has stopped sending.
    ///
    /// `None` means the stream ended *between* frames, which is how a stream ordinarily ends. A
    /// peer that stopped part-way through one is an error and must not be folded into the same
    /// answer: a subscription that was reset mid-frame would otherwise read as one the desktop
    /// finished, and the caller would stop listening instead of reconnecting.
    pub async fn read<R>(self, reader: &mut R) -> std::io::Result<Option<Vec<u8>>>
    where
        R: AsyncReadExt + Unpin,
    {
        let mut length = [0u8; 4];
        match reader.read_exact(&mut length).await {
            Ok(_) => {}
            Err(error) if Self::peer_gone(&error) => return Ok(None),
            Err(error) => return Err(error),
        }
        let mut body = vec![0u8; self.length_of(length)?];
        reader.read_exact(&mut body).await?;
        Ok(Some(body))
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

    /// The blocking twin of [`LengthPrefixed::read`], with the same distinction between a stream
    /// that ended and a frame that was cut short.
    pub fn read_blocking<R: std::io::Read>(
        self,
        reader: &mut R,
    ) -> std::io::Result<Option<Vec<u8>>> {
        let mut length = [0u8; 4];
        match reader.read_exact(&mut length) {
            Ok(_) => {}
            Err(error) if Self::peer_gone(&error) => return Ok(None),
            Err(error) => return Err(error),
        }
        let mut body = vec![0u8; self.length_of(length)?];
        reader.read_exact(&mut body)?;
        Ok(Some(body))
    }

    /// What the prefix claims, once it is small enough to believe.
    ///
    /// Checked before the allocation it sizes, so a peer cannot spend this process's memory with
    /// four bytes and then hang up.
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

    /// Whether an error means the peer is gone rather than that something went wrong.
    ///
    /// A clean shutdown yields `UnexpectedEof`; a crashed peer that closes its socket with unread
    /// data resets the connection, so the next read fails with `ConnectionReset` on Linux where
    /// macOS delivers a clean EOF. Both are a normal end of stream — in the service, a read error
    /// that escaped here would skip client cleanup and orphan PTY children.
    ///
    /// Only ever consulted at a frame boundary. Part-way through a frame the same error kinds mean
    /// data was lost, which is the opposite conclusion.
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

    /// The distinction the whole type exists for. A peer that stopped part-way through a frame
    /// lost data; reporting that as "no more frames" is how a reset subscription reads as one the
    /// desktop finished, and the caller stops listening instead of reconnecting.
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
            matches!(prefix_cut, Ok(None)),
            "a prefix cut mid-way is a peer that stopped before promising anything, got \
             {prefix_cut:?}"
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

    /// Refused from the prefix alone, so four bytes cannot make this process allocate.
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
