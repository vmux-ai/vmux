use std::time::{Duration, Instant};

use std::path::Path;

use crate::paths::RemotePaths;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Relay {
    url: String,
}

impl Relay {
    pub const DEFAULT_URL: &'static str = "https://relay.vmux.ai";

    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    pub fn from_env() -> Self {
        Self::resolve(std::env::var("VMUX_REMOTE_RELAY_URL").ok().as_deref())
    }

    pub fn configured() -> Self {
        let from_env = std::env::var("VMUX_REMOTE_RELAY_URL").ok();
        let path = RemotePaths::current().relay_url();
        let persisted = std::fs::read_to_string(&path).ok();
        let relay = Self::configured_from(from_env.as_deref(), persisted.as_deref());

        if let Some(recorded) = &persisted
            && !recorded.trim().is_empty()
            && PersistedRelay::parse(recorded).is_none()
        {
            tracing::warn!(
                recorded = %recorded.trim(),
                path = %path.display(),
                dialling = %relay.url(),
                "remote relay: the recorded relay was not written for this transport, so the \
                 port it names is not one this build can dial"
            );
        }
        relay
    }

    fn configured_from(from_env: Option<&str>, persisted: Option<&str>) -> Self {
        if let Some(from_env) = from_env
            && let Some(relay) = Self::normalized(from_env)
        {
            return relay;
        }
        if let Some(persisted) = persisted
            && let Some(recorded) = PersistedRelay::parse(persisted)
        {
            return recorded.relay();
        }
        Self::resolve(None)
    }

    pub fn resolve(from_env: Option<&str>) -> Self {
        let asked = from_env.unwrap_or(Self::DEFAULT_URL);
        Self::normalized(asked).unwrap_or_else(|| Self::new(Self::DEFAULT_URL))
    }

    pub fn normalized(url: &str) -> Option<Self> {
        let trimmed = url.trim().trim_end_matches('/');
        (!trimmed.is_empty()).then(|| Self::new(trimmed))
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn persist(&self) -> std::io::Result<()> {
        let path = RemotePaths::current().relay_url();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, PersistedRelay::of(self).contents())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn base_url(&self) -> Result<Option<String>, String> {
        if recorded_device().is_none() {
            return Ok(None);
        }
        let parsed = url::Url::parse(&self.url).map_err(|error| error.to_string())?;
        let host = parsed.host_str().ok_or("relay url has no host")?;
        let scheme = parsed.scheme();
        match parsed.port() {
            Some(port) => Ok(Some(format!("{scheme}://{host}:{port}"))),
            None => Ok(Some(format!("{scheme}://{host}"))),
        }
    }

    pub fn registered_device(&self) -> Option<String> {
        recorded_device()
    }

    pub fn pairing(&self, token: &str) -> Result<Option<PairingInfo>, String> {
        let (Some(base_url), Some(device), Some(fingerprint)) =
            (self.base_url()?, recorded_device(), recorded_fingerprint())
        else {
            return Ok(None);
        };
        PairingInfo::new(&base_url, token, &fingerprint, &device).map(Some)
    }

    pub fn wait_for_pairing(&self, token: &str, timeout: Duration) -> std::io::Result<String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(pairing) = self.pairing(token).map_err(std::io::Error::other)? {
                return Ok(pairing.url);
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("{} has not registered this desktop yet", self.url()),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedRelay {
    relay: Relay,
}

impl PersistedRelay {
    const TRANSPORT: &'static str = "quic";

    pub fn of(relay: &Relay) -> Self {
        Self {
            relay: Relay::new(relay.url().trim().trim_end_matches('/')),
        }
    }

    pub fn parse(contents: &str) -> Option<Self> {
        let (transport, url) = contents.trim().split_once(char::is_whitespace)?;
        if transport != Self::TRANSPORT {
            return None;
        }
        let relay = Relay::normalized(url)?;
        Some(Self { relay })
    }

    pub fn contents(&self) -> String {
        format!("{} {}", Self::TRANSPORT, self.relay.url())
    }

    pub fn relay(self) -> Relay {
        self.relay
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingInfo {
    pub url: String,
    pub deep_link: String,
}

impl PairingInfo {
    pub fn new(
        base_url: &str,
        token: &str,
        fingerprint: &str,
        device: &str,
    ) -> Result<Self, String> {
        let mut url = url::Url::parse(base_url).map_err(|error| error.to_string())?;
        url.set_fragment(Some(&if fingerprint.is_empty() {
            format!("token={token}&device={device}")
        } else {
            format!("token={token}&fp={fingerprint}&device={device}")
        }));

        let mut deep_link = url::Url::parse("vmux://pair").map_err(|error| error.to_string())?;
        deep_link
            .query_pairs_mut()
            .append_pair("base", base_url)
            .append_pair("token", token);
        if !fingerprint.is_empty() {
            deep_link.query_pairs_mut().append_pair("fp", fingerprint);
        }
        deep_link.query_pairs_mut().append_pair("device", device);

        Ok(Self {
            url: url.to_string(),
            deep_link: deep_link.to_string(),
        })
    }
}

fn recorded_device() -> Option<String> {
    read_trimmed(&RemotePaths::current().relay_registration())
}

fn recorded_fingerprint() -> Option<String> {
    read_trimmed(&RemotePaths::current().fingerprint())
}

fn read_trimmed(path: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    let trimmed = contents.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_reaches_both_pairing_shapes() {
        let pairing =
            PairingInfo::new("https://relay.vmux.ai", "secret", "abc123", "dev-1").unwrap();

        assert_eq!(
            pairing.url,
            "https://relay.vmux.ai/#token=secret&fp=abc123&device=dev-1"
        );
        assert_eq!(
            pairing.deep_link,
            "vmux://pair?base=https%3A%2F%2Frelay.vmux.ai&token=secret&fp=abc123&device=dev-1"
        );
    }

    #[test]
    fn an_absent_fingerprint_leaves_both_shapes_well_formed() {
        let pairing = PairingInfo::new("https://relay.vmux.ai", "secret", "", "dev-1").unwrap();

        assert_eq!(
            pairing.url,
            "https://relay.vmux.ai/#token=secret&device=dev-1"
        );
        assert_eq!(
            pairing.deep_link,
            "vmux://pair?base=https%3A%2F%2Frelay.vmux.ai&token=secret&device=dev-1"
        );
    }

    #[test]
    fn every_pairing_shape_names_a_device() {
        let pairing =
            PairingInfo::new("https://relay.vmux.ai", "secret", "abc123", "dev-2").unwrap();

        assert!(pairing.url.contains("device=dev-2"));
        assert!(pairing.deep_link.contains("device=dev-2"));
    }

    #[test]
    fn a_blank_relay_setting_falls_back_to_the_hosted_one() {
        for (from_env, expected) in [
            (None, Relay::DEFAULT_URL),
            (Some(""), Relay::DEFAULT_URL),
            (Some("   "), Relay::DEFAULT_URL),
            (
                Some("https://relay.example.com/"),
                "https://relay.example.com",
            ),
            (Some("  https://localhost:8788  "), "https://localhost:8788"),
        ] {
            assert_eq!(
                Relay::resolve(from_env).url(),
                expected,
                "from_env = {from_env:?}"
            );
        }
    }

    #[test]
    fn an_untagged_relay_file_is_not_a_dial_target() {
        for contents in [
            "https://localhost:8788",
            "https://relay.vmux.ai",
            "   https://localhost:8788   ",
            "",
            "   ",
            "http2 https://localhost:8788",
        ] {
            assert_eq!(
                PersistedRelay::parse(contents),
                None,
                "contents = {contents:?}"
            );
        }
    }

    #[test]
    fn a_tagged_relay_file_is_honoured_whatever_port_it_names() {
        for (contents, expected) in [
            ("quic https://localhost:8788", "https://localhost:8788"),
            ("quic https://localhost:8787", "https://localhost:8787"),
            (
                "quic https://relay.example.com:9443/",
                "https://relay.example.com:9443",
            ),
            ("  quic   https://relay.vmux.ai  ", "https://relay.vmux.ai"),
        ] {
            let parsed = PersistedRelay::parse(contents).expect("tagged file should parse");
            assert_eq!(parsed.relay().url(), expected, "contents = {contents:?}");
        }
    }

    #[test]
    fn what_this_build_writes_reads_back_as_the_same_relay() {
        let relay = Relay::new("  https://relay.example.com:9443/  ");
        let contents = PersistedRelay::of(&relay).contents();

        assert_eq!(
            PersistedRelay::parse(&contents).map(PersistedRelay::relay),
            Some(Relay::new("https://relay.example.com:9443"))
        );
    }

    #[test]
    fn the_dial_target_prefers_the_environment_then_a_tagged_file() {
        for (from_env, persisted, expected) in [
            (None, None, Relay::DEFAULT_URL),
            (None, Some("https://localhost:8788"), Relay::DEFAULT_URL),
            (
                None,
                Some("quic https://localhost:8788"),
                "https://localhost:8788",
            ),
            (
                Some("https://localhost:8787"),
                Some("quic https://localhost:9999"),
                "https://localhost:8787",
            ),
            (
                Some("   "),
                Some("quic https://localhost:9999"),
                "https://localhost:9999",
            ),
            (
                Some("   "),
                Some("https://localhost:8788"),
                Relay::DEFAULT_URL,
            ),
        ] {
            assert_eq!(
                Relay::configured_from(from_env, persisted).url(),
                expected,
                "from_env = {from_env:?}, persisted = {persisted:?}"
            );
        }
    }
}
