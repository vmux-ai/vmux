use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// True for I/O errors that mean the peer is gone, so the stream has no more
/// frames. A clean shutdown yields `UnexpectedEof`; a crashed peer that closes
/// its socket with unread data resets the connection, so the next read fails
/// with `ConnectionReset` (Linux) instead of EOF (macOS delivers a clean EOF).
/// Callers must treat both as a normal end-of-stream — in the service, a read
/// error that escaped here would skip client cleanup and orphan PTY children.
fn is_peer_gone(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe
    )
}

/// Write a length-prefixed frame to an async writer.
pub async fn write_raw_frame<W>(writer: &mut W, data: &[u8]) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let len = data.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(data).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a length-prefixed frame from an async reader.
/// Returns `None` on clean EOF.
pub async fn read_raw_frame<R>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>>
where
    R: AsyncReadExt + Unpin,
{
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if is_peer_gone(&e) => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > 64 * 1024 * 1024 {
        return Err(std::io::Error::other("frame too large"));
    }
    let mut buf = vec![0u8; len];
    match reader.read_exact(&mut buf).await {
        Ok(_) => {}
        Err(e) if is_peer_gone(&e) => return Ok(None),
        Err(e) => return Err(e),
    }
    Ok(Some(buf))
}

/// Write a length-prefixed frame to a blocking writer.
pub fn write_raw_frame_blocking<W: std::io::Write>(
    writer: &mut W,
    data: &[u8],
) -> std::io::Result<()> {
    let len = data.len() as u32;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(data)?;
    writer.flush()?;
    Ok(())
}

/// Read a length-prefixed frame from a blocking reader.
/// Returns `None` on clean EOF.
pub fn read_raw_frame_blocking<R: std::io::Read>(
    reader: &mut R,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(_) => {}
        Err(e) if is_peer_gone(&e) => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > 64 * 1024 * 1024 {
        return Err(std::io::Error::other("frame too large"));
    }
    let mut buf = vec![0u8; len];
    match reader.read_exact(&mut buf) {
        Ok(_) => {}
        Err(e) if is_peer_gone(&e) => return Ok(None),
        Err(e) => return Err(e),
    }
    Ok(Some(buf))
}

/// Serialize a message to rkyv bytes, write as a length-prefixed frame (blocking).
#[macro_export]
macro_rules! write_message_blocking {
    ($writer:expr, $msg:expr) => {{
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>($msg)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        $crate::framing::write_raw_frame_blocking($writer, &bytes)
    }};
}

/// Serialize a message to rkyv bytes, write as a length-prefixed frame.
/// Use: `write_message(&mut writer, &my_msg).await?`
#[macro_export]
macro_rules! write_message {
    ($writer:expr, $msg:expr) => {{
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>($msg)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        $crate::framing::write_raw_frame($writer, &bytes).await
    }};
}

/// Read a length-prefixed frame, deserialize from rkyv bytes.
/// Use: `let msg: Option<MyMsg> = read_message!(reader)?;`
#[macro_export]
macro_rules! read_message {
    ($reader:expr, $ty:ty) => {{
        match $crate::framing::read_raw_frame($reader).await? {
            Some(bytes) => {
                let msg = rkyv::from_bytes::<$ty, rkyv::rancor::Error>(&bytes)
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                Ok::<Option<$ty>, std::io::Error>(Some(msg))
            }
            None => Ok(None),
        }
    }};
}

/// Read a length-prefixed frame (blocking), deserialize from rkyv bytes.
/// Use: `let msg: Option<MyMsg> = read_message_blocking!(reader, MyMsg)?;`
#[macro_export]
macro_rules! read_message_blocking {
    ($reader:expr, $ty:ty) => {{
        match $crate::framing::read_raw_frame_blocking($reader)? {
            Some(bytes) => {
                let msg = rkyv::from_bytes::<$ty, rkyv::rancor::Error>(&bytes)
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                Ok::<Option<$ty>, std::io::Error>(Some(msg))
            }
            None => Ok(None),
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, ReadBuf};

    struct AsyncErrReader(Option<ErrorKind>);

    impl AsyncRead for AsyncErrReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            let kind = self.0.take().expect("reader polled again after error");
            Poll::Ready(Err(std::io::Error::from(kind)))
        }
    }

    struct BlockingErrReader(Option<ErrorKind>);

    impl std::io::Read for BlockingErrReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            let kind = self.0.take().expect("reader read again after error");
            Err(std::io::Error::from(kind))
        }
    }

    #[tokio::test]
    async fn read_raw_frame_maps_connection_reset_to_clean_eof() {
        let mut reader = AsyncErrReader(Some(ErrorKind::ConnectionReset));
        let got = read_raw_frame(&mut reader)
            .await
            .expect("a reset peer must not surface as an error");
        assert!(
            got.is_none(),
            "a reset connection must read as end-of-stream, like a clean EOF"
        );
    }

    #[tokio::test]
    async fn read_raw_frame_propagates_non_disconnect_errors() {
        let mut reader = AsyncErrReader(Some(ErrorKind::InvalidData));
        let err = read_raw_frame(&mut reader)
            .await
            .expect_err("genuine I/O errors must still surface");
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn read_raw_frame_blocking_maps_connection_reset_to_clean_eof() {
        let mut reader = BlockingErrReader(Some(ErrorKind::ConnectionReset));
        let got = read_raw_frame_blocking(&mut reader)
            .expect("a reset peer must not surface as an error");
        assert!(got.is_none());
    }
}
