//! Turning a registered desktop into something a phone can scan.
//!
//! Both the app and `vmux remote` hand out pairing links, and a phone that follows one is stuck
//! with whatever it says — a wrong port or a missing fingerprint surfaces much later as a stalled
//! connection. Built here so the two agree by construction.
use std::time::{Duration, Instant};

use std::path::Path;

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
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Record the relay the app resolved, so the daemon can read it.
    ///
    /// launchd starts the daemon, so it inherits no environment; whoever enables Remote has to
    /// write down what `VMUX_REMOTE_RELAY_URL` said or the daemon will only ever see the default.
    pub fn persist(&self) -> std::io::Result<()> {
        let path = crate::remote_relay_url_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, self.url.trim().trim_end_matches('/'))?;
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
    read_trimmed(&crate::remote_relay_port_path())?.parse().ok()
}

/// The fingerprint of the certificate the desktop presents, as recorded beside it.
fn recorded_fingerprint() -> Option<String> {
    read_trimmed(&crate::remote_fingerprint_path())
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
}
