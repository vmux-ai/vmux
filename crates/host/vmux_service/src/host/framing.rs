use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vmux_api::protocol::{ClientMessage, ServiceMessage};
use vmux_transport::framing::LengthPrefixed;

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

pub async fn write_client_message<W>(writer: &mut W, message: &ClientMessage) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(message)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    write_raw_frame(writer, &bytes).await
}

pub async fn write_service_message<W>(
    writer: &mut W,
    message: &ServiceMessage,
) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(message)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    write_raw_frame(writer, &bytes).await
}

pub async fn read_client_message<R>(reader: &mut R) -> std::io::Result<Option<ClientMessage>>
where
    R: AsyncReadExt + Unpin,
{
    let Some(bytes) = read_raw_frame(reader).await? else {
        return Ok(None);
    };
    rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes)
        .map(Some)
        .map_err(|error| std::io::Error::other(error.to_string()))
}

pub async fn read_service_message<R>(reader: &mut R) -> std::io::Result<Option<ServiceMessage>>
where
    R: AsyncReadExt + Unpin,
{
    let Some(bytes) = read_raw_frame(reader).await? else {
        return Ok(None);
    };
    rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes)
        .map(Some)
        .map_err(|error| std::io::Error::other(error.to_string()))
}

pub fn write_client_message_blocking<W: std::io::Write>(
    writer: &mut W,
    message: &ClientMessage,
) -> std::io::Result<()> {
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(message)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    write_raw_frame_blocking(writer, &bytes)
}

pub fn read_service_message_blocking<R: std::io::Read>(
    reader: &mut R,
) -> std::io::Result<Option<ServiceMessage>> {
    let Some(bytes) = read_raw_frame_blocking(reader)? else {
        return Ok(None);
    };
    rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes)
        .map(Some)
        .map_err(|error| std::io::Error::other(error.to_string()))
}
