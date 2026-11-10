use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use zeroize::Zeroize;

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OauthCredentials {
    pub client_id: String,
    pub token_endpoint: String,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub token_type: String,
}

impl Debug for OauthCredentials {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OauthCredentials")
            .field("client_id", &self.client_id)
            .field("token_endpoint", &self.token_endpoint)
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .field("scopes", &self.scopes)
            .field("token_type", &self.token_type)
            .finish()
    }
}

impl Drop for OauthCredentials {
    fn drop(&mut self) {
        self.access_token.zeroize();
        if let Some(refresh_token) = &mut self.refresh_token {
            refresh_token.zeroize();
        }
    }
}

impl OauthCredentials {
    pub fn expires_soon(&self) -> bool {
        self.expires_soon_at(chrono::Utc::now().timestamp())
    }

    pub fn expires_soon_at(&self, now: i64) -> bool {
        self.expires_at != 0 && self.expires_at <= now.saturating_add(60)
    }

    pub fn expires_at(expires_in: Option<u64>) -> i64 {
        let Some(expires_in) = expires_in else {
            return 0;
        };
        let lifetime = i64::try_from(expires_in).unwrap_or(i64::MAX);
        chrono::Utc::now().timestamp().saturating_add(lifetime)
    }
}

#[derive(Clone, Debug)]
pub struct OauthCredentialAccess {
    transaction: Arc<Mutex<()>>,
    refresh: Arc<Mutex<()>>,
    cached: Arc<Mutex<Option<OauthCredentials>>>,
    revision: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
}

impl Default for OauthCredentialAccess {
    fn default() -> Self {
        Self {
            transaction: Arc::new(Mutex::new(())),
            refresh: Arc::new(Mutex::new(())),
            cached: Arc::new(Mutex::new(None)),
            revision: Arc::new(AtomicU64::new(0)),
            active: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl OauthCredentialAccess {
    pub fn read<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _access = self.transaction.lock().map_err(|error| error.to_string())?;
        self.require_active()?;
        let result = operation()?;
        self.require_active()?;
        Ok(result)
    }

    pub fn write<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _access = self.transaction.lock().map_err(|error| error.to_string())?;
        self.require_active()?;
        self.revision.fetch_add(1, Ordering::AcqRel);
        let result = operation();
        self.revision.fetch_add(1, Ordering::AcqRel);
        self.require_active()?;
        result
    }

    pub fn refresh<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _refresh = self.refresh.lock().map_err(|error| error.to_string())?;
        self.require_active()?;
        operation()
    }

    pub fn read_versioned<T>(
        &self,
        operation: impl FnOnce() -> Result<T, String>,
    ) -> Result<(u64, T), String> {
        let _access = self.transaction.lock().map_err(|error| error.to_string())?;
        self.require_active()?;
        let revision = self.revision.load(Ordering::Acquire);
        let value = operation()?;
        self.require_active()?;
        Ok((revision, value))
    }

    pub fn write_if_revision<T>(
        &self,
        revision: u64,
        operation: impl FnOnce() -> Result<T, String>,
    ) -> Result<Option<T>, String> {
        let _access = self.transaction.lock().map_err(|error| error.to_string())?;
        if !self.active.load(Ordering::Acquire)
            || self.revision.load(Ordering::Acquire) != revision
            || !revision.is_multiple_of(2)
        {
            return Ok(None);
        }
        self.revision.fetch_add(1, Ordering::AcqRel);
        let result = operation();
        self.revision.fetch_add(1, Ordering::AcqRel);
        if !self.active.load(Ordering::Acquire) {
            return Ok(None);
        }
        result.map(Some)
    }

    pub fn invalidate(&self) {
        self.active.store(false, Ordering::Release);
        let _access = self.transaction.lock();
        self.revision.fetch_add(2, Ordering::AcqRel);
        if let Ok(mut cached) = self.cached.lock() {
            cached.take();
        }
    }

    pub fn cleanup<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _access = self.transaction.lock().map_err(|error| error.to_string())?;
        if self.active.load(Ordering::Acquire) {
            return Err("Connection is still active".to_string());
        }
        self.revision.fetch_add(1, Ordering::AcqRel);
        let result = operation();
        self.revision.fetch_add(1, Ordering::AcqRel);
        result
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub fn cached(&self) -> Result<Option<OauthCredentials>, String> {
        self.cached
            .lock()
            .map(|cached| cached.clone())
            .map_err(|error| error.to_string())
    }

    pub fn cache(&self, credentials: OauthCredentials) -> Result<(), String> {
        let mut cached = self.cached.lock().map_err(|error| error.to_string())?;
        *cached = Some(credentials);
        Ok(())
    }

    fn require_active(&self) -> Result<(), String> {
        if !self.active.load(Ordering::Acquire) {
            return Err("Connection is disconnected".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_redacts_tokens() {
        let mut credentials = OauthCredentials::default();
        credentials.access_token = "access-secret".to_string();
        credentials.refresh_token = Some("refresh-secret".to_string());
        let debug = format!("{credentials:?}");

        assert!(!debug.contains("access-secret"));
        assert!(!debug.contains("refresh-secret"));
    }

    #[test]
    fn tokens_without_expiry_do_not_refresh() {
        let credentials = OauthCredentials::default();

        assert!(!credentials.expires_soon_at(i64::MAX));
    }

    #[test]
    fn tokens_refresh_with_one_minute_left() {
        let mut credentials = OauthCredentials::default();
        credentials.expires_at = 160;

        assert!(credentials.expires_soon_at(100));
        assert!(!credentials.expires_soon_at(99));
    }

    #[test]
    fn revisioned_write_rejects_stale_credentials() {
        let access = OauthCredentialAccess::default();
        let (revision, ()) = access.read_versioned(|| Ok(())).unwrap();
        access.write(|| Ok(())).unwrap();

        assert_eq!(
            access.write_if_revision(revision, || Ok("stale")).unwrap(),
            None
        );
    }

    #[test]
    fn invalidation_clears_cached_credentials() {
        let access = OauthCredentialAccess::default();
        let mut credentials = OauthCredentials::default();
        credentials.access_token = "secret".into();
        access.cache(credentials).unwrap();

        access.invalidate();

        assert_eq!(access.cached().unwrap(), None);
    }

    #[test]
    fn concurrent_refreshes_wait_for_the_fresh_credentials() {
        let access = OauthCredentialAccess::default();
        let first_access = access.clone();
        let (started_sender, started_receiver) = std::sync::mpsc::sync_channel(1);
        let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(1);
        let first = std::thread::spawn(move || {
            first_access.refresh(|| {
                started_sender.send(()).unwrap();
                release_receiver.recv().unwrap();
                Ok("first")
            })
        });
        started_receiver.recv().unwrap();
        let second_access = access.clone();
        let (second_sender, second_receiver) = std::sync::mpsc::sync_channel(1);
        let second = std::thread::spawn(move || {
            second_sender
                .send(second_access.refresh(|| Ok("second")))
                .unwrap();
        });

        assert!(
            second_receiver
                .recv_timeout(std::time::Duration::from_millis(20))
                .is_err()
        );
        release_sender.send(()).unwrap();

        assert_eq!(first.join().unwrap().unwrap(), "first");
        assert_eq!(second_receiver.recv().unwrap().unwrap(), "second");
        second.join().unwrap();
    }

    #[test]
    fn invalidation_clears_credentials_cached_by_an_in_flight_read() {
        let access = OauthCredentialAccess::default();
        let reader_access = access.clone();
        let (started_sender, started_receiver) = std::sync::mpsc::sync_channel(1);
        let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            reader_access.read(|| {
                started_sender.send(()).unwrap();
                release_receiver.recv().unwrap();
                let mut credentials = OauthCredentials::default();
                credentials.access_token = "stale".into();
                reader_access.cache(credentials)
            })
        });
        started_receiver.recv().unwrap();
        let invalidated_access = access.clone();
        let invalidation = std::thread::spawn(move || invalidated_access.invalidate());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while access.active.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(!access.active.load(Ordering::Acquire));
        release_sender.send(()).unwrap();

        assert!(reader.join().unwrap().is_err());
        invalidation.join().unwrap();
        assert_eq!(access.cached().unwrap(), None);
    }
}
