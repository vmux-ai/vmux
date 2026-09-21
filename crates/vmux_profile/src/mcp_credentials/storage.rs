use super::McpOauthCredentials;

#[cfg(target_os = "macos")]
use crate::safe_storage::{ProtectedFile, SafeStorage, SafeStorageContext, encoded_file_name};

pub struct McpCredentialStorage;

#[cfg(target_os = "macos")]
struct McpCredentialFile {
    context: SafeStorageContext,
    account: String,
    file: ProtectedFile,
}

impl McpCredentialStorage {
    pub fn load(server: &str) -> Result<Option<McpOauthCredentials>, String> {
        Self::load_account(&Self::account(server))
    }

    pub fn store(server: &str, credentials: &McpOauthCredentials) -> Result<(), String> {
        let bytes = serde_json::to_vec(credentials).map_err(|error| error.to_string())?;
        Self::store_account(&Self::account(server), &bytes)
    }

    pub fn remove(server: &str) -> Result<(), String> {
        Self::remove_account(&Self::account(server))
    }

    fn account(server: &str) -> String {
        format!("{}:{server}", crate::active_profile_name())
    }

    fn decode(bytes: &[u8]) -> Result<McpOauthCredentials, String> {
        serde_json::from_slice(bytes).map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "macos")]
impl McpCredentialFile {
    fn new(account: &str) -> Self {
        let context = SafeStorageContext::current();
        let file = context.protected_file(
            std::path::PathBuf::from("mcp").join(format!("{}.bin", encoded_file_name(account))),
        );
        Self {
            context,
            account: account.to_string(),
            file,
        }
    }

    fn load(&self) -> Result<Option<McpOauthCredentials>, String> {
        let Some(envelope) = self.file.read().map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        let plaintext = SafeStorage::open_mcp(&self.context, &self.account, &envelope)
            .map_err(|error| error.to_string())?;
        McpCredentialStorage::decode(&plaintext).map(Some)
    }

    fn store(&self, bytes: &[u8]) -> Result<(), String> {
        let envelope = SafeStorage::seal_mcp(&self.context, &self.account, bytes)
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
impl McpCredentialStorage {
    fn load_account(account: &str) -> Result<Option<McpOauthCredentials>, String> {
        McpCredentialFile::new(account).load()
    }

    fn store_account(account: &str, bytes: &[u8]) -> Result<(), String> {
        Self::decode(bytes)?;
        McpCredentialFile::new(account).store(bytes)
    }

    fn remove_account(account: &str) -> Result<(), String> {
        McpCredentialFile::new(account).remove()
    }
}

#[cfg(not(target_os = "macos"))]
impl McpCredentialStorage {
    fn load_account(account: &str) -> Result<Option<McpOauthCredentials>, String> {
        match std::fs::read(Self::path(account)) {
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
}
