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
