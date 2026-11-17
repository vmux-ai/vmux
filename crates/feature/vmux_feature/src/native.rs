use std::io::Read as _;
use std::time::Duration;

use bevy_ecs::prelude::*;
use reqwest::Url;
use sha2::{Digest, Sha256};
use vmux_ecs::Job;

use crate::administration::{self, Snapshot};

#[derive(Component, Clone)]
#[component(immutable)]
pub struct Endpoint {
    url: Url,
    pub(crate) environment: String,
    credential: Option<String>,
}

impl Endpoint {
    pub fn new(url: &str, environment: &str, credential: Option<String>) -> Result<Self, String> {
        let url = Url::parse(url).map_err(|error| error.to_string())?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if (url.scheme() != "https" && !(local && url.scheme() == "http"))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || environment.is_empty()
            || environment.len() > 64
            || !environment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || credential.as_ref().is_some_and(|token| {
                token.is_empty()
                    || token.len() > 256
                    || !token.bytes().all(|byte| byte.is_ascii_graphic())
            })
        {
            return Err("Invalid feature administration endpoint".into());
        }
        Ok(Self {
            url,
            environment: environment.into(),
            credential,
        })
    }

    pub(crate) fn writable(&self) -> bool {
        self.credential.is_some()
    }

    pub fn is_loopback(&self) -> bool {
        matches!(
            self.url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        )
    }

    pub(crate) fn fingerprint(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        for value in [
            self.url.as_str(),
            self.environment.as_str(),
            self.credential.as_deref().unwrap_or(""),
        ] {
            digest.update((value.len() as u64).to_be_bytes());
            digest.update(value.as_bytes());
        }
        digest.finalize().into()
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, String> {
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .build()
            .map_err(|error| error.to_string())?;
        let url = self.url.join(path).map_err(|error| error.to_string())?;
        let mut request = client.request(method, url);
        if let Some(credential) = &self.credential {
            request = request.bearer_auth(credential);
        }
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body);
        }
        let response = request.send().map_err(|error| error.to_string())?;
        let status = response.status();
        let mut bytes = Vec::new();
        response
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Feature response exceeds 2 MiB".into());
        }
        if !status.is_success() {
            let detail = String::from_utf8_lossy(&bytes);
            return Err(format!(
                "HTTP {status}: {}",
                detail.chars().take(512).collect::<String>()
            ));
        }
        Ok(bytes)
    }
}

#[derive(Clone)]
pub(crate) struct Inspect {
    pub endpoint: Endpoint,
}

impl Job for Inspect {
    type Output = Result<crate::snapshot::Document, String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }

    fn run(self) -> Self::Output {
        let path = format!(
            "/v1/flags/snapshot?environment={}",
            self.endpoint.environment
        );
        let bytes = self.endpoint.request(reqwest::Method::GET, &path, None)?;
        crate::snapshot::Document::decode(&bytes)
    }
}

#[derive(Clone)]
pub(crate) struct Load {
    pub endpoint: Endpoint,
}

impl Job for Load {
    type Output = Result<Snapshot, String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }

    fn run(self) -> Self::Output {
        let bytes =
            self.endpoint
                .request(reqwest::Method::GET, "/v1/flags/administration", None)?;
        Snapshot::decode(&bytes, &self.endpoint.environment)
    }
}

#[derive(Clone)]
pub(crate) struct Save {
    pub endpoint: Endpoint,
    pub request: administration::Save,
}

impl Job for Save {
    type Output = Result<Snapshot, String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }

    fn run(self) -> Self::Output {
        let body = serde_json::to_vec(&self.request).map_err(|error| error.to_string())?;
        let bytes = self
            .endpoint
            .request(reqwest::Method::POST, "/v1/flags/draft", Some(body))?;
        Snapshot::decode(&bytes, &self.endpoint.environment)
    }
}

#[derive(Clone)]
pub(crate) struct Review {
    pub endpoint: Endpoint,
    pub request: administration::Review,
}

impl Job for Review {
    type Output = Result<Snapshot, String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }

    fn run(self) -> Self::Output {
        let body = serde_json::to_vec(&self.request).map_err(|error| error.to_string())?;
        let bytes = self
            .endpoint
            .request(reqwest::Method::POST, "/v1/flags/review", Some(body))?;
        Snapshot::decode(&bytes, &self.endpoint.environment)
    }
}

#[derive(Clone)]
pub(crate) struct Publish {
    pub endpoint: Endpoint,
    pub request: administration::Publish,
}

impl Job for Publish {
    type Output = Result<Snapshot, String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }

    fn run(self) -> Self::Output {
        let body = serde_json::to_vec(&self.request).map_err(|error| error.to_string())?;
        let bytes =
            self.endpoint
                .request(reqwest::Method::POST, "/v1/flags/publish", Some(body))?;
        Snapshot::decode(&bytes, &self.endpoint.environment)
    }
}

#[derive(Clone)]
pub(crate) struct Discard {
    pub endpoint: Endpoint,
    pub request: administration::Discard,
}

impl Job for Discard {
    type Output = Result<Snapshot, String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }

    fn run(self) -> Self::Output {
        let body = serde_json::to_vec(&self.request).map_err(|error| error.to_string())?;
        let bytes =
            self.endpoint
                .request(reqwest::Method::POST, "/v1/flags/discard", Some(body))?;
        Snapshot::decode(&bytes, &self.endpoint.environment)
    }
}
