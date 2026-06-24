use std::io::{Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};

/// Stream `url` to `dest`, calling `progress(downloaded, total)` as bytes arrive.
/// Blocking — run on a worker thread.
pub fn download_to(
    url: &str,
    dest: &Path,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let mut resp = reqwest::blocking::get(url).map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("http {}", resp.status()));
    }
    let total = resp.content_length();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 8192];
    let mut downloaded = 0u64;
    loop {
        let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        downloaded += n as u64;
        progress(downloaded, total);
    }
    Ok(())
}

/// Lowercase hex sha256 of a file's contents.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher).map_err(|e| e.to_string())?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Minimal one-shot HTTP server returning `body`; returns its base URL.
    fn serve_once(body: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut req = [0u8; 1024];
                let _ = stream.read(&mut req);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
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
        // sha256("hello vmux lsp")
        let sum = sha256_file(&dest).unwrap();
        assert_eq!(sum.len(), 64);
    }
}
