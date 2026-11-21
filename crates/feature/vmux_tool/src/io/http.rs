use std::io::Read;
use std::sync::Arc;
use zeroize::Zeroizing;

use vmux_ecs::{BlockingCancellation, Job};

use super::FileBytes;

#[derive(Clone)]
pub struct Authorization {
    token: Arc<Zeroizing<String>>,
    origin: url::Origin,
}

impl Authorization {
    pub fn from_environment(variable: &str, origin: &str) -> Result<Option<Self>, String> {
        let token = match std::env::var(variable) {
            Ok(token) => Zeroizing::new(token),
            _ => return Ok(None),
        };
        if token.trim().is_empty() {
            return Ok(None);
        }
        Self::new(
            token,
            url::Url::parse(origin).map_err(|error| error.to_string())?,
        )
        .map(Some)
    }

    fn new(token: Zeroizing<String>, origin: url::Url) -> Result<Self, String> {
        if origin.scheme() != "https" || origin.host_str().is_none() {
            return Err("HTTP authorization requires an HTTPS origin".into());
        }
        Ok(Self {
            token: Arc::new(token),
            origin: origin.origin(),
        })
    }

    fn apply(
        &self,
        request: reqwest::blocking::RequestBuilder,
        url: &url::Url,
    ) -> Result<reqwest::blocking::RequestBuilder, String> {
        if url.origin() != self.origin {
            return Err("HTTP authorization origin changed".into());
        }
        let encoded = Zeroizing::new(format!("Bearer {}", self.token.as_str()));
        let mut value = reqwest::header::HeaderValue::from_str(&encoded)
            .map_err(|_| "HTTP authorization header is invalid".to_string())?;
        value.set_sensitive(true);
        Ok(request.header(reqwest::header::AUTHORIZATION, value))
    }
}

#[derive(Clone)]
pub struct Get {
    pub url: String,
    pub limit: usize,
    pub authorization: Option<Authorization>,
}

impl Get {
    fn client(&self) -> Result<reqwest::blocking::ClientBuilder, String> {
        let url = url::Url::parse(&self.url).map_err(|error| error.to_string())?;
        let origin = self
            .authorization
            .as_ref()
            .map(|authorization| authorization.origin.clone());
        Ok(reqwest::blocking::Client::builder()
            .user_agent("vmux")
            .timeout(std::time::Duration::from_secs(60))
            .https_only(origin.is_some() || url.scheme() == "https")
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.error("too many redirects");
                }
                if origin
                    .as_ref()
                    .is_some_and(|origin| attempt.url().origin() != *origin)
                {
                    return attempt.error("HTTP authorization origin changed");
                }
                attempt.follow()
            })))
    }

    fn request(
        &self,
        client: &reqwest::blocking::Client,
    ) -> Result<reqwest::blocking::RequestBuilder, String> {
        let url = url::Url::parse(&self.url).map_err(|error| error.to_string())?;
        let request = client.get(url.clone());
        match &self.authorization {
            Some(authorization) => authorization.apply(request, &url),
            None => Ok(request),
        }
    }
}

impl Job for Get {
    type Output = Result<FileBytes, String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("HTTP request canceled".into());
        }
        let client = self.client()?.build().map_err(|error| error.to_string())?;
        let mut response = self
            .request(&client)?
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| error.to_string())?;
        let limit = self.limit.min(64 * 1024 * 1024);
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(format!("HTTP response exceeds {limit} bytes"));
        }
        let mut bytes = Vec::new();
        let mut buffer = [0; 65536];
        loop {
            if BlockingCancellation::canceled() {
                return Err("HTTP request canceled".into());
            }
            let read = response
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            if bytes.len().saturating_add(read) > limit {
                return Err(format!("HTTP response exceeds {limit} bytes"));
            }
            bytes.extend_from_slice(&buffer[..read]);
        }
        Ok(FileBytes(Some(bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct TlsFixture {
        url: String,
        certificate: reqwest::Certificate,
        stopped: Arc<AtomicBool>,
        connections: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<String>>>,
        worker: Option<std::thread::JoinHandle<()>>,
    }

    impl TlsFixture {
        fn new(redirect: Option<String>) -> Self {
            let key = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
            let certificate = reqwest::Certificate::from_der(key.cert.der()).unwrap();
            let config = rustls::ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![key.cert.der().clone()],
                rustls::pki_types::PrivateKeyDer::Pkcs8(key.signing_key.serialize_der().into()),
            )
            .unwrap();
            let config = Arc::new(config);
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("https://{}", listener.local_addr().unwrap());
            let next = match redirect {
                Some(next) => {
                    let mut next = url::Url::parse(&next).unwrap();
                    if next.port() == Some(0) {
                        next.set_port(Some(listener.local_addr().unwrap().port()))
                            .unwrap();
                    }
                    next.to_string()
                }
                None => format!("{url}/next"),
            };
            let stopped = Arc::new(AtomicBool::new(false));
            let stopping = stopped.clone();
            let connections = Arc::new(AtomicUsize::new(0));
            let connected = connections.clone();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let received = requests.clone();
            let worker = std::thread::spawn(move || {
                while !stopping.load(Ordering::Acquire) {
                    let (socket, _) = match listener.accept() {
                        Ok(socket) => socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(1));
                            continue;
                        }
                        Err(error) => panic!("{error}"),
                    };
                    connected.fetch_add(1, Ordering::AcqRel);
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    socket
                        .set_write_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    let connection = rustls::ServerConnection::new(config.clone()).unwrap();
                    let mut stream = rustls::StreamOwned::new(connection, socket);
                    let mut bytes = Vec::new();
                    let mut buffer = [0; 4096];
                    while !bytes.windows(4).any(|bytes| bytes == b"\r\n\r\n") && bytes.len() < 8192
                    {
                        let count = match stream.read(&mut buffer) {
                            Ok(0) | Err(_) => break,
                            Ok(count) => count,
                        };
                        bytes.extend_from_slice(&buffer[..count]);
                    }
                    if bytes.is_empty() {
                        continue;
                    }
                    let request = String::from_utf8(bytes).unwrap();
                    let response = if request.starts_with("GET /next ") {
                        "HTTP/1.1 200 OK\r\nContent-Length: 16\r\nConnection: close\r\n\r\nfixture complete".to_string()
                    } else {
                        format!(
                            "HTTP/1.1 302 Found\r\nLocation: {next}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        )
                    };
                    received.lock().unwrap().push(request);
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
            });
            Self {
                url,
                certificate,
                stopped,
                connections,
                requests,
                worker: Some(worker),
            }
        }

        fn request(&self) -> Get {
            Get {
                url: format!("{}/start", self.url),
                limit: 1024,
                authorization: Some(
                    Authorization::new(
                        Zeroizing::new("fixture-token".into()),
                        url::Url::parse(&self.url).unwrap(),
                    )
                    .unwrap(),
                ),
            }
        }
    }

    impl Drop for TlsFixture {
        fn drop(&mut self) {
            self.stopped.store(true, Ordering::Release);
            self.worker.take().unwrap().join().unwrap();
        }
    }

    #[test]
    fn credential_redirects_preserve_the_accepted_https_origin() {
        let fixture = TlsFixture::new(None);
        let request = fixture.request();
        let client = request
            .client()
            .unwrap()
            .add_root_certificate(fixture.certificate.clone())
            .build()
            .unwrap();
        let response = request.request(&client).unwrap().send().unwrap();
        assert_eq!(response.text().unwrap(), "fixture complete");
        let requests = fixture.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        for request in &*requests {
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer fixture-token")
            );
        }
    }

    #[test]
    fn credential_redirects_refuse_a_different_origin_before_connecting() {
        let rejected = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        rejected.set_nonblocking(true).unwrap();
        let fixture = TlsFixture::new(Some(format!(
            "https://{}/next",
            rejected.local_addr().unwrap()
        )));
        let request = fixture.request();
        let client = request
            .client()
            .unwrap()
            .add_root_certificate(fixture.certificate.clone())
            .build()
            .unwrap();
        assert!(request.request(&client).unwrap().send().is_err());
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
        assert_eq!(
            rejected.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn credential_redirects_refuse_plain_http_before_connecting() {
        let fixture = TlsFixture::new(Some("http://127.0.0.1:0/next".into()));
        let request = fixture.request();
        let client = request
            .client()
            .unwrap()
            .add_root_certificate(fixture.certificate.clone())
            .build()
            .unwrap();
        assert!(request.request(&client).unwrap().send().is_err());
        assert_eq!(fixture.connections.load(Ordering::Acquire), 1);
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    }

    #[test]
    fn authorization_is_origin_bound_and_sensitive_at_the_http_boundary() {
        let origin = url::Url::parse("https://api.github.com").unwrap();
        let authorization =
            Authorization::new(Zeroizing::new("test-secret".into()), origin).unwrap();
        let client = reqwest::blocking::Client::new();
        let url = url::Url::parse("https://API.GITHUB.COM:443/repos/example/releases").unwrap();
        let request = Get {
            url: url.to_string(),
            limit: 1024,
            authorization: Some(authorization.clone()),
        }
        .request(&client)
        .unwrap()
        .build()
        .unwrap();
        let header = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .unwrap();
        assert_eq!(header.to_str().unwrap(), "Bearer test-secret");
        assert!(!format!("{:?}", request.headers()).contains("test-secret"));
        for rejected in [
            "https://example.com/releases",
            "http://api.github.com/releases",
            "https://api.github.com:8443/releases",
        ] {
            assert!(
                Get {
                    url: rejected.into(),
                    limit: 1024,
                    authorization: Some(authorization.clone())
                }
                .request(&client)
                .is_err()
            );
        }
    }

    #[test]
    fn invalid_credentials_do_not_enter_native_http_requests() {
        let origin = url::Url::parse("https://api.github.com").unwrap();
        let authorization = Authorization::new(
            Zeroizing::new("secret\r\nInjected: yes".into()),
            origin.clone(),
        )
        .unwrap();
        let client = reqwest::blocking::Client::new();
        assert!(
            authorization
                .apply(client.get(origin.clone()), &origin)
                .is_err()
        );
        assert!(
            Authorization::new(
                Zeroizing::new("secret".into()),
                url::Url::parse("http://api.github.com").unwrap()
            )
            .is_err()
        );
        assert!(
            !Get {
                url: "https://example.com/archive.zip".into(),
                limit: 1024,
                authorization: None
            }
            .request(&client)
            .unwrap()
            .build()
            .unwrap()
            .headers()
            .contains_key(reqwest::header::AUTHORIZATION)
        );
    }
}
