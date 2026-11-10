use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(not(unix))]
use std::io::{Seek, SeekFrom};
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct Source(Arc<RegisteredFile>);

struct RegisteredFile {
    token: String,
    file: File,
    size: u64,
    mime: &'static str,
}

struct Server {
    port: u16,
    files: Arc<Mutex<HashMap<String, Weak<RegisteredFile>>>>,
}

impl Source {
    pub(crate) fn open(path: &Path) -> std::io::Result<Self> {
        let server = Server::shared()?;
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(std::io::Error::other("video source is not a file"));
        }
        let mime = vmux_api::media::MediaKind::mime(&path.to_string_lossy())
            .filter(|mime| mime.starts_with("video/"))
            .ok_or_else(|| std::io::Error::other("unsupported video source"))?;
        let mime = if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mov"))
        {
            "video/quicktime"
        } else {
            mime
        };
        let source = Self(Arc::new(RegisteredFile {
            token: uuid::Uuid::new_v4().to_string(),
            file,
            size: metadata.len(),
            mime,
        }));
        let mut files = server
            .files
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        files.retain(|_, file| file.strong_count() != 0);
        files.insert(source.0.token.clone(), Arc::downgrade(&source.0));
        Ok(source)
    }

    pub(crate) fn url(&self) -> String {
        let port = Server::shared()
            .expect("a registered source has a server")
            .port;
        format!("http://127.0.0.1:{port}/{}", self.0.token)
    }

    pub(crate) fn path(path: &str) -> PathBuf {
        if let Some(relative) = path.strip_prefix("~/")
            && let Some(home) = std::env::var_os("HOME")
        {
            return PathBuf::from(home).join(relative);
        }
        PathBuf::from(path)
    }
}

impl Server {
    fn shared() -> std::io::Result<&'static Self> {
        static SERVER: OnceLock<Result<Server, String>> = OnceLock::new();
        match SERVER.get_or_init(|| Self::start().map_err(|error| error.to_string())) {
            Ok(server) => Ok(server),
            Err(error) => Err(std::io::Error::other(error.clone())),
        }
    }

    fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let files = Arc::new(Mutex::new(HashMap::new()));
        for index in 0..4 {
            let listener = listener.try_clone()?;
            let files = files.clone();
            std::thread::Builder::new()
                .name(format!("session-video-{index}"))
                .spawn(move || {
                    for stream in listener.incoming().flatten() {
                        let _ = Self::serve(stream, port, &files);
                    }
                })?;
        }
        Ok(Self { port, files })
    }

    fn serve(
        mut stream: TcpStream,
        port: u16,
        files: &Mutex<HashMap<String, Weak<RegisteredFile>>>,
    ) -> std::io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(15)))?;
        let mut request = BufReader::new((&stream).take(16 * 1024));
        let mut line = String::new();
        request.read_line(&mut line)?;
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let target = parts.next().unwrap_or_default().to_string();
        let mut range = None;
        let mut host = String::new();
        loop {
            line.clear();
            if request.read_line(&mut line)? == 0 {
                return Self::error(&mut stream, 400, "Bad Request");
            }
            if line == "\r\n" {
                break;
            }
            let Some((name, value)) = line.split_once(':') else {
                return Self::error(&mut stream, 400, "Bad Request");
            };
            if name.eq_ignore_ascii_case("range") {
                range = Some(value.trim().to_string());
            }
            if name.eq_ignore_ascii_case("host") {
                host = value.trim().to_string();
            }
        }
        if host != format!("127.0.0.1:{port}") {
            return Self::error(&mut stream, 403, "Forbidden");
        }
        if method != "GET" && method != "HEAD" {
            return Self::error(&mut stream, 405, "Method Not Allowed");
        }
        let source = files
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(target.strip_prefix('/').unwrap_or_default())
            .and_then(Weak::upgrade);
        let Some(source) = source else {
            return Self::error(&mut stream, 404, "Not Found");
        };
        let selected = if method == "HEAD" {
            Ok(ByteRange {
                start: 0,
                length: source.size,
                partial: false,
            })
        } else {
            ByteRange::parse(range.as_deref(), source.size)
        };
        let Ok(selected) = selected else {
            return write!(
                stream,
                "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                source.size
            );
        };
        write!(
            stream,
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\nConnection: close\r\n",
            if selected.partial {
                "206 Partial Content"
            } else {
                "200 OK"
            },
            source.mime,
            selected.length
        )?;
        if selected.partial {
            write!(
                stream,
                "Content-Range: bytes {}-{}/{}\r\n",
                selected.start,
                selected.start + selected.length - 1,
                source.size
            )?;
        }
        write!(stream, "\r\n")?;
        if method == "HEAD" {
            return Ok(());
        }
        let mut position = selected.start;
        let mut remaining = selected.length;
        let mut buffer = [0u8; 64 * 1024];
        while remaining > 0 {
            let capacity = remaining.min(buffer.len() as u64) as usize;
            #[cfg(unix)]
            let count = { source.file.read_at(&mut buffer[..capacity], position)? };
            #[cfg(not(unix))]
            let count = {
                let mut file = source.file.try_clone()?;
                file.seek(SeekFrom::Start(position))?;
                file.read(&mut buffer[..capacity])?
            };
            if count == 0 {
                break;
            }
            stream.write_all(&buffer[..count])?;
            position += count as u64;
            remaining -= count as u64;
        }
        Ok(())
    }

    fn error(stream: &mut TcpStream, status: u16, reason: &str) -> std::io::Result<()> {
        write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
    }
}

#[derive(Debug, PartialEq)]
struct ByteRange {
    start: u64,
    length: u64,
    partial: bool,
}

impl ByteRange {
    fn parse(header: Option<&str>, size: u64) -> Result<Self, ()> {
        let Some(header) = header else {
            return Ok(Self {
                start: 0,
                length: size,
                partial: false,
            });
        };
        let value = header.strip_prefix("bytes=").ok_or(())?;
        let (start, end) = value.split_once('-').ok_or(())?;
        if size == 0 {
            return Err(());
        }
        let (start, end) = if start.is_empty() {
            let suffix: u64 = end.parse().map_err(|_| ())?;
            if suffix == 0 {
                return Err(());
            }
            (size.saturating_sub(suffix), size - 1)
        } else {
            let start: u64 = start.parse().map_err(|_| ())?;
            let end = if end.is_empty() {
                size - 1
            } else {
                end.parse::<u64>().map_err(|_| ())?.min(size - 1)
            };
            (start, end)
        };
        if start >= size || start > end {
            return Err(());
        }
        Ok(Self {
            start,
            length: end - start + 1,
            partial: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Client(url::Url);

    impl Client {
        fn request(&self, method: &str, range: &str) -> String {
            let mut stream = TcpStream::connect(("127.0.0.1", self.0.port().unwrap())).unwrap();
            write!(
                stream,
                "{method} {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{range}\r\n",
                self.0.path(),
                self.0.port().unwrap()
            )
            .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        }
    }

    #[test]
    fn registered_video_serves_seek_ranges_and_revokes_after_drop() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("clip.mov");
        std::fs::write(&path, b"0123456789").unwrap();
        let source = Source::open(&path).unwrap();
        let client = Client(url::Url::parse(&source.url()).unwrap());
        let partial = client.request("GET", "Range: bytes=3-6\r\n");
        assert!(partial.starts_with("HTTP/1.1 206"));
        assert!(partial.contains("Content-Type: video/quicktime\r\n"));
        assert!(partial.contains("Content-Range: bytes 3-6/10\r\n"));
        assert!(partial.ends_with("\r\n\r\n3456"));
        assert!(
            client
                .request("GET", "Range: bytes=-2\r\n")
                .ends_with("\r\n\r\n89")
        );
        assert!(
            client
                .request("GET", "Range: bytes=10-\r\n")
                .starts_with("HTTP/1.1 416")
        );
        assert!(client.request("HEAD", "").ends_with("\r\n\r\n"));
        assert!(client.request("GET", "").ends_with("\r\n\r\n0123456789"));
        drop(source);
        assert!(client.request("GET", "").starts_with("HTTP/1.1 404"));
    }

    #[test]
    fn ranges_reject_invalid_and_multiple_intervals() {
        for range in [
            "bytes=8-2",
            "bytes=-0",
            "bytes=1-3,5-8",
            "items=1-2",
            "bytes=a-2",
        ] {
            assert!(ByteRange::parse(Some(range), 10).is_err(), "{range}");
        }
        assert!(ByteRange::parse(Some("bytes=0-"), 0).is_err());
    }
}
