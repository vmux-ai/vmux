use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > 64 * 1024 * 1024 {
        return Err(std::io::Error::other("frame too large"));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    Ok(Some(buf))
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
