//! Turning a registered desktop into something a phone can scan.
//!
//! Both the app and `vmux remote` hand out pairing links, and a phone that follows one is stuck
//! with whatever it says — a wrong port or a missing fingerprint surfaces much later as a stalled
//! connection. Built here so the two agree by construction.
use std::time::{Duration, Instant};

use std::path::Path;

use crate::paths::RemotePaths;

/// The relay a desktop pairs through.
///
/// Registration is asynchronous: the daemon dials out, the relay allocates it a port, and only
/// then can a link name somewhere a phone can reach. Every reader here returns `Option` for that
/// window rather than inventing a port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Relay {
    url: String,
}

impl Relay {
    /// The hosted relay, used when nothing overrides it.
    pub const DEFAULT_URL: &'static str = "https://relay.vmux.ai";

    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    /// The relay `VMUX_REMOTE_RELAY_URL` asks for, or the hosted one when it is unset.
    pub fn from_env() -> Self {
        Self::resolve(std::env::var("VMUX_REMOTE_RELAY_URL").ok().as_deref())
    }

    /// Which relay the daemon should dial.
    ///
    /// The persisted file is the normal source, not the fallback: launchd starts the daemon, so it
    /// inherits nothing from the shell that launched the app, and the environment is only ever set
    /// in a developer's terminal. Whoever enables Remote writes what it resolved with
    /// [`Relay::persist`] for exactly this reason.
    ///
    /// A file this build cannot vouch for is announced rather than dropped quietly — the dial that
    /// would follow it fails as a bare timeout, which says nothing about why.
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

    /// The same decision, over what the environment and the file actually said.
    ///
    /// A value typed into the environment is being chosen right now, so it is taken as given. The
    /// file is a record left by an earlier run and only counts when [`PersistedRelay`] can tell
    /// which transport wrote it.
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

    /// Decide the relay from what the environment said, if anything.
    ///
    /// Every pairing goes through a relay now, so a blank value means "use the hosted one" rather
    /// than "turn the relay off". There is no off: a desktop sits behind NAT and is unreachable
    /// without one.
    pub fn resolve(from_env: Option<&str>) -> Self {
        let asked = from_env.unwrap_or(Self::DEFAULT_URL);
        Self::normalized(asked).unwrap_or_else(|| Self::new(Self::DEFAULT_URL))
    }

    /// A relay whose URL has been trimmed to canonical form, or `None` when the URL is blank.
    pub fn normalized(url: &str) -> Option<Self> {
        let trimmed = url.trim().trim_end_matches('/');
        (!trimmed.is_empty()).then(|| Self::new(trimmed))
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Record the relay the app resolved, so the daemon can read it.
    ///
    /// launchd starts the daemon, so it inherits no environment; whoever enables Remote has to
    /// write down what `VMUX_REMOTE_RELAY_URL` said or the daemon will only ever see the default.
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

    /// Where the phone should dial: this relay's host, on the UDP port it allocated this desktop.
    ///
    /// The relay's own control port is discarded — that one is for desktops registering, and a
    /// phone sending there reaches no particular desktop. `None` until a port has been recorded.
    pub fn base_url(&self) -> Result<Option<String>, String> {
        let Some(port) = recorded_port() else {
            return Ok(None);
        };
        self.base_url_on(port).map(Some)
    }

    fn base_url_on(&self, port: u16) -> Result<String, String> {
        let parsed = url::Url::parse(&self.url).map_err(|error| error.to_string())?;
        let host = parsed.host_str().ok_or("relay url has no host")?;
        let scheme = parsed.scheme();
        Ok(format!("{scheme}://{host}:{port}"))
    }

    /// The complete link, or `None` until the daemon has recorded both a port and a certificate.
    pub fn pairing(&self, token: &str) -> Result<Option<PairingInfo>, String> {
        let (Some(base_url), Some(fingerprint)) = (self.base_url()?, recorded_fingerprint()) else {
            return Ok(None);
        };
        PairingInfo::new(&base_url, token, &fingerprint).map(Some)
    }

    /// Block until the relay has allocated a port for this desktop, then return the pairing link.
    ///
    /// The port comes from the relay and the fingerprint from the certificate the daemon loads, so
    /// neither exists until it has started and dialled out. Waiting beats printing a link that
    /// names a port nothing answers on.
    pub fn wait_for_pairing(&self, token: &str, timeout: Duration) -> std::io::Result<String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(pairing) = self.pairing(token).map_err(std::io::Error::other)? {
                return Ok(pairing.url);
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "{} has not allocated a port for this desktop yet",
                        self.url()
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// A relay URL as the file on disk spells it, tagged with the transport it was recorded for.
///
/// Builds before the QUIC cutover wrote the URL on its own, and that URL named an HTTP endpoint:
/// the relay served TCP then and serves UDP on a different port now. Dialling the recorded port
/// reaches nothing, and a UDP packet into the void is indistinguishable from a slow network, so
/// the whole story arrives as `relay connect: timed out`. Recording the transport is what lets a
/// value from before the cutover be recognised instead of inherited.
///
/// What is checked is the tag, never the port number. A relay someone deliberately put somewhere
/// other than the default was recorded by a build that writes the tag, so it survives whatever
/// port it names — which is the difference between a migration and throwing away a setting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedRelay {
    relay: Relay,
}

impl PersistedRelay {
    /// The transport a URL in this file is good for. A file naming any other — none, because it
    /// predates the tag, or one a later build introduced — is not a dial target for this build.
    const TRANSPORT: &'static str = "quic";

    /// How `relay` should be written down.
    pub fn of(relay: &Relay) -> Self {
        Self {
            relay: Relay::new(relay.url().trim().trim_end_matches('/')),
        }
    }

    /// The relay a file's contents name, or `None` when they are blank or name another transport.
    pub fn parse(contents: &str) -> Option<Self> {
        let (transport, url) = contents.trim().split_once(char::is_whitespace)?;
        if transport != Self::TRANSPORT {
            return None;
        }
        let relay = Relay::normalized(url)?;
        Some(Self { relay })
    }

    /// What the file should hold.
    pub fn contents(&self) -> String {
        format!("{} {}", Self::TRANSPORT, self.relay.url())
    }

    /// The relay this names.
    pub fn relay(self) -> Relay {
        self.relay
    }
}

/// Everything a phone needs to reach one desktop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingInfo {
    /// Carried by the QR code.
    pub url: String,
    /// Carried by the `vmuxremote://` deep link, for a phone already holding the device.
    pub deep_link: String,
}

impl PairingInfo {
    /// Build the pairing URL and deep link.
    ///
    /// `fingerprint` is passed in rather than read here so the result depends only on the
    /// arguments — reading the certificate from disk would make this answer differ between a
    /// machine that has started Remote and one that has not.
    pub fn new(base_url: &str, token: &str, fingerprint: &str) -> Result<Self, String> {
        let mut url = url::Url::parse(base_url).map_err(|error| error.to_string())?;
        url.set_fragment(Some(&if fingerprint.is_empty() {
            format!("token={token}")
        } else {
            format!("token={token}&fp={fingerprint}")
        }));

        let mut deep_link =
            url::Url::parse("vmuxremote://pair").map_err(|error| error.to_string())?;
        deep_link
            .query_pairs_mut()
            .append_pair("base", base_url)
            .append_pair("token", token);
        if !fingerprint.is_empty() {
            deep_link.query_pairs_mut().append_pair("fp", fingerprint);
        }

        Ok(Self {
            url: url.to_string(),
            deep_link: deep_link.to_string(),
        })
    }
}

/// The port the relay gave this desktop, as recorded by the daemon at registration.
fn recorded_port() -> Option<u16> {
    read_trimmed(&RemotePaths::current().relay_port())?
        .parse()
        .ok()
}

/// The fingerprint of the certificate the desktop presents, as recorded beside it.
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
    fn a_base_url_takes_the_relay_host_and_the_allocated_port() {
        assert_eq!(
            Relay::new("https://relay.vmux.ai")
                .base_url_on(41003)
                .unwrap(),
            "https://relay.vmux.ai:41003"
        );
        // The relay's own control port is replaced, not appended to.
        assert_eq!(
            Relay::new("https://localhost:8787")
                .base_url_on(41003)
                .unwrap(),
            "https://localhost:41003"
        );
    }

    /// The phone can only pin the desktop's certificate if the fingerprint survives into both
    /// pairing shapes — the QR-encoded URL and the deep link. Dropping it from either would
    /// downgrade that phone to an unpinned connection with nothing to show for it.
    #[test]
    fn a_fingerprint_reaches_both_pairing_shapes() {
        let pairing = PairingInfo::new("https://localhost:41003", "secret", "abc123").unwrap();

        assert_eq!(
            pairing.url,
            "https://localhost:41003/#token=secret&fp=abc123"
        );
        assert_eq!(
            pairing.deep_link,
            "vmuxremote://pair?base=https%3A%2F%2Flocalhost%3A41003&token=secret&fp=abc123"
        );
    }

    #[test]
    fn an_absent_fingerprint_leaves_both_shapes_well_formed() {
        let pairing = PairingInfo::new("https://localhost:41003", "secret", "").unwrap();

        assert_eq!(pairing.url, "https://localhost:41003/#token=secret");
        assert_eq!(
            pairing.deep_link,
            "vmuxremote://pair?base=https%3A%2F%2Flocalhost%3A41003&token=secret"
        );
    }

    /// There is no way to ask for no relay: a desktop behind NAT is unreachable without one, so
    /// a blank setting falls back to the hosted relay rather than disabling pairing.
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

    /// The file a pre-QUIC build left behind holds a bare URL naming the HTTP port. Dialling it
    /// over UDP reaches nothing and reports only a timeout, so an untagged value must not become
    /// the dial target however plausible it looks.
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

    /// Recognising the stale value cannot cost someone the relay they deliberately stood up on
    /// another port — the tag is what is checked, so a tagged non-default URL is honoured verbatim.
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

    /// A URL this build writes has to read back as itself, or every daemon restart would fall to
    /// the hosted relay and the migration would never end.
    #[test]
    fn what_this_build_writes_reads_back_as_the_same_relay() {
        let relay = Relay::new("  https://relay.example.com:9443/  ");
        let contents = PersistedRelay::of(&relay).contents();

        assert_eq!(
            PersistedRelay::parse(&contents).map(PersistedRelay::relay),
            Some(Relay::new("https://relay.example.com:9443"))
        );
    }

    /// What the daemon ends up dialling, over every combination of the two sources it has. The
    /// environment is a choice being made now and wins; the file only counts when tagged; and a
    /// stale file falls back to the hosted relay rather than to the port nothing answers on.
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
