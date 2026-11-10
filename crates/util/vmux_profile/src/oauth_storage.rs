use crate::oauth::OauthCredentials;
use crate::{Profile, ProfilePaths};
use std::time::Duration;
use zeroize::Zeroizing;

#[cfg(target_os = "macos")]
use crate::safe_storage::{ProtectedFile, SafeStorage, SafeStorageContext, SafeStorageName};

#[derive(Clone, Debug)]
pub struct OauthCredentialStorage {
    profile: Profile,
    #[cfg(target_os = "macos")]
    paths: ProfilePaths,
}

#[cfg(target_os = "macos")]
struct OauthCredentialFile {
    context: SafeStorageContext,
    account: String,
    file: ProtectedFile,
}

impl OauthCredentialStorage {
    pub fn at(profile: Profile, paths: ProfilePaths) -> Self {
        #[cfg(not(target_os = "macos"))]
        let _ = paths;
        Self {
            profile,
            #[cfg(target_os = "macos")]
            paths,
        }
    }

    pub fn load(
        &self,
        provider: &str,
        connection: &str,
    ) -> Result<Option<OauthCredentials>, String> {
        let account = self.account(provider, connection);
        if let Some(credentials) = self.load_account(&account, provider, connection)? {
            return Ok(Some(credentials));
        }
        let legacy_account = self.legacy_account(provider, connection);
        let Some(credentials) = self.load_account(&legacy_account, provider, connection)? else {
            return Ok(None);
        };
        let bytes =
            Zeroizing::new(serde_json::to_vec(&credentials).map_err(|error| error.to_string())?);
        self.store_account(&account, provider, connection, &bytes)?;
        self.remove_account(&legacy_account, provider, connection)?;
        Ok(Some(credentials))
    }

    pub fn load_retrying(
        &self,
        provider: &str,
        connection: &str,
        attempts: usize,
    ) -> Result<Option<OauthCredentials>, String> {
        let attempts = attempts.max(1);
        let mut last_error = None;
        for attempt in 0..attempts {
            match self.load(provider, connection) {
                Ok(credentials) => return Ok(credentials),
                Err(error) => last_error = Some(error),
            }
            if attempt + 1 < attempts {
                std::thread::sleep(Duration::from_millis(50 * (attempt as u64 + 1)));
            }
        }
        Err(last_error.unwrap_or_else(|| "OAuth credential load failed".to_string()))
    }

    pub fn store(
        &self,
        provider: &str,
        connection: &str,
        credentials: &OauthCredentials,
    ) -> Result<(), String> {
        let bytes =
            Zeroizing::new(serde_json::to_vec(credentials).map_err(|error| error.to_string())?);
        self.store_account(
            &self.account(provider, connection),
            provider,
            connection,
            &bytes,
        )?;
        self.remove_account(
            &self.legacy_account(provider, connection),
            provider,
            connection,
        )
    }

    pub fn remove(&self, provider: &str, connection: &str) -> Result<(), String> {
        let current =
            self.remove_account(&self.account(provider, connection), provider, connection);
        let legacy = self.remove_account(
            &self.legacy_account(provider, connection),
            provider,
            connection,
        );
        match (current, legacy) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(current), Ok(())) => Err(current),
            (Ok(()), Err(legacy)) => Err(legacy),
            (Err(current), Err(legacy)) => Err(format!("{current}; {legacy}")),
        }
    }

    pub const fn available(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn account(&self, provider: &str, connection: &str) -> String {
        format!(
            "oauth-v2|{}:{}|{}:{}|{}:{}",
            self.profile.id().len(),
            self.profile.id(),
            provider.len(),
            provider,
            connection.len(),
            connection
        )
    }

    fn legacy_account(&self, provider: &str, connection: &str) -> String {
        format!("{}:{provider}:{connection}", self.profile)
    }

    #[cfg(target_os = "macos")]
    fn decode(bytes: &[u8]) -> Result<OauthCredentials, String> {
        serde_json::from_slice(bytes).map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "macos")]
impl OauthCredentialFile {
    fn new(account: &str, application_data: std::path::PathBuf) -> Self {
        let context = SafeStorageContext::at(application_data);
        let file = context.protected_file(
            std::path::PathBuf::from("oauth").join(format!("{}.bin", SafeStorageName::of(account))),
        );
        Self {
            context,
            account: account.to_string(),
            file,
        }
    }

    fn load(&self) -> Result<Option<OauthCredentials>, String> {
        let Some(envelope) = self.file.read().map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        let plaintext = SafeStorage::open_oauth(&self.context, &self.account, &envelope)
            .map_err(|error| error.to_string())?;
        OauthCredentialStorage::decode(&plaintext).map(Some)
    }

    fn store(&self, bytes: &[u8]) -> Result<(), String> {
        let envelope = SafeStorage::seal_oauth(&self.context, &self.account, bytes)
            .map_err(|error| error.to_string())?;
        self.file
            .write(&envelope)
            .map_err(|error| error.to_string())
    }

    fn remove(&self) -> Result<(), String> {
        self.file.remove().map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "macos")]
impl OauthCredentialStorage {
    fn load_account(
        &self,
        account: &str,
        provider: &str,
        connection: &str,
    ) -> Result<Option<OauthCredentials>, String> {
        let _ = (provider, connection);
        OauthCredentialFile::new(account, self.paths.application_data()).load()
    }

    fn store_account(
        &self,
        account: &str,
        provider: &str,
        connection: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        Self::decode(bytes)?;
        let _ = (provider, connection);
        OauthCredentialFile::new(account, self.paths.application_data()).store(bytes)
    }

    fn remove_account(
        &self,
        account: &str,
        provider: &str,
        connection: &str,
    ) -> Result<(), String> {
        let _ = (provider, connection);
        OauthCredentialFile::new(account, self.paths.application_data()).remove()
    }
}

#[cfg(not(target_os = "macos"))]
impl OauthCredentialStorage {
    fn load_account(
        &self,
        _account: &str,
        _provider: &str,
        _connection: &str,
    ) -> Result<Option<OauthCredentials>, String> {
        Ok(None)
    }

    fn store_account(
        &self,
        _account: &str,
        _provider: &str,
        _connection: &str,
        _bytes: &[u8],
    ) -> Result<(), String> {
        Err("Encrypted OAuth credential storage is unavailable on this platform".to_string())
    }

    fn remove_account(
        &self,
        _account: &str,
        _provider: &str,
        _connection: &str,
    ) -> Result<(), String> {
        Ok(())
    }
}
