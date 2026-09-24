use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use vmux_remote::{ClientCredential, DeviceId};

use crate::RemotePaths;

const MIN_TOKEN_LEN: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayToken(String);

impl RelayToken {
    pub fn ensure() -> std::io::Result<Self> {
        let path = RemotePaths::current().relay_token();
        if let Some(token) = Self::read(&path) {
            return Ok(Self(token));
        }
        let token = Token::generate();
        PrivateFile::new(path).write(token.as_bytes())?;
        Ok(Self(token))
    }

    pub fn wait(timeout: Duration) -> std::io::Result<Self> {
        let path = RemotePaths::current().relay_token();
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(token) = Self::read(&path) {
                return Ok(Self(token));
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("relay token not created: {}", path.display()),
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn read(path: &Path) -> Option<String> {
        let token = std::fs::read_to_string(path).ok()?;
        let token = token.trim();
        (token.len() >= MIN_TOKEN_LEN).then(|| token.to_string())
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

#[derive(Clone, Debug)]
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
        Ok(file
            .devices
            .into_iter()
            .map(|device| AuthorizedDevice {
                id: device.id,
                authorized_at_unix: device.authorized_at_unix,
            })
            .collect())
    }

    pub fn authenticate(
        &self,
        client_id: &DeviceId,
        credential: &ClientCredential,
    ) -> std::io::Result<Option<AuthorizationOutcome>> {
        let mut file = self.load_or_create()?;
        match credential {
            ClientCredential::Device(token) => {
                let accepted = file
                    .devices
                    .iter()
                    .any(|device| device.id == *client_id && device.token_hash.matches(token));
                Ok(accepted.then_some(AuthorizationOutcome::Accepted))
            }
            ClientCredential::Pairing(token) => {
                if !Token::secure_eq(token, &file.pairing_token) {
                    return Ok(None);
                }
                let device_token = Token::generate();
                let authorization = StoredDevice {
                    id: client_id.clone(),
                    token_hash: TokenHash::of(&device_token),
                    authorized_at_unix: chrono::Utc::now().timestamp(),
                };
                if let Some(existing) = file
                    .devices
                    .iter_mut()
                    .find(|device| device.id == *client_id)
                {
                    *existing = authorization;
                } else {
                    file.devices.push(authorization);
                }
                file.devices.sort_by(|left, right| left.id.cmp(&right.id));
                file.pairing_token = Token::generate();
                self.save(&file)?;
                Ok(Some(AuthorizationOutcome::Paired { device_token }))
            }
        }
    }

    pub fn authorizes(&self, client_id: &DeviceId, device_token: &str) -> std::io::Result<bool> {
        let file = self.load_or_create()?;
        Ok(file
            .devices
            .iter()
            .any(|device| device.id == *client_id && device.token_hash.matches(device_token)))
    }

    pub fn revoke(&self, client_id: &DeviceId) -> std::io::Result<bool> {
        let mut file = self.load_or_create()?;
        let before = file.devices.len();
        file.devices.retain(|device| device.id != *client_id);
        if file.devices.len() == before {
            return Ok(false);
        }
        self.save(&file)?;
        Ok(true)
    }

    pub fn reset(&self) -> std::io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn load_or_create(&self) -> std::io::Result<StoredAuthorizations> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid remote authorization store: {error}"),
                )
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let file = StoredAuthorizations::new();
                self.save(&file)?;
                Ok(file)
            }
            Err(error) => Err(error),
        }
    }

    fn save(&self, file: &StoredAuthorizations) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(file).map_err(std::io::Error::other)?;
        PrivateFile::new(self.path.clone()).write(&bytes)
    }
}

struct PrivateFile(PathBuf);

impl PrivateFile {
    fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        vmux_path::AtomicFile::write(&self.0, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

struct Token;

impl Token {
    fn generate() -> String {
        format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )
    }

    fn secure_eq(left: &str, right: &str) -> bool {
        if left.len() != right.len() {
            return false;
        }
        left.bytes()
            .zip(right.bytes())
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredAuthorizations {
    version: u32,
    pairing_token: String,
    devices: Vec<StoredDevice>,
}

impl StoredAuthorizations {
    fn new() -> Self {
        Self {
            version: 1,
            pairing_token: Token::generate(),
            devices: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredDevice {
    id: DeviceId,
    token_hash: TokenHash,
    authorized_at_unix: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
struct TokenHash(String);

impl TokenHash {
    fn of(token: &str) -> Self {
        let hash = digest(&SHA256, token.as_bytes());
        let mut encoded = String::with_capacity(hash.as_ref().len() * 2);
        for byte in hash.as_ref() {
            let _ = write!(encoded, "{byte:02x}");
        }
        Self(encoded)
    }

    fn matches(&self, token: &str) -> bool {
        Token::secure_eq(&self.0, &Self::of(token).0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Store {
        _directory: tempfile::TempDir,
        file: RemoteAuthorizationStore,
    }

    impl Store {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let file = RemoteAuthorizationStore::new(directory.path().join("authorizations.json"));
            Self {
                _directory: directory,
                file,
            }
        }
    }

    #[test]
    fn pairing_issues_a_device_token_and_rotates_the_pairing_token() {
        let store = Store::new();
        let pairing_token = store.file.pairing_token().unwrap();
        let client = DeviceId::new("phone-a");

        let outcome = store
            .file
            .authenticate(&client, &ClientCredential::Pairing(pairing_token.clone()))
            .unwrap()
            .unwrap();
        let AuthorizationOutcome::Paired { device_token } = outcome else {
            panic!("pairing did not issue a device token")
        };

        assert!(store.file.authorizes(&client, &device_token).unwrap());
        assert_ne!(store.file.pairing_token().unwrap(), pairing_token);
        assert_eq!(
            store
                .file
                .authenticate(
                    &DeviceId::new("phone-b"),
                    &ClientCredential::Pairing(pairing_token)
                )
                .unwrap(),
            None
        );
    }

    #[test]
    fn each_device_has_an_independent_revocable_token() {
        let store = Store::new();
        let first = DeviceId::new("phone-a");
        let second = DeviceId::new("phone-b");
        let first_token = store.file.pairing_token().unwrap();
        let AuthorizationOutcome::Paired {
            device_token: first_token,
        } = store
            .file
            .authenticate(&first, &ClientCredential::Pairing(first_token))
            .unwrap()
            .unwrap()
        else {
            panic!("first pairing failed")
        };
        let second_token = store.file.pairing_token().unwrap();
        let AuthorizationOutcome::Paired {
            device_token: second_token,
        } = store
            .file
            .authenticate(&second, &ClientCredential::Pairing(second_token))
            .unwrap()
            .unwrap()
        else {
            panic!("second pairing failed")
        };

        assert!(store.file.revoke(&first).unwrap());
        assert!(!store.file.authorizes(&first, &first_token).unwrap());
        assert!(store.file.authorizes(&second, &second_token).unwrap());
        assert_eq!(store.file.devices().unwrap().len(), 1);
    }
}
