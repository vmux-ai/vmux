use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vmux_remote::framing::LengthPrefixed;

const CODEC: LengthPrefixed = LengthPrefixed::new(64 * 1024 * 1024);

pub async fn write_raw_frame<W>(writer: &mut W, data: &[u8]) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    CODEC.write(writer, data).await
}

pub async fn read_raw_frame<R>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>>
where
    R: AsyncReadExt + Unpin,
{
    CODEC.read(reader).await
}

pub fn write_raw_frame_blocking<W: std::io::Write>(
    writer: &mut W,
    data: &[u8],
) -> std::io::Result<()> {
    CODEC.write_blocking(writer, data)
}

pub fn read_raw_frame_blocking<R: std::io::Read>(
    reader: &mut R,
) -> std::io::Result<Option<Vec<u8>>> {
    CODEC.read_blocking(reader)
}

#[macro_export]
macro_rules! write_message_blocking {
    ($writer:expr, $msg:expr) => {{
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>($msg)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        $crate::framing::write_raw_frame_blocking($writer, &bytes)
    }};
}

#[macro_export]
macro_rules! write_message {
    ($writer:expr, $msg:expr) => {{
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>($msg)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        $crate::framing::write_raw_frame($writer, &bytes).await
    }};
}

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
