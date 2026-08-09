//! Turning a registered desktop into something a phone can scan.
//!
//! Both the app and `vmux remote` hand out pairing links, and a phone that follows one is stuck
//! with whatever it says — a wrong port or a missing fingerprint surfaces much later as a stalled
//! connection. Built here so the two agree by construction.

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
#[path = "pairing.test.rs"]
mod tests;
