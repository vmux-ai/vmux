//! The unix socket's framing, which is [`LengthPrefixed`] with this transport's limit chosen.
//!
//! The codec itself lives in `vmux_remote` so the relay can link it without dragging `vmux_wire`
//! and `vmux_profile` along. What belongs here is the policy: how large a frame this particular
//! socket will believe.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vmux_remote::framing::LengthPrefixed;

/// Generous, because both ends of this socket are already on the machine: an oversized frame here
/// costs a memory copy, not a stranger's licence to allocate. A QUIC stream carrying the same
/// rkyv messages picks a far smaller number for exactly that reason.
const CODEC: LengthPrefixed = LengthPrefixed::new(64 * 1024 * 1024);

/// Write a length-prefixed frame to an async writer.
pub async fn write_raw_frame<W>(writer: &mut W, data: &[u8]) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    CODEC.write(writer, data).await
}

/// Read a length-prefixed frame from an async reader.
///
/// `None` means the stream ended between frames. A frame cut short is an error — see
/// [`LengthPrefixed::read`] for why the two must not be the same answer.
pub async fn read_raw_frame<R>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>>
where
    R: AsyncReadExt + Unpin,
{
    CODEC.read(reader).await
}

/// Write a length-prefixed frame to a blocking writer.
pub fn write_raw_frame_blocking<W: std::io::Write>(
    writer: &mut W,
    data: &[u8],
) -> std::io::Result<()> {
    CODEC.write_blocking(writer, data)
}

/// Read a length-prefixed frame from a blocking reader.
///
/// `None` means the stream ended between frames.
pub fn read_raw_frame_blocking<R: std::io::Read>(
    reader: &mut R,
) -> std::io::Result<Option<Vec<u8>>> {
    CODEC.read_blocking(reader)
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
///
/// Yields `Ok(None)` at a clean end of stream and `Err` for everything else, including a frame cut
/// short. Every failure is *returned*, never propagated out of the caller behind its back — a
/// caller with cleanup after its read loop has to be able to see the error and still run it.
#[macro_export]
macro_rules! read_message {
    ($reader:expr, $ty:ty) => {{
        match $crate::framing::read_raw_frame($reader).await {
            Ok(Some(bytes)) => rkyv::from_bytes::<$ty, rkyv::rancor::Error>(&bytes)
                .map(Some)
                .map_err(|e| std::io::Error::other(e.to_string())),
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        }
    }};
}

/// Read a length-prefixed frame (blocking), deserialize from rkyv bytes.
///
/// Returns rather than propagates, for the reason in [`read_message`].
#[macro_export]
macro_rules! read_message_blocking {
    ($reader:expr, $ty:ty) => {{
        match $crate::framing::read_raw_frame_blocking($reader) {
            Ok(Some(bytes)) => rkyv::from_bytes::<$ty, rkyv::rancor::Error>(&bytes)
                .map(Some)
                .map_err(|e| std::io::Error::other(e.to_string())),
            Ok(None) => Ok::<Option<$ty>, std::io::Error>(None),
            Err(error) => Err(error),
        }
    }};
}
