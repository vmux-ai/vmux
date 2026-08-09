use super::*;
use std::net::TcpListener;

fn serve_once(body: &'static [u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut req = [0u8; 1024];
            let _ = stream.read(&mut req);
            let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(body);
        }
    });
    format!("http://{addr}/file")
}

#[test]
fn downloads_and_hashes() {
    let url = serve_once(b"hello vmux lsp");
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.bin");
    let mut last = 0u64;
    download_to(&url, &dest, |d, _| last = d).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"hello vmux lsp");
    assert_eq!(last, 14);
    let sum = sha256_file(&dest).unwrap();
    assert_eq!(sum.len(), 64);
}
