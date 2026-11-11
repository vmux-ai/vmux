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
            self.network
                .post("/services/forwarder/unexpose", &request)?
                .require_success()?;
            self.active.remove(&port);
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
            self.network
                .post("/services/forwarder/expose", &request)?
                .require_success()?;
            self.active.insert(port);
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

    fn local(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
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
