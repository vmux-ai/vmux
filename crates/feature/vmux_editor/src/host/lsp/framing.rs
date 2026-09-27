use std::io::{self, BufRead, Read, Write};

use serde_json::Value;

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

pub fn write_message<W: Write>(w: &mut W, msg: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(msg)?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()
}

pub fn read_message<R: BufRead>(r: &mut R) -> io::Result<Option<Value>> {
    let mut content_len: Option<usize> = None;
    let mut header_bytes = 0usize;
    loop {
        let mut line = String::new();
        let remaining = MAX_HEADER_BYTES.saturating_sub(header_bytes);
        if remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LSP header is too large",
            ));
        }
        let n = r
            .by_ref()
            .take(remaining.saturating_add(1) as u64)
            .read_line(&mut line)?;
        if n == 0 {
            return Ok(None);
        }
        if n > remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LSP header is too large",
            ));
        }
        header_bytes += n;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        let Some((name, value)) = trimmed.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            if content_len.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "duplicate Content-Length",
                ));
            }
            content_len = Some(value.trim().parse::<usize>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid Content-Length")
            })?);
        }
    }
    let len = content_len
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    if len > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "LSP message is too large",
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    let value =
        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn write_then_read_roundtrip() {
        let msg = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
        let mut buf = Vec::new();
        write_message(&mut buf, &msg).unwrap();
        let header = String::from_utf8(buf[..20].to_vec()).unwrap();
        assert!(header.starts_with("Content-Length: "), "got: {header}");
        let mut cur = Cursor::new(buf);
        let back = read_message(&mut cur).unwrap().unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn reads_two_messages_from_one_stream() {
        let mut buf = Vec::new();
        write_message(&mut buf, &json!({"id": 1})).unwrap();
        write_message(&mut buf, &json!({"id": 2})).unwrap();
        let mut cur = Cursor::new(buf);
        assert_eq!(read_message(&mut cur).unwrap().unwrap(), json!({"id": 1}));
        assert_eq!(read_message(&mut cur).unwrap().unwrap(), json!({"id": 2}));
        assert!(read_message(&mut cur).unwrap().is_none());
    }

    #[test]
    fn body_split_across_reads_is_reassembled() {
        let mut raw = Vec::new();
        write_message(&mut raw, &json!({"hello": "world", "n": 42})).unwrap();
        let mut cur = std::io::BufReader::with_capacity(4, Cursor::new(raw));
        let back = read_message(&mut cur).unwrap().unwrap();
        assert_eq!(back, json!({"hello": "world", "n": 42}));
    }

    #[test]
    fn missing_content_length_errors() {
        let mut cur = Cursor::new(b"\r\n{}".to_vec());
        assert!(read_message(&mut cur).is_err());
    }

    #[test]
    fn oversized_message_is_rejected_before_allocation() {
        let mut cur =
            Cursor::new(format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1).into_bytes());
        assert_eq!(
            read_message(&mut cur).unwrap_err().to_string(),
            "LSP message is too large"
        );
    }

    #[test]
    fn oversized_or_ambiguous_headers_are_rejected() {
        let mut oversized = Cursor::new(vec![b'x'; MAX_HEADER_BYTES + 1]);
        assert_eq!(
            read_message(&mut oversized).unwrap_err().to_string(),
            "LSP header is too large"
        );

        let mut duplicate =
            Cursor::new(b"Content-Length: 2\r\ncontent-length: 2\r\n\r\n{}".to_vec());
        assert_eq!(
            read_message(&mut duplicate).unwrap_err().to_string(),
            "duplicate Content-Length"
        );
    }
}
