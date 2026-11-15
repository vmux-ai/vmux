use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct UnixHttp {
    socket: PathBuf,
    timeout: Duration,
}

impl UnixHttp {
    pub(crate) fn new(path: &Path) -> Self {
        Self {
            socket: path.to_path_buf(),
            timeout: Duration::from_secs(2),
        }
    }

    pub(crate) fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub(crate) fn get(&self, path: &str) -> io::Result<Response> {
        self.request("GET", path, &[])
    }

    pub(crate) fn post(&self, path: &str, body: &[u8]) -> io::Result<Response> {
        self.request("POST", path, body)
    }

    pub(crate) fn delete(&self, path: &str) -> io::Result<Response> {
        self.request("DELETE", path, &[])
    }

    fn request(&self, method: &str, path: &str, body: &[u8]) -> io::Result<Response> {
        let mut stream = UnixStream::connect(&self.socket)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        write!(
            stream,
            "{method} {path} HTTP/1.0\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )?;
        stream.write_all(body)?;
        let mut bytes = Vec::new();
        stream.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP response is too large",
            ));
        }
        Response::parse(bytes)
    }
}

pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) body: Vec<u8>,
}

impl Response {
    fn parse(bytes: Vec<u8>) -> io::Result<Self> {
        if bytes.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before the HTTP response",
            ));
        }
        let boundary = bytes
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing HTTP headers"))?;
        let headers = std::str::from_utf8(&bytes[..boundary]).map_err(io::Error::other)?;
        let mut lines = headers.split("\r\n");
        let mut status_line = lines.next().unwrap_or_default().split_whitespace();
        let version = status_line.next().unwrap_or_default();
        if version != "HTTP/1.0" && version != "HTTP/1.1" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid HTTP version",
            ));
        }
        let status = status_line
            .next()
            .and_then(|code| code.parse::<u16>().ok())
            .filter(|code| (100..=599).contains(code))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP status"))?;
        let body = bytes[boundary + 4..].to_vec();
        for line in lines {
            let Some((key, value)) = line.split_once(':') else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid HTTP header",
                ));
            };
            if key.eq_ignore_ascii_case("content-length") {
                let length = value.trim().parse::<usize>().map_err(io::Error::other)?;
                if length != body.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "truncated HTTP body",
                    ));
                }
            }
            if key.eq_ignore_ascii_case("transfer-encoding") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unsupported HTTP transfer encoding",
                ));
            }
        }
        Ok(Self { status, body })
    }

    pub(crate) fn require_success(self) -> io::Result<Vec<u8>> {
        if (200..300).contains(&self.status) {
            return Ok(self.body);
        }
        Err(io::Error::other(format!(
            "HTTP {}: {}",
            self.status,
            String::from_utf8_lossy(&self.body)
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_readiness_accepts_http11_and_rejects_truncated_responses() {
        let response =
            Response::parse(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK".to_vec()).unwrap();
        assert_eq!(response.require_success().unwrap(), b"OK");
        let error = Response::parse(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nO".to_vec())
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert!(
            Response::parse(b"HTTP/1.0 500 Broken\r\n\r\nfailed".to_vec())
                .unwrap()
                .require_success()
                .is_err()
        );
    }
}
