use serde::{Deserialize, Serialize};

static MCP_CREDENTIAL_ACCESS: std::sync::OnceLock<std::sync::Mutex<()>> =
    std::sync::OnceLock::new();
static MCP_CREDENTIAL_REFRESH_ACCESS: std::sync::OnceLock<std::sync::Mutex<()>> =
    std::sync::OnceLock::new();
static MCP_CREDENTIAL_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

const DEFAULT_TOKEN_LIFETIME_SECS: u64 = 3600;

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "ai.vmux.mcp";

#[cfg(target_os = "macos")]
struct McpCredentialFile {
    account: String,
    file: crate::safe_storage::ProtectedFile,
}

#[cfg(target_os = "macos")]
impl McpCredentialFile {
    fn new(account: &str) -> Self {
        let path = crate::application_data_dir()
            .join("safe-storage")
            .join("mcp")
            .join(format!(
                "{}.bin",
                crate::safe_storage::encoded_file_name(account)
            ));
        Self {
            account: account.to_string(),
            file: crate::safe_storage::ProtectedFile::new(path),
        }
    }

    fn load(&self) -> Result<Option<McpOauthCredentials>, String> {
        let Some(envelope) = self.file.read()? else {
            return Ok(None);
        };
        let plaintext = crate::safe_storage::SafeStorage::open_mcp(&self.account, &envelope)?;
        McpOauthCredentials::decode(&plaintext).map(Some)
    }

    fn store(&self, bytes: &[u8]) -> Result<(), String> {
        let envelope = crate::safe_storage::SafeStorage::seal_mcp(&self.account, bytes)?;
        self.file.write(&envelope)
    }

    fn remove(&self) -> Result<(), String> {
        self.file.remove()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpOauthCredentials {
    pub token_endpoint: String,
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub resource: String,
}

impl McpOauthCredentials {
    pub fn read_transaction<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _access = MCP_CREDENTIAL_ACCESS
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        operation()
    }

    pub fn write_transaction<T>(
        operation: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let _access = MCP_CREDENTIAL_ACCESS
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        MCP_CREDENTIAL_REVISION.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let result = operation();
        MCP_CREDENTIAL_REVISION.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        result
    }

    pub fn refresh_transaction<T>(
        operation: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let _access = MCP_CREDENTIAL_REFRESH_ACCESS
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        operation()
    }

    pub fn stable_revision() -> Result<u64, String> {
        let _access = MCP_CREDENTIAL_ACCESS
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        Ok(Self::revision())
    }

    pub fn revision() -> u64 {
        MCP_CREDENTIAL_REVISION.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn with_revision<T>(
        revision: u64,
        operation: impl FnOnce() -> T,
    ) -> Result<Option<T>, String> {
        let _access = match MCP_CREDENTIAL_ACCESS
            .get_or_init(Default::default)
            .try_lock()
        {
            Ok(access) => access,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
            Err(std::sync::TryLockError::Poisoned(error)) => return Err(error.to_string()),
        };
        if Self::revision() != revision || !revision.is_multiple_of(2) {
            return Ok(None);
        }
        Ok(Some(operation()))
    }

    pub fn load(server: &str) -> Result<Option<Self>, String> {
        Self::load_account(&Self::account(server))
    }

    pub fn store(&self, server: &str) -> Result<(), String> {
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        Self::store_account(&Self::account(server), &bytes)
    }

    pub fn remove(server: &str) -> Result<(), String> {
        Self::remove_account(&Self::account(server))
    }

    pub fn expires_soon(&self) -> bool {
        self.expires_at != 0 && self.expires_at <= chrono::Utc::now().timestamp() + 60
    }

    pub fn expires_at(expires_in: Option<u64>) -> i64 {
        let lifetime = expires_in.unwrap_or(DEFAULT_TOKEN_LIFETIME_SECS);
        let lifetime = i64::try_from(lifetime).unwrap_or(i64::MAX);
        chrono::Utc::now().timestamp().saturating_add(lifetime)
    }

    pub fn authorizes(&self, resource: &str) -> bool {
        let Ok(stored) = url::Url::parse(&self.resource) else {
            return false;
        };
        let Ok(requested) = url::Url::parse(resource) else {
            return false;
        };
        stored.scheme() == "https" && requested.scheme() == "https" && stored == requested
    }

    fn account(server: &str) -> String {
        format!("{}:{server}", crate::active_profile_name())
    }

    fn decode(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|error| error.to_string())
    }

    fn migrate_legacy(
        credentials: Option<Self>,
        store: impl FnOnce(&[u8]) -> Result<(), String>,
    ) -> Result<Option<Self>, String> {
        let Some(credentials) = credentials else {
            return Ok(None);
        };
        let bytes = serde_json::to_vec(&credentials).map_err(|error| error.to_string())?;
        store(&bytes)?;
        Ok(Some(credentials))
    }
}

#[cfg(target_os = "macos")]
impl McpOauthCredentials {
    fn load_account(account: &str) -> Result<Option<Self>, String> {
        let file = McpCredentialFile::new(account);
        if let Some(credentials) = file.load()? {
            return Ok(Some(credentials));
        }
        Self::migrate_legacy(Self::load_legacy_keychain_account(account)?, |bytes| {
            file.store(bytes)
        })
    }

    fn store_account(account: &str, bytes: &[u8]) -> Result<(), String> {
        Self::decode(bytes)?;
        McpCredentialFile::new(account).store(bytes)
    }

    fn remove_account(account: &str) -> Result<(), String> {
        McpCredentialFile::new(account).remove()?;
        Self::remove_legacy_keychain_account(account)
    }

    fn load_legacy_keychain_account(account: &str) -> Result<Option<Self>, String> {
        use security_framework::passwords::{PasswordOptions, generic_password};
        use security_framework_sys::base::errSecItemNotFound;

        crate::safe_storage::require_desktop_process()?;
        let options = PasswordOptions::new_generic_password(KEYCHAIN_SERVICE, account);
        match generic_password(options) {
            Ok(bytes) => Self::decode(&bytes).map(Some),
            Err(error) if error.code() == errSecItemNotFound => Ok(None),
            Err(error) => Err(format!("failed to load MCP credentials: {error}")),
        }
    }

    fn remove_legacy_keychain_account(account: &str) -> Result<(), String> {
        use security_framework_sys::base::errSecItemNotFound;

        crate::safe_storage::require_desktop_process()?;
        match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, account) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == errSecItemNotFound => Ok(()),
            Err(error) => Err(format!("failed to remove MCP credentials: {error}")),
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl McpOauthCredentials {
    fn load_account(account: &str) -> Result<Option<Self>, String> {
        let path = Self::path(account);
        match std::fs::read(path) {
            Ok(bytes) => Self::decode(&bytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn store_account(account: &str, bytes: &[u8]) -> Result<(), String> {
        use std::io::Write;

        let path = Self::path(account);
        let Some(parent) = path.parent() else {
            return Err("MCP credential path has no parent".to_string());
        };
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }
        file.write_all(bytes).map_err(|error| error.to_string())
    }

    fn remove_account(account: &str) -> Result<(), String> {
        match std::fs::remove_file(Self::path(account)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn path(account: &str) -> std::path::PathBuf {
        crate::profile_dir().join("mcp-credentials").join(format!(
            "{}.json",
            crate::safe_storage::encoded_file_name(account)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_ACCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn expiry_keeps_tokens_with_more_than_a_minute_left() {
        let credentials = McpOauthCredentials {
            expires_at: chrono::Utc::now().timestamp() + 61,
            ..McpOauthCredentials::default()
        };

        assert!(!credentials.expires_soon());
    }

    #[test]
    fn expiry_refreshes_tokens_with_a_minute_left() {
        let credentials = McpOauthCredentials {
            expires_at: chrono::Utc::now().timestamp() + 60,
            ..McpOauthCredentials::default()
        };

        assert!(credentials.expires_soon());
    }

    #[test]
    fn missing_expiry_uses_a_bounded_lifetime() {
        let before = chrono::Utc::now().timestamp() + DEFAULT_TOKEN_LIFETIME_SECS as i64;
        let expires_at = McpOauthCredentials::expires_at(None);
        let after = chrono::Utc::now().timestamp() + DEFAULT_TOKEN_LIFETIME_SECS as i64;

        assert!((before..=after).contains(&expires_at));
    }

    #[test]
    fn credential_file_names_preserve_distinct_accounts() {
        assert_ne!(
            crate::safe_storage::encoded_file_name("work.dev"),
            crate::safe_storage::encoded_file_name("work-dev")
        );
        assert_ne!(
            crate::safe_storage::encoded_file_name("linear"),
            crate::safe_storage::encoded_file_name("linear.")
        );
    }

    #[test]
    fn oauth_credentials_only_authorize_the_registered_resource() {
        let credentials = McpOauthCredentials {
            resource: "https://mcp.linear.app/mcp".to_string(),
            ..McpOauthCredentials::default()
        };

        assert!(credentials.authorizes("https://mcp.linear.app/mcp"));
        assert!(credentials.authorizes("https://MCP.LINEAR.APP:443/mcp"));
        assert!(!credentials.authorizes("https://example.com/mcp"));
        assert!(!credentials.authorizes("http://mcp.linear.app/mcp"));
    }

    #[test]
    fn legacy_credentials_are_preserved_while_migrating() {
        let credentials = McpOauthCredentials {
            token_endpoint: "https://auth.example.com/token".to_string(),
            client_id: "client".to_string(),
            client_secret: Some("secret".to_string()),
            access_token: "access".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at: 42,
            scope: "read write".to_string(),
            resource: "https://mcp.example.com".to_string(),
        };
        let mut migrated = Vec::new();

        let loaded = McpOauthCredentials::migrate_legacy(Some(credentials.clone()), |bytes| {
            migrated.extend_from_slice(bytes);
            Ok(())
        })
        .unwrap();

        assert_eq!(loaded, Some(credentials.clone()));
        assert_eq!(McpOauthCredentials::decode(&migrated).unwrap(), credentials);
    }

    #[test]
    fn credential_writes_invalidate_prepared_launches() {
        let _test = TEST_ACCESS.lock().unwrap();
        let revision = McpOauthCredentials::stable_revision().unwrap();
        assert_eq!(
            McpOauthCredentials::with_revision(revision, || "current").unwrap(),
            Some("current")
        );

        McpOauthCredentials::write_transaction(|| Ok(())).unwrap();

        assert_eq!(
            McpOauthCredentials::with_revision(revision, || "stale").unwrap(),
            None
        );

        let revision = McpOauthCredentials::stable_revision().unwrap();
        let result: Result<(), String> =
            McpOauthCredentials::write_transaction(|| Err("possibly changed".to_string()));
        assert!(result.is_err());
        assert_eq!(
            McpOauthCredentials::with_revision(revision, || "current").unwrap(),
            None
        );
    }

    #[test]
    fn launch_validation_does_not_wait_for_credential_writes() {
        let _test = TEST_ACCESS.lock().unwrap();
        let revision = McpOauthCredentials::stable_revision().unwrap();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            McpOauthCredentials::write_transaction(|| {
                started_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
                Ok(())
            })
            .unwrap();
        });
        started_rx.recv().unwrap();

        assert_eq!(
            McpOauthCredentials::with_revision(revision, || "stale").unwrap(),
            None
        );

        finish_tx.send(()).unwrap();
        writer.join().unwrap();
    }
}
