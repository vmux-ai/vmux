use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::http::UnixHttp;

pub struct Client(UnixHttp);

impl Client {
    pub fn new(socket: &Path) -> Self {
        Self(UnixHttp::new(socket))
    }

    pub fn list(&self) -> io::Result<Vec<Container>> {
        let body = self.0.get("/containers/json?all=1")?.require_success()?;
        Container::decode(&body)
    }

    pub fn change(&self, id: &str, operation: Operation) -> io::Result<()> {
        let id = Id::new(id)?;
        let http = self.0.clone().timeout(Duration::from_secs(30));
        let response = match operation {
            Operation::Start => http.post(&id.endpoint("/start"), &[])?,
            Operation::Stop => http.post(&id.endpoint("/stop?t=10"), &[])?,
            Operation::Restart => http.post(&id.endpoint("/restart?t=10"), &[])?,
            Operation::Remove => http.delete(&id.endpoint("?force=false&v=false"))?,
        };
        if response.status == 304 && matches!(operation, Operation::Start | Operation::Stop) {
            return Ok(());
        }
        response.require_success()?;
        Ok(())
    }

    pub fn logs(&self, id: &str) -> io::Result<String> {
        let id = Id::new(id)?;
        let inspection = self.0.get(&id.endpoint("/json"))?.require_success()?;
        let inspection: Inspection =
            serde_json::from_slice(&inspection).map_err(io::Error::other)?;
        let body = self
            .0
            .get(&id.endpoint("/logs?stdout=1&stderr=1&tail=200&timestamps=1"))?
            .require_success()?;
        Output::decode(&body, inspection.config.tty)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Operation {
    Start,
    Stop,
    Restart,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub project: String,
    pub service: String,
    pub ports: Vec<Port>,
}

impl Container {
    fn decode(body: &[u8]) -> io::Result<Vec<Self>> {
        let responses: Vec<Description> = serde_json::from_slice(body).map_err(io::Error::other)?;
        let mut containers = Vec::new();
        for response in responses {
            Id::new(&response.id)?;
            let name = match response.names.first() {
                Some(name) => name.trim_start_matches('/').to_string(),
                None => response.id[..12].to_string(),
            };
            let mut ports = Vec::new();
            for port in response.ports {
                let Some(host) = port.host else {
                    continue;
                };
                if host == 0 || !matches!(port.ip.as_str(), "" | "0.0.0.0" | "::") {
                    continue;
                }
                ports.push(Port {
                    host,
                    guest: port.guest,
                    protocol: port.protocol,
                });
            }
            ports.sort();
            ports.dedup();
            containers.push(Self {
                id: response.id,
                name,
                image: response.image,
                state: response.state,
                status: response.status,
                project: response
                    .labels
                    .get("com.docker.compose.project")
                    .cloned()
                    .unwrap_or_default(),
                service: response
                    .labels
                    .get("com.docker.compose.service")
                    .cloned()
                    .unwrap_or_default(),
                ports,
            });
        }
        containers.sort_by(|left, right| {
            left.project
                .cmp(&right.project)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(containers)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Port {
    pub host: u16,
    pub guest: u16,
    pub protocol: String,
}

#[derive(Deserialize)]
struct Description {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Names", default)]
    names: Vec<String>,
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Status", default)]
    status: String,
    #[serde(rename = "Labels", default)]
    labels: BTreeMap<String, String>,
    #[serde(rename = "Ports", default)]
    ports: Vec<Binding>,
}

#[derive(Deserialize)]
struct Binding {
    #[serde(rename = "PublicPort")]
    host: Option<u16>,
    #[serde(rename = "PrivatePort")]
    guest: u16,
    #[serde(rename = "Type")]
    protocol: String,
    #[serde(rename = "IP", default)]
    ip: String,
}

struct Id<'a>(&'a str);

impl<'a> Id<'a> {
    fn new(value: &'a str) -> io::Result<Self> {
        if value.len() < 12
            || value.len() > 64
            || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid container ID",
            ));
        }
        Ok(Self(value))
    }

    fn endpoint(&self, suffix: &str) -> String {
        format!("/containers/{}{suffix}", self.0)
    }
}

#[derive(Deserialize)]
struct Inspection {
    #[serde(rename = "Config")]
    config: Configuration,
}

#[derive(Deserialize)]
struct Configuration {
    #[serde(rename = "Tty", default)]
    tty: bool,
}

struct Output;

impl Output {
    fn decode(bytes: &[u8], tty: bool) -> io::Result<String> {
        if tty {
            return Ok(String::from_utf8_lossy(bytes).into_owned());
        }
        let mut output = Vec::new();
        let mut cursor = 0;
        while cursor < bytes.len() {
            let remaining = &bytes[cursor..];
            if remaining.len() < 8 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "truncated Docker log header",
                ));
            }
            if remaining[0] > 2 || remaining[1..4] != [0, 0, 0] {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid Docker log frame",
                ));
            }
            let length = u32::from_be_bytes(remaining[4..8].try_into().unwrap()) as usize;
            if length > remaining.len() - 8 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "truncated Docker log frame",
                ));
            }
            output.extend_from_slice(&remaining[8..8 + length]);
            cursor += 8 + length;
        }
        Ok(String::from_utf8_lossy(&output).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::thread::{self, JoinHandle};

    struct Daemon {
        socket: PathBuf,
        response: JoinHandle<String>,
        _directory: tempfile::TempDir,
    }

    impl Daemon {
        fn new(status: u16, body: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let socket = directory.path().join("docker.sock");
            let listener = UnixListener::bind(&socket).unwrap();
            let reply = format!(
                "HTTP/1.0 {status} OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let response = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                stream.write_all(reply.as_bytes()).unwrap();
                String::from_utf8(request).unwrap()
            });
            Self {
                socket,
                response,
                _directory: directory,
            }
        }

        fn request(self) -> String {
            self.response.join().unwrap()
        }
    }

    #[test]
    fn inventory_requests_stopped_containers_from_the_daemon() {
        let daemon = Daemon::new(200, "[]");
        assert!(Client::new(&daemon.socket).list().unwrap().is_empty());
        assert_eq!(
            daemon.request().lines().next(),
            Some("GET /containers/json?all=1 HTTP/1.0")
        );
    }

    #[test]
    fn container_commands_are_idempotent_and_delete_without_removing_volumes() {
        for (operation, status, expected) in [
            (
                Operation::Start,
                304,
                "POST /containers/eeeeeeeeeeee/start HTTP/1.0",
            ),
            (
                Operation::Stop,
                304,
                "POST /containers/eeeeeeeeeeee/stop?t=10 HTTP/1.0",
            ),
            (
                Operation::Restart,
                204,
                "POST /containers/eeeeeeeeeeee/restart?t=10 HTTP/1.0",
            ),
            (
                Operation::Remove,
                204,
                "DELETE /containers/eeeeeeeeeeee?force=false&v=false HTTP/1.0",
            ),
        ] {
            let daemon = Daemon::new(status, "");
            Client::new(&daemon.socket)
                .change("eeeeeeeeeeee", operation)
                .unwrap();
            assert_eq!(daemon.request().lines().next(), Some(expected));
        }
    }

    #[test]
    fn docker_logs_preserve_stdout_stderr_and_tty_text() {
        let framed = b"\x01\0\0\0\0\0\0\x04out\n\x02\0\0\0\0\0\0\x04err\n";
        assert_eq!(Output::decode(framed, false).unwrap(), "out\nerr\n");
        assert_eq!(
            Output::decode(b"\x1b[32mhello\n", true).unwrap(),
            "\x1b[32mhello\n"
        );
        assert_eq!(
            Output::decode(&framed[..framed.len() - 1], false)
                .unwrap_err()
                .kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn container_requests_refuse_path_and_query_injection_before_connecting() {
        let client = Client::new(Path::new("/missing/docker.sock"));
        for id in [
            "../other",
            "eeeeeeeeeeee?force=true",
            "eeeeeeeeeeee/start",
            "",
            "name",
        ] {
            assert_eq!(
                client.change(id, Operation::Remove).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn inventory_keeps_stopped_services_and_deduplicates_forwarded_ports() {
        let body = br#"[{"Id":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","Names":["/stack-web-1"],"Image":"nginx:alpine","State":"exited","Status":"Exited (0)","Labels":{"com.docker.compose.project":"stack","com.docker.compose.service":"web"},"Ports":[{"PrivatePort":80,"PublicPort":8080,"Type":"tcp","IP":"0.0.0.0"},{"PrivatePort":80,"PublicPort":8080,"Type":"tcp","IP":"::"},{"PrivatePort":443,"Type":"tcp"}]}]"#;
        let entries = Container::decode(body).unwrap();
        assert_eq!(entries[0].name, "stack-web-1");
        assert_eq!(entries[0].project, "stack");
        assert_eq!(entries[0].service, "web");
        assert_eq!(entries[0].state, "exited");
        assert_eq!(
            entries[0].ports,
            vec![Port {
                host: 8080,
                guest: 80,
                protocol: "tcp".into()
            }]
        );
    }
}
