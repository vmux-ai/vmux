use crate::safe_storage::{SafeStorage, SafeStorageContext, SafeStorageName};
use crate::{Profile, ProfilePaths};

pub struct CredentialStore {
    profile: Profile,
    namespace: String,
    context: SafeStorageContext,
}

impl CredentialStore {
    pub fn current(namespace: &str) -> Self {
        Self::at(
            Profile::current(),
            ProfilePaths::current().application_data(),
            namespace,
        )
    }

    pub fn at(profile: Profile, application_data: std::path::PathBuf, namespace: &str) -> Self {
        Self {
            profile,
            namespace: namespace.into(),
            context: SafeStorageContext::at(application_data),
        }
    }

    pub fn read(&self, name: &str) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>, String> {
        let account = self.account(name)?;
        let Some(envelope) = self
            .file(&account)
            .read()
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        SafeStorage::open_mcp(&self.context, &account, &envelope)
            .map(Some)
            .map_err(|error| error.to_string())
    }

    pub fn write(&self, name: &str, value: &[u8]) -> Result<(), String> {
        let account = self.account(name)?;
        let envelope = SafeStorage::seal_mcp(&self.context, &account, value)
            .map_err(|error| error.to_string())?;
        self.file(&account)
            .write(&envelope)
            .map_err(|error| error.to_string())
    }

    fn account(&self, name: &str) -> Result<String, String> {
        serde_json::to_string(&(self.profile.id(), &self.namespace, name))
            .map_err(|error| error.to_string())
    }

    fn file(&self, account: &str) -> crate::safe_storage::ProtectedFile {
        self.context.protected_file(
            std::path::PathBuf::from("credentials")
                .join(format!("{}.bin", SafeStorageName::of(account))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_keys_are_encrypted_and_bound_to_the_profile_and_provider_namespace() {
        let fixture = tempfile::tempdir().unwrap();
        let mut store = CredentialStore::current("acp");
        store.profile = Profile::named("test-profile");
        store.context = SafeStorageContext::for_environment(fixture.path().to_path_buf(), true);
        store.write("agent", b"private-test-api-key").unwrap();
        assert_eq!(
            &*store.read("agent").unwrap().unwrap(),
            b"private-test-api-key"
        );
        let account = store.account("agent").unwrap();
        let envelope = store.file(&account).read().unwrap().unwrap();
        assert!(
            !envelope
                .windows(b"private-test-api-key".len())
                .any(|window| window == b"private-test-api-key")
        );
        store.profile = Profile::named("another-profile");
        let other = store.account("agent").unwrap();
        store.file(&other).write(&envelope).unwrap();
        assert!(store.read("agent").is_err());
    }
}
