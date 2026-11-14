use std::collections::BTreeSet;
use std::io;
use std::path::Path;

use serde::Deserialize;

use crate::http::UnixHttp;

pub(crate) struct PortForwarder {
    docker: UnixHttp,
    network: UnixHttp,
    active: BTreeSet<PublishedPort>,
}

impl PortForwarder {
    pub(crate) fn new(docker: &Path, network: &Path) -> Self {
        Self {
            docker: UnixHttp::new(docker),
            network: UnixHttp::new(network),
            active: BTreeSet::new(),
        }
    }

    pub(crate) fn refresh(&mut self) -> io::Result<()> {
        let body = self.docker.get("/containers/json")?.require_success()?;
        let published = PublishedPort::from_containers(&body)?;
        let body = self
            .network
            .get("/services/forwarder/all")?
            .require_success()?;
        let observed = PublishedPort::from_forwarders(&body)?;
        self.active.retain(|port| observed.contains(port));
        for port in &published {
            if observed.contains(port) {
                self.active.insert(port.clone());
            }
        }
        let mut errors = Vec::new();
        let removals = self
            .active
            .difference(&published)
            .cloned()
            .collect::<Vec<_>>();
        for port in removals {
            let request = serde_json::to_vec(&serde_json::json!({
                "local": port.local(), "protocol": port.protocol,
            }))
            .map_err(io::Error::other)?;
            match self
                .network
                .post("/services/forwarder/unexpose", &request)
                .and_then(|response| response.require_success())
            {
                Ok(_) => {
                    self.active.remove(&port);
                }
                Err(error) => {
                    errors.push((error.kind(), format!("{}: {error}", port.local())));
                }
            }
        }
        for port in published {
            if self.active.contains(&port) {
                continue;
            }
            let request = serde_json::to_vec(&serde_json::json!({
                "local": port.local(),
                "remote": format!("192.168.127.2:{}", port.port),
                "protocol": port.protocol,
            }))
            .map_err(io::Error::other)?;
            match self
                .network
                .post("/services/forwarder/expose", &request)
                .and_then(|response| response.require_success())
            {
                Ok(_) => {
                    self.active.insert(port);
                }
                Err(error) => {
                    errors.push((error.kind(), format!("{}: {error}", port.local())));
                }
            }
        }
        if !errors.is_empty() {
            let mut kind = errors[0].0;
            let mut messages = Vec::new();
            for (error, message) in errors {
                if !matches!(
                    error,
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::Interrupted
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::ConnectionAborted
                        | io::ErrorKind::NotConnected
                        | io::ErrorKind::ConnectionRefused
                        | io::ErrorKind::UnexpectedEof
                        | io::ErrorKind::BrokenPipe
                ) {
                    kind = error;
                }
                messages.push(message);
            }
            return Err(io::Error::new(kind, messages.join("\n")));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PublishedPort {
    port: u16,
    protocol: String,
}

impl PublishedPort {
    fn from_containers(body: &[u8]) -> io::Result<BTreeSet<Self>> {
        let containers: Vec<Container> = serde_json::from_slice(body).map_err(io::Error::other)?;
        let mut ports = BTreeSet::new();
        for container in containers {
            for port in container.ports {
                let Some(public) = port.public else {
                    continue;
                };
                if public == 0 || (port.protocol != "tcp" && port.protocol != "udp") {
                    continue;
                }
                if !matches!(port.ip.as_str(), "" | "0.0.0.0" | "::") {
                    continue;
                }
                ports.insert(Self {
                    port: public,
                    protocol: port.protocol,
                });
            }
        }
        Ok(ports)
    }

    fn from_forwarders(body: &[u8]) -> io::Result<BTreeSet<Self>> {
        let mappings: Vec<Mapping> = serde_json::from_slice(body).map_err(io::Error::other)?;
        let mut ports = BTreeSet::new();
        for mapping in mappings {
            if mapping.protocol != "tcp" && mapping.protocol != "udp" {
                continue;
            }
            let Some(local) = mapping.local.strip_prefix("127.0.0.1:") else {
                continue;
            };
            let Ok(port) = local.parse::<u16>() else {
                continue;
            };
            if port == 0 || mapping.remote != format!("192.168.127.2:{port}") {
                continue;
            }
            ports.insert(Self {
                port,
                protocol: mapping.protocol,
            });
        }
        Ok(ports)
    }

    fn local(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
}

#[derive(Deserialize)]
struct Mapping {
    local: String,
    remote: String,
    protocol: String,
}

#[derive(Deserialize)]
struct Container {
    #[serde(rename = "Ports", default)]
    ports: Vec<Port>,
}

#[derive(Deserialize)]
struct Port {
    #[serde(rename = "PublicPort")]
    public: Option<u16>,
    #[serde(rename = "Type")]
    protocol: String,
    #[serde(rename = "IP", default)]
    ip: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, Read, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread::JoinHandle;
    use std::time::Duration;

    #[derive(Default)]
    struct BackendState {
        published: BTreeSet<u16>,
        active: BTreeSet<u16>,
        conflict: Option<u16>,
        lose_removal: bool,
        additions: Vec<u16>,
        removals: Vec<u16>,
    }

    struct Backend {
        directory: tempfile::TempDir,
        state: Arc<Mutex<BackendState>>,
        alive: Arc<AtomicBool>,
        threads: Vec<JoinHandle<()>>,
    }

    impl Backend {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let state = Arc::new(Mutex::new(BackendState::default()));
            let alive = Arc::new(AtomicBool::new(true));
            let mut threads = Vec::new();
            for name in ["docker", "network"] {
                let listener = UnixListener::bind(directory.path().join(name)).unwrap();
                listener.set_nonblocking(true).unwrap();
                let state = state.clone();
                let alive = alive.clone();
                threads.push(std::thread::spawn(move || {
                    Self::serve(listener, state, alive, name == "docker")
                }));
            }
            Self {
                directory,
                state,
                alive,
                threads,
            }
        }

        fn forwarder(&self) -> PortForwarder {
            PortForwarder::new(
                &self.directory.path().join("docker"),
                &self.directory.path().join("network"),
            )
        }

        fn serve(
            listener: UnixListener,
            state: Arc<Mutex<BackendState>>,
            alive: Arc<AtomicBool>,
            docker: bool,
        ) {
            while alive.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(stream) => stream,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = std::io::BufReader::new(&mut stream);
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header == "\r\n" {
                        break;
                    }
                    if let Some(value) = header.strip_prefix("Content-Length: ") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let mut state = state.lock().unwrap();
                let mut status = 200;
                let body = if docker {
                    let mut ports = Vec::new();
                    for port in &state.published {
                        ports.push(
                            serde_json::json!({"PublicPort":port,"Type":"tcp","IP":"0.0.0.0"}),
                        );
                    }
                    serde_json::to_vec(&serde_json::json!([{"Ports":ports}])).unwrap()
                } else if request.starts_with("GET /services/forwarder/all ") {
                    let mut mappings = Vec::new();
                    for port in &state.active {
                        mappings.push(serde_json::json!({"local":format!("127.0.0.1:{port}"),"remote":format!("192.168.127.2:{port}"),"protocol":"tcp"}));
                    }
                    serde_json::to_vec(&mappings).unwrap()
                } else {
                    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    let port = body["local"]
                        .as_str()
                        .unwrap()
                        .strip_prefix("127.0.0.1:")
                        .unwrap()
                        .parse::<u16>()
                        .unwrap();
                    if request.starts_with("POST /services/forwarder/unexpose ") {
                        state.removals.push(port);
                        if !state.active.remove(&port) {
                            status = 500;
                        }
                        if state.lose_removal {
                            state.lose_removal = false;
                            continue;
                        }
                    } else {
                        assert!(request.starts_with("POST /services/forwarder/expose "));
                        state.additions.push(port);
                        if state.conflict == Some(port) || !state.active.insert(port) {
                            status = 500;
                        }
                    }
                    b"proxy result".to_vec()
                };
                write!(
                    stream,
                    "HTTP/1.0 {status} Result\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
        }
    }

    impl Drop for Backend {
        fn drop(&mut self) {
            self.alive.store(false, Ordering::Release);
            for thread in self.threads.drain(..) {
                thread.join().unwrap();
            }
        }
    }

    #[test]
    fn lost_removal_acknowledgement_does_not_block_new_ports_or_repeat_absent_removals() {
        let backend = Backend::new();
        backend.state.lock().unwrap().published.insert(8080);
        let mut forwarder = backend.forwarder();
        forwarder.refresh().unwrap();
        {
            let mut state = backend.state.lock().unwrap();
            state.published = BTreeSet::from([9090]);
            state.lose_removal = true;
        }
        assert_eq!(
            forwarder.refresh().unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        assert!(backend.state.lock().unwrap().active.contains(&9090));
        forwarder.refresh().unwrap();
        let state = backend.state.lock().unwrap();
        assert_eq!(state.removals, [8080]);
        assert_eq!(state.additions, [8080, 9090]);
    }

    #[test]
    fn one_conflicting_port_does_not_starve_other_container_ports() {
        let backend = Backend::new();
        {
            let mut state = backend.state.lock().unwrap();
            state.published = BTreeSet::from([8080, 9090]);
            state.conflict = Some(8080);
        }
        let mut forwarder = backend.forwarder();
        assert!(forwarder.refresh().is_err());
        assert!(backend.state.lock().unwrap().active.contains(&9090));
        backend.state.lock().unwrap().conflict = None;
        forwarder.refresh().unwrap();
        assert_eq!(
            backend.state.lock().unwrap().active,
            BTreeSet::from([8080, 9090])
        );
    }

    #[test]
    fn only_published_ports_are_forwarded_and_dual_stack_entries_are_deduplicated() {
        let body = br#"[{"Ports":[
            {"PrivatePort":80,"PublicPort":8080,"Type":"tcp","IP":"0.0.0.0"},
            {"PrivatePort":80,"PublicPort":8080,"Type":"tcp","IP":"::"},
            {"PrivatePort":53,"PublicPort":5353,"Type":"udp","IP":"0.0.0.0"},
            {"PrivatePort":443,"Type":"tcp"},
            {"PrivatePort":90,"PublicPort":9090,"Type":"sctp"}
        ]}]"#;
        let actual = PublishedPort::from_containers(body).unwrap();
        let expected = BTreeSet::from([
            PublishedPort {
                port: 8080,
                protocol: "tcp".into(),
            },
            PublishedPort {
                port: 5353,
                protocol: "udp".into(),
            },
        ]);
        assert_eq!(actual, expected);
    }
}
