use std::time::{Duration, Instant};

use super::file_driver::PrivateFile;
use super::pairing::{PairingInfo, PersistedRelay, Relay};
use crate::RemotePaths;

impl Relay {
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
                "remote relay: the recorded relay was not written for this transport, so the port it names is not one this build can dial"
            );
        }
        relay
    }

    pub fn persist(&self) -> std::io::Result<()> {
        let path = RemotePaths::current().relay_url();
        PrivateFile::new(path).write(PersistedRelay::from(self).contents())
    }

    pub fn base_url(&self) -> Result<Option<String>, String> {
        if RemoteRegistration::current().device().is_none() {
            return Ok(None);
        }
        let parsed = url::Url::parse(self.url()).map_err(|error| error.to_string())?;
        let host = parsed.host_str().ok_or("relay url has no host")?;
        let scheme = parsed.scheme();
        match parsed.port() {
            Some(port) => Ok(Some(format!("{scheme}://{host}:{port}"))),
            None => Ok(Some(format!("{scheme}://{host}"))),
        }
    }

    pub fn registered_device(&self) -> Option<String> {
        RemoteRegistration::current().device()
    }

    pub fn pairing(
        &self,
        relay_token: &str,
        pairing_token: &str,
    ) -> Result<Option<PairingInfo>, String> {
        let registration = RemoteRegistration::current();
        let (Some(base_url), Some(device), Some(fingerprint)) = (
            self.base_url()?,
            registration.device(),
            registration.fingerprint(),
        ) else {
            return Ok(None);
        };
        PairingInfo::new(&base_url, relay_token, pairing_token, &fingerprint, &device).map(Some)
    }

    pub fn wait_for_pairing(
        &self,
        relay_token: &str,
        pairing_token: &str,
        timeout: Duration,
    ) -> std::io::Result<String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(pairing) = self
                .pairing(relay_token, pairing_token)
                .map_err(std::io::Error::other)?
            {
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

struct RemoteRegistration(RemotePaths);

impl RemoteRegistration {
    fn current() -> Self {
        Self(RemotePaths::current())
    }

    fn device(&self) -> Option<String> {
        Self::read(&self.0.relay_registration())
    }

    fn fingerprint(&self) -> Option<String> {
        Self::read(&self.0.fingerprint())
    }

    fn read(path: &std::path::Path) -> Option<String> {
        let contents = std::fs::read_to_string(path).ok()?;
        let trimmed = contents.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }
}
