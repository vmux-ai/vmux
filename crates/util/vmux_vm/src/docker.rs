use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::http::UnixHttp;

pub struct Client(UnixHttp);

impl Client {
    pub fn new(socket: &Path) -> Self {
        Self(UnixHttp::new(socket))
    }

    pub fn list(&self) -> io::Result<Vec<Container>> {
        let mut retries = 0;
        loop {
            match self.0.get("/containers/json?all=1") {
                Ok(response) => return Container::decode(&response.require_success()?),
                Err(error) => {
                    if retries == 2
                        || !matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock
                                | io::ErrorKind::TimedOut
                                | io::ErrorKind::Interrupted
                                | io::ErrorKind::ConnectionReset
                                | io::ErrorKind::ConnectionAborted
                                | io::ErrorKind::ConnectionRefused
                                | io::ErrorKind::UnexpectedEof
                        )
                    {
                        return Err(error);
                    }
                    retries += 1;
                    std::thread::sleep(Duration::from_millis(250));
                }
            }
        }
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

    pub fn start_many(&self, ids: &[String]) -> io::Result<()> {
        let containers = self.list()?;
        let ordered = Batch::new(ids, &containers)?
            .with_dependencies(&containers)
            .ordered()?;
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut failed = HashSet::new();
        let mut errors = Vec::new();
        for entry in &ordered {
            let mut refusal = None;
            for dependency in &entry.dependencies {
                let Some(target) = ordered.iter().find(|target| {
                    target.project == entry.project && target.service == dependency.service
                }) else {
                    continue;
                };
                if failed.contains(&target.id) {
                    refusal = Some(format!("dependency {} failed to start", dependency.service));
                    break;
                }
                if let Err(error) = self.wait(&target.id, dependency.condition, deadline) {
                    refusal = Some(format!("dependency {}: {error}", dependency.service));
                    break;
                }
            }
            let result = match refusal {
                Some(error) => Err(io::Error::other(error)),
                None => self.change(&entry.id, Operation::Start),
            };
            if let Err(error) = result {
                failed.insert(entry.id.clone());
                errors.push(format!("{}: {error}", entry.name));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(io::Error::other(errors.join("\n")))
        }
    }

    fn wait(&self, id: &str, condition: Condition, deadline: Instant) -> io::Result<()> {
        let id = Id::new(id)?;
        loop {
            let body = self.0.get(&id.endpoint("/json"))?.require_success()?;
            let observation: Readiness = serde_json::from_slice(&body).map_err(io::Error::other)?;
            let state = observation.state;
            let ready = match condition {
                Condition::Started => state.running,
                Condition::Healthy => {
                    state.running
                        && state
                            .health
                            .as_ref()
                            .is_some_and(|health| health.status == "healthy")
                }
                Condition::Completed => {
                    !state.running && state.status == "exited" && state.exit_code == 0
                }
            };
            if ready {
                return Ok(());
            }
            if !state.running && matches!(state.status.as_str(), "exited" | "dead") {
                return Err(io::Error::other(format!(
                    "container stopped with exit code {}",
                    state.exit_code
                )));
            }
            if condition == Condition::Healthy
                && state
                    .health
                    .as_ref()
                    .is_none_or(|health| health.status == "unhealthy")
            {
                return Err(io::Error::other(
                    "container healthcheck is missing or unhealthy",
                ));
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "container dependency did not become ready",
                ));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub project: String,
    pub service: String,
    pub dependencies: Vec<Dependency>,
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
            let mut dependencies = Vec::new();
            if let Some(value) = response.labels.get("com.docker.compose.depends_on") {
                for item in value.split(',') {
                    let mut fields = item.split(':');
                    let service = fields.next().unwrap_or_default().trim();
                    if service.is_empty() {
                        continue;
                    }
                    let condition = match fields.next() {
                        Some("service_healthy") => Condition::Healthy,
                        Some("service_completed_successfully") => Condition::Completed,
                        _ => Condition::Started,
                    };
                    dependencies.push(Dependency {
                        service: service.into(),
                        condition,
                    });
                }
            }
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
                dependencies,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub service: String,
    pub condition: Condition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Condition {
    Started,
    Healthy,
    Completed,
}

struct Batch<'a> {
    entries: Vec<&'a Container>,
}

impl<'a> Batch<'a> {
    fn new(ids: &[String], containers: &'a [Container]) -> io::Result<Self> {
        let mut entries = Vec::new();
        for id in ids {
            let Some(entry) = containers.iter().find(|entry| entry.id == *id) else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("container {id} is no longer present"),
                ));
            };
            if !entries
                .iter()
                .any(|existing: &&Container| existing.id == entry.id)
            {
                entries.push(entry);
            }
        }
        Ok(Self { entries })
    }

    fn ordered(mut self) -> io::Result<Vec<&'a Container>> {
        let mut ordered = Vec::new();
        while !self.entries.is_empty() {
            let next = self.entries.iter().position(|entry| {
                !entry.dependencies.iter().any(|dependency| {
                    self.entries.iter().any(|candidate| {
                        candidate.project == entry.project
                            && candidate.service == dependency.service
                    })
                })
            });
            let Some(index) = next else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "container group has a dependency cycle",
                ));
            };
            ordered.push(self.entries.remove(index));
        }
        Ok(ordered)
    }

    fn with_dependencies(mut self, containers: &'a [Container]) -> Self {
        let mut index = 0;
        while index < self.entries.len() {
            let entry = self.entries[index];
            index += 1;
            if entry.project.is_empty() {
                continue;
            }
            for dependency in &entry.dependencies {
                let Some(target) = containers.iter().find(|target| {
                    target.project == entry.project && target.service == dependency.service
                }) else {
                    continue;
                };
                if self.entries.iter().any(|entry| entry.id == target.id) {
                    continue;
                }
                self.entries.push(target);
            }
        }
        self
    }
}

#[derive(Deserialize)]
struct Readiness {
    #[serde(rename = "State")]
    state: RunState,
}

#[derive(Deserialize)]
struct RunState {
    #[serde(rename = "Running")]
    running: bool,
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "ExitCode", default)]
    exit_code: i32,
    #[serde(rename = "Health", default)]
    health: Option<Health>,
}

#[derive(Deserialize)]
struct Health {
    #[serde(rename = "Status")]
    status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
        response: JoinHandle<Vec<String>>,
        _directory: tempfile::TempDir,
    }

    enum Reply {
        Stalled,
        Http(String),
    }

    impl Daemon {
        fn new(replies: Vec<Reply>) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let socket = directory.path().join("docker.sock");
            let listener = UnixListener::bind(&socket).unwrap();
            listener.set_nonblocking(true).unwrap();
            let response = thread::spawn(move || {
                let mut requests = Vec::new();
                for reply in replies {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                                assert!(Instant::now() < deadline, "missing Docker request");
                                thread::sleep(Duration::from_millis(10));
                            }
                            Err(error) => panic!("{error}"),
                        }
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut byte = [0];
                    while !request.ends_with(b"\r\n\r\n") {
                        stream.read_exact(&mut byte).unwrap();
                        request.push(byte[0]);
                    }
                    match reply {
                        Reply::Stalled => thread::sleep(Duration::from_millis(2100)),
                        Reply::Http(reply) => stream.write_all(reply.as_bytes()).unwrap(),
                    }
                    requests.push(String::from_utf8(request).unwrap());
                }
                requests
            });
            Self {
                socket,
                response,
                _directory: directory,
            }
        }

        fn request(self) -> String {
            let mut requests = self.response.join().unwrap();
            assert_eq!(requests.len(), 1);
            requests.pop().unwrap()
        }

        fn reply(status: u16, body: &str) -> Reply {
            Reply::Http(format!(
                "HTTP/1.0 {status} OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ))
        }
    }

    #[test]
    fn inventory_requests_stopped_containers_from_the_daemon() {
        let daemon = Daemon::new(vec![Daemon::reply(200, "[]")]);
        assert!(Client::new(&daemon.socket).list().unwrap().is_empty());
        assert_eq!(
            daemon.request().lines().next(),
            Some("GET /containers/json?all=1 HTTP/1.0")
        );
    }

    #[test]
    fn group_start_orders_dependencies_and_waits_for_their_health() {
        let inventory = r#"[{"Id":"aaaaaaaaaaaa","Names":["/account"],"Image":"account","State":"exited","Labels":{"com.docker.compose.project":"cloud","com.docker.compose.service":"account","com.docker.compose.depends_on":"postgres:service_healthy:true"}},{"Id":"bbbbbbbbbbbb","Names":["/postgres"],"Image":"postgres","State":"exited","Labels":{"com.docker.compose.project":"cloud","com.docker.compose.service":"postgres"}},{"Id":"cccccccccccc","Names":["/relay"],"Image":"relay","State":"exited","Labels":{"com.docker.compose.project":"cloud","com.docker.compose.service":"relay"}}]"#;
        for healthy in [true, false] {
            let readiness = if healthy {
                r#"{"State":{"Running":true,"Status":"running","Health":{"Status":"healthy"}}}"#
            } else {
                r#"{"State":{"Running":false,"Status":"exited","ExitCode":255}}"#
            };
            let mut replies = vec![Daemon::reply(200, inventory), Daemon::reply(204, "")];
            if healthy {
                replies.push(Daemon::reply(200, r#"{"State":{"Running":true,"Status":"running","Health":{"Status":"starting"}}}"#));
            }
            replies.push(Daemon::reply(200, readiness));
            if healthy {
                replies.push(Daemon::reply(204, ""));
            }
            replies.push(Daemon::reply(204, ""));
            let daemon = Daemon::new(replies);
            let result = Client::new(&daemon.socket).start_many(&[
                "aaaaaaaaaaaa".into(),
                "bbbbbbbbbbbb".into(),
                "cccccccccccc".into(),
            ]);
            assert_eq!(result.is_ok(), healthy);
            if !healthy {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("dependency postgres")
                );
            }
            let requests = daemon.response.join().unwrap();
            let mut expected = vec![
                "GET /containers/json?all=1 HTTP/1.0",
                "POST /containers/bbbbbbbbbbbb/start HTTP/1.0",
            ];
            if healthy {
                expected.push("GET /containers/bbbbbbbbbbbb/json HTTP/1.0");
            }
            expected.push("GET /containers/bbbbbbbbbbbb/json HTTP/1.0");
            if healthy {
                expected.push("POST /containers/aaaaaaaaaaaa/start HTTP/1.0");
            }
            expected.push("POST /containers/cccccccccccc/start HTTP/1.0");
            let lines: Vec<_> = requests
                .iter()
                .map(|request| request.lines().next().unwrap())
                .collect();
            assert_eq!(lines, expected);
        }
    }

    #[test]
    fn single_start_includes_only_its_compose_dependencies_and_waits_for_health() {
        let inventory = r#"[{"Id":"aaaaaaaaaaaa","Names":["/account"],"Image":"account","State":"exited","Labels":{"com.docker.compose.project":"cloud","com.docker.compose.service":"account","com.docker.compose.depends_on":"postgres:service_healthy:true"}},{"Id":"cccccccccccc","Names":["/other-postgres"],"Image":"postgres","State":"exited","Labels":{"com.docker.compose.project":"a-other","com.docker.compose.service":"postgres"}},{"Id":"bbbbbbbbbbbb","Names":["/postgres"],"Image":"postgres","State":"exited","Labels":{"com.docker.compose.project":"cloud","com.docker.compose.service":"postgres"}},{"Id":"dddddddddddd","Names":["/relay"],"Image":"relay","State":"exited","Labels":{"com.docker.compose.project":"cloud","com.docker.compose.service":"relay"}}]"#;
        for healthy in [true, false] {
            let readiness = if healthy {
                r#"{"State":{"Running":true,"Status":"running","Health":{"Status":"healthy"}}}"#
            } else {
                r#"{"State":{"Running":false,"Status":"exited","ExitCode":255}}"#
            };
            let mut replies = vec![
                Daemon::reply(200, inventory),
                Daemon::reply(204, ""),
                Daemon::reply(200, readiness),
            ];
            if healthy {
                replies.push(Daemon::reply(204, ""));
            }
            let daemon = Daemon::new(replies);
            let result = Client::new(&daemon.socket).start_many(&["aaaaaaaaaaaa".into()]);
            assert_eq!(result.is_ok(), healthy);
            let requests = daemon.response.join().unwrap();
            let mut expected = vec![
                "GET /containers/json?all=1 HTTP/1.0",
                "POST /containers/bbbbbbbbbbbb/start HTTP/1.0",
                "GET /containers/bbbbbbbbbbbb/json HTTP/1.0",
            ];
            if healthy {
                expected.push("POST /containers/aaaaaaaaaaaa/start HTTP/1.0");
            }
            let actual: Vec<_> = requests
                .iter()
                .map(|request| request.lines().next().unwrap())
                .collect();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn inventory_retries_interrupted_reads_with_a_bounded_budget() {
        let daemon = Daemon::new(vec![Reply::Stalled, Daemon::reply(200, "[]")]);
        assert!(Client::new(&daemon.socket).list().unwrap().is_empty());
        assert_eq!(daemon.response.join().unwrap().len(), 2);
        for failures in [1, 3] {
            let mut replies = Vec::new();
            for _ in 0..failures {
                replies.push(Reply::Http(
                    "HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\n[".into(),
                ));
            }
            if failures == 1 {
                replies.push(Daemon::reply(200, "[]"));
            }
            let daemon = Daemon::new(replies);
            let result = Client::new(&daemon.socket).list();
            if failures == 1 {
                assert!(result.unwrap().is_empty());
            } else {
                assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
            }
            let requests = daemon.response.join().unwrap();
            assert_eq!(requests.len(), if failures == 1 { 2 } else { 3 });
            for request in requests {
                assert_eq!(
                    request.lines().next(),
                    Some("GET /containers/json?all=1 HTTP/1.0")
                );
            }
        }
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
            let daemon = Daemon::new(vec![Daemon::reply(status, "")]);
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
