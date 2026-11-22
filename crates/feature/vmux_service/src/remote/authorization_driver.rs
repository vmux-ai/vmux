use std::fmt::Write;
use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bevy::prelude::{Bundle, Component};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use vmux_transport::{ClientCredential, DeviceId};

use super::authorization::{AuthorizationAuthority, PairingCredential};
use super::file_driver::PrivateFile;
use crate::RemotePaths;

#[derive(Clone, Debug, Eq, PartialEq, Component)]
pub struct AccountRelayCredential {
    device_id: DeviceId,
    assertion: String,
}

impl AccountRelayCredential {
    pub fn new(device_id: DeviceId, assertion: String) -> Option<Self> {
        if !Self::valid_device_id(device_id.as_str()) || !Self::valid_assertion(&assertion) {
            return None;
        }
        Some(Self {
            device_id,
            assertion,
        })
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let mut lines = raw.lines().map(str::trim).filter(|line| !line.is_empty());
        let device_id = lines.next()?;
        let assertion = lines.next()?;
        if lines.next().is_some() {
            return None;
        }
        Self::new(DeviceId::new(device_id), assertion.to_string())
    }

    pub fn read_current() -> Option<Self> {
        Self::read_at(&RemotePaths::current().relay_assertion())
    }

    pub fn read_at(path: &Path) -> Option<Self> {
        let contents = std::fs::read_to_string(path).ok()?;
        Self::parse(&contents)
    }

    pub fn write_current(device_id: &DeviceId, assertion: &str) -> std::io::Result<()> {
        let paths = RemotePaths::current();
        Self::write_at(&paths.relay_assertion(), device_id, assertion)?;
        let _ = std::fs::remove_file(paths.relay_token());
        let _ = std::fs::remove_file(paths.relay_device());
        Ok(())
    }

    pub fn write_at(path: &Path, device_id: &DeviceId, assertion: &str) -> std::io::Result<()> {
        if Self::new(device_id.clone(), assertion.to_string()).is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid account relay assertion",
            ));
        }
        let body = format!("{}\n{}\n", device_id.as_str(), assertion);
        PrivateFile::new(path).write(body.as_bytes())
    }

    pub fn clear_current() {
        let paths = RemotePaths::current();
        let _ = std::fs::remove_file(paths.relay_assertion());
        let _ = std::fs::remove_file(paths.relay_token());
        let _ = std::fs::remove_file(paths.relay_device());
    }

    pub fn wait(timeout: Duration) -> std::io::Result<Self> {
        let path = RemotePaths::current().relay_assertion();
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(credential) = Self::read_at(&path) {
                return Ok(credential);
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Sign in to your Vmux account in the desktop app before enabling Live",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    pub fn assertion(&self) -> &str {
        &self.assertion
    }

    fn valid_device_id(raw: &str) -> bool {
        uuid::Uuid::parse_str(raw).is_ok_and(|id| !id.is_nil())
    }

    fn valid_assertion(raw: &str) -> bool {
        let Some(rest) = raw.strip_prefix("a1.") else {
            return false;
        };
        let Some((claims, signature)) = rest.split_once('.') else {
            return false;
        };
        !claims.is_empty()
            && !signature.is_empty()
            && !signature.contains('.')
            && claims.bytes().all(|byte| byte.is_ascii_hexdigit())
            && signature.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizedDevice {
    pub id: DeviceId,
    pub authorized_at_unix: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorizationOutcome {
    Accepted,
    Paired { device_token: String },
}

#[derive(Clone, Component)]
pub(crate) struct RemoteAuthorizations {
    authenticate_device: mpsc::UnboundedSender<AuthenticateDevice>,
    pair_device: mpsc::UnboundedSender<PairDevice>,
    revalidate: mpsc::UnboundedSender<RevalidateAuthorization>,
    revoke: mpsc::UnboundedSender<RevokeAuthorization>,
    reset: mpsc::UnboundedSender<ResetAuthorization>,
    wake: mpsc::UnboundedSender<()>,
}

impl RemoteAuthorizations {
    pub(crate) fn new(wake: mpsc::UnboundedSender<()>) -> (Self, impl Bundle) {
        Self::with_store(RemoteAuthorizationStore::current(), wake)
    }

    pub(crate) fn with_store(
        store: RemoteAuthorizationStore,
        wake: mpsc::UnboundedSender<()>,
    ) -> (Self, impl Bundle) {
        let (authenticate_device, authenticate_device_inbox) = mpsc::unbounded_channel();
        let (pair_device, pair_device_inbox) = mpsc::unbounded_channel();
        let (revalidate, revalidate_inbox) = mpsc::unbounded_channel();
        let (revoke, revoke_inbox) = mpsc::unbounded_channel();
        let (reset, reset_inbox) = mpsc::unbounded_channel();
        (
            Self {
                authenticate_device,
                pair_device,
                revalidate,
                revoke,
                reset,
                wake,
            },
            (
                AuthorizationAuthority::new(store.path),
                PairingCredential::default(),
                AuthorizationInbox(AuthorizationReceivers {
                    authenticate_device: authenticate_device_inbox,
                    pair_device: pair_device_inbox,
                    revalidate: revalidate_inbox,
                    revoke: revoke_inbox,
                    reset: reset_inbox,
                }),
            ),
        )
    }

    pub(crate) async fn authenticate(
        &self,
        client_id: DeviceId,
        credential: ClientCredential,
    ) -> std::io::Result<Option<AuthorizationOutcome>> {
        let (response, receiver) = oneshot::channel();
        match credential {
            ClientCredential::Device(device_token) => self
                .authenticate_device
                .send(AuthenticateDevice {
                    client_id,
                    device_token,
                    response: Some(response),
                })
                .map_err(|_| Self::unavailable())?,
            ClientCredential::Pairing(pairing_token) => self
                .pair_device
                .send(PairDevice {
                    client_id,
                    pairing_token,
                    response: Some(response),
                })
                .map_err(|_| Self::unavailable())?,
        }
        self.wake.send(()).map_err(|_| Self::unavailable())?;
        receiver.await.map_err(|_| Self::unavailable())?
    }

    pub(crate) async fn authorizes(
        &self,
        client_id: DeviceId,
        device_token: String,
    ) -> std::io::Result<bool> {
        let (response, receiver) = oneshot::channel();
        self.revalidate
            .send(RevalidateAuthorization {
                client_id,
                device_token,
                response: Some(response),
            })
            .map_err(|_| Self::unavailable())?;
        self.wake.send(()).map_err(|_| Self::unavailable())?;
        receiver.await.map_err(|_| Self::unavailable())?
    }

    #[allow(dead_code)]
    pub(crate) async fn revoke(&self, client_id: DeviceId) -> std::io::Result<bool> {
        let (response, receiver) = oneshot::channel();
        self.revoke
            .send(RevokeAuthorization {
                client_id,
                response: Some(response),
            })
            .map_err(|_| Self::unavailable())?;
        self.wake.send(()).map_err(|_| Self::unavailable())?;
        receiver.await.map_err(|_| Self::unavailable())?
    }

    #[allow(dead_code)]
    pub(crate) async fn reset(&self) -> std::io::Result<()> {
        let (response, receiver) = oneshot::channel();
        self.reset
            .send(ResetAuthorization {
                response: Some(response),
            })
            .map_err(|_| Self::unavailable())?;
        self.wake.send(()).map_err(|_| Self::unavailable())?;
        receiver.await.map_err(|_| Self::unavailable())?
    }

    fn unavailable() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "remote authorization runtime unavailable",
        )
    }
}

pub(super) struct AuthorizationReceivers {
    pub(super) authenticate_device: mpsc::UnboundedReceiver<AuthenticateDevice>,
    pub(super) pair_device: mpsc::UnboundedReceiver<PairDevice>,
    pub(super) revalidate: mpsc::UnboundedReceiver<RevalidateAuthorization>,
    pub(super) revoke: mpsc::UnboundedReceiver<RevokeAuthorization>,
    pub(super) reset: mpsc::UnboundedReceiver<ResetAuthorization>,
}

#[derive(Component)]
pub(super) struct AuthorizationInbox(pub(super) AuthorizationReceivers);

#[derive(Component)]
pub(super) struct AuthenticateDevice {
    pub(super) client_id: DeviceId,
    pub(super) device_token: String,
    pub(super) response: Option<oneshot::Sender<std::io::Result<Option<AuthorizationOutcome>>>>,
}

#[derive(Component)]
pub(super) struct PairDevice {
    pub(super) client_id: DeviceId,
    pub(super) pairing_token: String,
    pub(super) response: Option<oneshot::Sender<std::io::Result<Option<AuthorizationOutcome>>>>,
}

#[derive(Component)]
pub(super) struct RevalidateAuthorization {
    pub(super) client_id: DeviceId,
    pub(super) device_token: String,
    pub(super) response: Option<oneshot::Sender<std::io::Result<bool>>>,
}

#[derive(Component)]
pub(super) struct RevokeAuthorization {
    pub(super) client_id: DeviceId,
    pub(super) response: Option<oneshot::Sender<std::io::Result<bool>>>,
}

#[derive(Component)]
pub(super) struct ResetAuthorization {
    pub(super) response: Option<oneshot::Sender<std::io::Result<()>>>,
}

#[derive(Clone, Component, Debug)]
pub struct RemoteAuthorizationStore {
    path: PathBuf,
}

impl RemoteAuthorizationStore {
    pub fn current() -> Self {
        Self::new(RemotePaths::current().authorizations())
    }

    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        let _ = self.load_or_create()?;
        Ok(())
    }

    pub fn pairing_token(&self) -> std::io::Result<String> {
        Ok(self.load_or_create()?.pairing_token)
    }

    pub fn devices(&self) -> std::io::Result<Vec<AuthorizedDevice>> {
        let file = self.load_or_create()?;
        let mut devices = Vec::with_capacity(file.devices.len());
        for device in file.devices {
            devices.push(AuthorizedDevice {
                id: device.id,
                authorized_at_unix: device.authorized_at_unix,
            });
        }
        Ok(devices)
    }

    pub fn revoke(&self, client_id: &DeviceId) -> std::io::Result<bool> {
        let mut file = self.load_or_create()?;
        let before = file.devices.len();
        file.devices.retain(|device| device.id != *client_id);
        if file.devices.len() == before {
            return Ok(false);
        }
        AuthorizationFile::write(&self.path, &file)?;
        Ok(true)
    }

    pub fn reset(&self) -> std::io::Result<()> {
        AuthorizationFile::remove(&self.path)
    }

    fn load_or_create(&self) -> std::io::Result<StoredAuthorizations> {
        match AuthorizationFile::read(&self.path)? {
            Some((_, file)) => Ok(file),
            None => {
                let file = StoredAuthorizations::new();
                AuthorizationFile::write(&self.path, &file)?;
                Ok(file)
            }
        }
    }
}

pub(super) struct AuthorizationFile;

impl AuthorizationFile {
    pub(super) fn read(path: &Path) -> std::io::Result<Option<(u64, StoredAuthorizations)>> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let fingerprint = Self::fingerprint(&bytes);
                let parsed = serde_json::from_slice(&bytes).map_err(|error| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid remote authorization store: {error}"),
                    )
                })?;
                Ok(Some((fingerprint, parsed)))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) fn write(path: &Path, file: &StoredAuthorizations) -> std::io::Result<u64> {
        let bytes = serde_json::to_vec_pretty(file).map_err(std::io::Error::other)?;
        PrivateFile::new(path.to_path_buf()).write(&bytes)?;
        Ok(Self::fingerprint(&bytes))
    }

    pub(super) fn remove(path: &Path) -> std::io::Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn fingerprint(bytes: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(bytes);
        hasher.finish()
    }
}

pub(super) struct Token;

impl Token {
    pub(super) fn generate() -> String {
        format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )
    }

    pub(super) fn secure_eq(left: &str, right: &str) -> bool {
        if left.len() != right.len() {
            return false;
        }
        let mut difference = 0_u8;
        for (left, right) in left.bytes().zip(right.bytes()) {
            difference |= left ^ right;
        }
        difference == 0
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct StoredAuthorizations {
    pub(super) version: u32,
    pub(super) pairing_token: String,
    pub(super) devices: Vec<StoredDevice>,
}

impl StoredAuthorizations {
    pub(super) fn new() -> Self {
        Self {
            version: 1,
            pairing_token: Token::generate(),
            devices: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct StoredDevice {
    pub(super) id: DeviceId,
    pub(super) token_hash: TokenHash,
    pub(super) authorized_at_unix: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub(super) struct TokenHash(String);

impl TokenHash {
    pub(super) fn of(token: &str) -> Self {
        let hash = digest(&SHA256, token.as_bytes());
        let mut encoded = String::with_capacity(hash.as_ref().len() * 2);
        for byte in hash.as_ref() {
            let _ = write!(encoded, "{byte:02x}");
        }
        Self(encoded)
    }

    pub(super) fn matches(&self, token: &str) -> bool {
        Token::secure_eq(&self.0, &Self::of(token).0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_relay_credential_round_trips_and_rejects_anonymous_tokens() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("remote-relay-assertion");
        let device = DeviceId::new(uuid::Uuid::new_v4().to_string());
        let assertion = "a1.deadbeef.cafebabe";

        AccountRelayCredential::write_at(&path, &device, assertion).expect("write");
        let loaded = AccountRelayCredential::read_at(&path).expect("read");
        assert_eq!(loaded.device_id(), &device);
        assert_eq!(loaded.assertion(), assertion);

        assert!(
            AccountRelayCredential::parse(&format!("{}\nanonymous-token", device.as_str()))
                .is_none()
        );
        assert!(AccountRelayCredential::parse("not-a-uuid\na1.deadbeef.cafebabe").is_none());
        assert!(AccountRelayCredential::write_at(&path, &device, "anonymous-token").is_err());
    }
}
