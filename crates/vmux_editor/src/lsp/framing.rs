use std::io::{self, BufRead, Write};

use serde_json::Value;

/// Write a single JSON-RPC message with a `Content-Length` header.
pub fn write_message<W: Write>(w: &mut W, msg: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(msg)?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()
}

/// Read a single JSON-RPC message. Returns `Ok(None)` on clean EOF.
pub fn read_message<R: BufRead>(r: &mut R) -> io::Result<Option<Value>> {
    let mut content_len: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line)?;
        if n == 0 {
            return Ok(None); // EOF
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break; // end of headers
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            content_len = rest.trim().parse::<usize>().ok();
        }
    }
    let len = content_len
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    let value = serde_json::from_slice(&buf)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
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
        assert!(read_message(&mut cur).unwrap().is_none()); // EOF
    }

    #[test]
    fn body_split_across_reads_is_reassembled() {
        // BufReader with a tiny capacity forces read_exact to loop.
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
}
