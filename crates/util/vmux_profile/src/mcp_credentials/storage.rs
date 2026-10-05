use super::McpOauthCredentials;
use crate::{Profile, ProfilePaths};

#[cfg(target_os = "macos")]
use crate::safe_storage::{ProtectedFile, SafeStorage, SafeStorageContext, SafeStorageName};

#[cfg(not(target_os = "macos"))]
use std::io::Write;
#[cfg(all(not(target_os = "macos"), unix))]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

#[derive(Clone, Debug)]
pub struct McpCredentialStorage {
    profile: Profile,
    paths: ProfilePaths,
}

#[cfg(target_os = "macos")]
struct McpCredentialFile {
    context: SafeStorageContext,
    account: String,
    file: ProtectedFile,
}

impl McpCredentialStorage {
    pub fn at(profile: Profile, paths: ProfilePaths) -> Self {
        Self { profile, paths }
    }

    pub fn load(&self, server: &str) -> Result<Option<McpOauthCredentials>, String> {
        self.load_account(&self.account(server))
    }

    pub fn store(&self, server: &str, credentials: &McpOauthCredentials) -> Result<(), String> {
        let bytes = serde_json::to_vec(credentials).map_err(|error| error.to_string())?;
        self.store_account(&self.account(server), &bytes)
    }

    pub fn remove(&self, server: &str) -> Result<(), String> {
        self.remove_account(&self.account(server))
    }

    fn account(&self, server: &str) -> String {
        format!("{}:{server}", self.profile)
    }

    fn decode(bytes: &[u8]) -> Result<McpOauthCredentials, String> {
        serde_json::from_slice(bytes).map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "macos")]
impl McpCredentialFile {
    fn new(account: &str, application_data: std::path::PathBuf) -> Self {
        let context = SafeStorageContext::at(application_data);
        let file = context.protected_file(
            std::path::PathBuf::from("mcp").join(format!("{}.bin", SafeStorageName::of(account))),
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
    fn load_account(&self, account: &str) -> Result<Option<McpOauthCredentials>, String> {
        McpCredentialFile::new(account, self.paths.application_data()).load()
    }

    fn store_account(&self, account: &str, bytes: &[u8]) -> Result<(), String> {
        Self::decode(bytes)?;
        McpCredentialFile::new(account, self.paths.application_data()).store(bytes)
    }

    fn remove_account(&self, account: &str) -> Result<(), String> {
        McpCredentialFile::new(account, self.paths.application_data()).remove()
    }
}

#[cfg(not(target_os = "macos"))]
impl McpCredentialStorage {
    fn load_account(&self, account: &str) -> Result<Option<McpOauthCredentials>, String> {
        match std::fs::read(self.path(account)) {
            Ok(bytes) => Self::decode(&bytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn store_account(&self, account: &str, bytes: &[u8]) -> Result<(), String> {
        let path = self.path(account);
        let Some(parent) = path.parent() else {
            return Err("MCP credential path has no parent".to_string());
        };
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options.open(path).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }
        file.write_all(bytes).map_err(|error| error.to_string())
    }

    fn remove_account(&self, account: &str) -> Result<(), String> {
        match std::fs::remove_file(self.path(account)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn path(&self, account: &str) -> std::path::PathBuf {
        self.paths.profile().join("mcp-credentials").join(format!(
            "{}.json",
            crate::safe_storage::SafeStorageName::of(account)
        ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn credential_file_names_preserve_distinct_accounts() {
        assert_ne!(
            crate::safe_storage::SafeStorageName::of("work.dev"),
            crate::safe_storage::SafeStorageName::of("work-dev")
        );
        assert_ne!(
            crate::safe_storage::SafeStorageName::of("linear"),
            crate::safe_storage::SafeStorageName::of("linear.")
        );
    }
}
