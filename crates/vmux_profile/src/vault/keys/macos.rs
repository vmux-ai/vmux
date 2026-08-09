//! macOS key storage: the login Keychain, reached directly or through the signed
//! `vmux vault-key` broker subprocess when one ships alongside the app.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroizing;

use super::LOCKED;
use super::native::decode_key_hex;
use crate::vault::{KEY_LEN, hex, random_hex, validate_key};

const KEYCHAIN_SERVICE: &str = "ai.vmux.vault";

impl super::DeviceKeys {
    pub(super) fn load(vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        let key = if key_broker_path().is_some() {
            load_key_from_broker(vault_id)?
        } else {
            load_keychain_key(vault_id)?
        };
        key.ok_or_else(|| LOCKED.to_string())
    }

    pub(super) fn load_silent(vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        let Some(output) = run_key_broker("load", vault_id, None, true)? else {
            return Ok(None);
        };
        if output.status.success() {
            let key = decode_key_hex(String::from_utf8_lossy(&output.stdout).trim())?;
            return Ok(Some(Zeroizing::new(key)));
        }
        if output.status.code() == Some(2) {
            return Ok(None);
        }
        Err(key_broker_error(&output))
    }

    pub(super) fn create() -> Result<Zeroizing<Vec<u8>>, String> {
        let mut key = Zeroizing::new(vec![0_u8; KEY_LEN]);
        SystemRandom::new()
            .fill(&mut key)
            .map_err(|_| "failed to generate Vault encryption key".to_string())?;
        Ok(key)
    }

    pub(super) fn store(vault_id: &str, key: &[u8]) -> Result<(), String> {
        if key_broker_path().is_some() {
            store_key_with_broker(vault_id, key)
        } else {
            store_keychain_key(KEYCHAIN_SERVICE, vault_id, key)
        }
    }

    pub(super) fn broker_load(vault_id: &str) -> Result<Option<String>, String> {
        use security_framework::passwords::generic_password;
        use security_framework_sys::base::errSecItemNotFound;

        match generic_password(keychain_options(KEYCHAIN_SERVICE, vault_id, false)) {
            Ok(key) => {
                validate_key(&key)?;
                Ok(Some(hex(&key)))
            }
            Err(error) if error.code() == errSecItemNotFound => Ok(None),
            Err(error) => Err(format!("failed to unlock Vault key: {error}")),
        }
    }

    pub(super) fn broker_load_silent(vault_id: &str) -> Result<Option<String>, String> {
        use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};

        let mut search = ItemSearchOptions::new();
        let results = search
            .class(ItemClass::generic_password())
            .service(KEYCHAIN_SERVICE)
            .account(vault_id)
            .load_data(true)
            .skip_authenticated_items(true)
            .search();
        let Ok(results) = results else {
            return Ok(None);
        };
        if let Some(key) = results.into_iter().find_map(|result| match result {
            SearchResult::Data(key) => Some(key),
            _ => None,
        }) {
            validate_key(&key)?;
            return Ok(Some(hex(&key)));
        }
        Ok(None)
    }

    pub(super) fn broker_store(vault_id: &str, encoded_key: &str) -> Result<(), String> {
        let key = decode_key_hex(encoded_key)?;
        store_keychain_key(KEYCHAIN_SERVICE, vault_id, &key)
    }

    pub(super) fn authorize_broker_parent() -> Result<(), String> {
        use security_framework::os::macos::code_signing::{
            Flags, GuestAttributes, SecCode, SecRequirement,
        };
        use std::str::FromStr;

        let parent_pid = unsafe { libc::getppid() };
        let mut attributes = GuestAttributes::new();
        attributes.set_pid(parent_pid);
        let parent = SecCode::copy_guest_with_attribues(None, &attributes, Flags::NONE)
            .map_err(|error| format!("failed to identify Vault key broker caller: {error}"))?;
        let broker = std::env::current_exe().map_err(|error| error.to_string())?;
        let certificate = signing_leaf_hash(&broker)?;
        let requirement = SecRequirement::from_str(&format!(
            "certificate leaf = H\"{certificate}\""
        ))
        .map_err(|error| format!("failed to create Vault key broker requirement: {error}"))?;
        validate_key_broker_caller(&parent, &requirement)?;

        let expected = broker
            .parent()
            .ok_or_else(|| "Vault key broker has no parent directory".to_string())?
            .join("vmux_desktop")
            .canonicalize()
            .map_err(|error| format!("failed to locate vmux desktop: {error}"))?;
        let actual = parent_process_path(parent_pid)?
            .canonicalize()
            .map_err(|error| format!("failed to resolve Vault key broker caller: {error}"))?;
        if actual != expected {
            return Err("Vault key broker rejected its caller".to_string());
        }
        Ok(())
    }
}

fn keychain_options(
    service: &str,
    vault_id: &str,
    synchronized: bool,
) -> security_framework::passwords::PasswordOptions {
    let mut options =
        security_framework::passwords::PasswordOptions::new_generic_password(service, vault_id);
    options.set_access_synchronized(Some(synchronized));
    options
}

fn validate_key_broker_caller(
    caller: &security_framework::os::macos::code_signing::SecCode,
    requirement: &security_framework::os::macos::code_signing::SecRequirement,
) -> Result<(), String> {
    use security_framework::os::macos::code_signing::Flags;

    caller
        .check_validity(Flags::NONE, requirement)
        .map_err(|_| "Vault key broker rejected its caller".to_string())
}

fn load_keychain_key(vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    use security_framework::passwords::generic_password;
    use security_framework_sys::base::errSecItemNotFound;

    match generic_password(keychain_options(KEYCHAIN_SERVICE, vault_id, false)) {
        Ok(key) => {
            validate_key(&key)?;
            Ok(Some(Zeroizing::new(key)))
        }
        Err(error) if error.code() == errSecItemNotFound => Ok(None),
        Err(error) => Err(format!("failed to unlock Vault encryption key: {error}")),
    }
}

fn store_keychain_key(service: &str, vault_id: &str, key: &[u8]) -> Result<(), String> {
    use security_framework::passwords::set_generic_password_options;

    validate_key(key)?;
    set_generic_password_options(key, keychain_options(service, vault_id, false))
        .map_err(|error| format!("failed to store Vault key in system Keychain: {error}"))
}

fn key_broker_path() -> Option<PathBuf> {
    if crate::build_profile() == "dev" {
        return None;
    }
    let path = std::env::current_exe().ok()?.parent()?.join("vmux");
    path.is_file().then_some(path)
}

fn parent_process_path(pid: libc::pid_t) -> Result<PathBuf, String> {
    use std::os::unix::ffi::OsStringExt;

    let mut buffer = vec![0_u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let length = unsafe {
        libc::proc_pidpath(
            pid,
            buffer.as_mut_ptr().cast(),
            buffer.len().try_into().unwrap_or(u32::MAX),
        )
    };
    if length <= 0 {
        return Err("failed to locate Vault key broker caller".to_string());
    }
    buffer.truncate(length as usize);
    Ok(PathBuf::from(std::ffi::OsString::from_vec(buffer)))
}

fn signing_leaf_hash(path: &Path) -> Result<String, String> {
    let directory =
        std::env::temp_dir().join(format!("vmux-vault-certificate-{}", random_hex(16)?));
    std::fs::create_dir(&directory)
        .map_err(|error| format!("failed to prepare Vault certificate check: {error}"))?;
    let output = Command::new("/usr/bin/codesign")
        .args(["--display", "--extract-certificates"])
        .arg(path)
        .current_dir(&directory)
        .output()
        .map_err(|error| format!("failed to inspect Vault key broker signature: {error}"))?;
    let certificate_path = directory.join("codesign0");
    let result = if output.status.success() {
        std::fs::read(&certificate_path)
            .map(|certificate| {
                hex(
                    ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, &certificate)
                        .as_ref(),
                )
            })
            .map_err(|error| format!("failed to read Vault key broker certificate: {error}"))
    } else {
        Err("Vault key broker is not signed with a certificate".to_string())
    };
    let _ = std::fs::remove_dir_all(directory);
    result
}

fn run_key_broker(
    action: &str,
    vault_id: &str,
    input: Option<&str>,
    silent: bool,
) -> Result<Option<Output>, String> {
    let Some(path) = key_broker_path() else {
        return Ok(None);
    };
    let mut command = Command::new(path);
    command
        .args(["vault-key", action, "--vault-id", vault_id])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if silent {
        command.arg("--no-ui");
    }
    if input.is_none() {
        return command
            .output()
            .map(Some)
            .map_err(|error| format!("failed to run Vault key broker: {error}"));
    }
    let mut child = command
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run Vault key broker: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "failed to open Vault key broker input".to_string())?
        .write_all(input.unwrap_or_default().as_bytes())
        .map_err(|error| format!("failed to send key to Vault key broker: {error}"))?;
    child
        .wait_with_output()
        .map(Some)
        .map_err(|error| format!("failed to wait for Vault key broker: {error}"))
}

fn load_key_from_broker(vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    let Some(output) = run_key_broker("load", vault_id, None, false)? else {
        return Ok(None);
    };
    if output.status.success() {
        let key = decode_key_hex(String::from_utf8_lossy(&output.stdout).trim())?;
        return Ok(Some(Zeroizing::new(key)));
    }
    if output.status.code() == Some(2) {
        return Ok(None);
    }
    Err(key_broker_error(&output))
}

fn store_key_with_broker(vault_id: &str, key: &[u8]) -> Result<(), String> {
    let encoded = Zeroizing::new(hex(key));
    let Some(output) = run_key_broker("store", vault_id, Some(&encoded), false)? else {
        return store_keychain_key(KEYCHAIN_SERVICE, vault_id, key);
    };
    if output.status.success() {
        Ok(())
    } else {
        Err(key_broker_error(&output))
    }
}

fn key_broker_error(output: &Output) -> String {
    let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if error.is_empty() {
        "Vault key broker failed".to_string()
    } else {
        error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_key_broker_reads_signing_leaf_certificate() {
        let hash = signing_leaf_hash(Path::new("/usr/bin/codesign")).unwrap();

        assert_eq!(hash.len(), 40);
        assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn vault_key_broker_validates_running_callers() {
        use security_framework::os::macos::code_signing::{Flags, SecCode, SecRequirement};
        use std::str::FromStr;

        let caller = SecCode::for_self(Flags::NONE).unwrap();
        let requirement = SecRequirement::from_str("true").unwrap();

        validate_key_broker_caller(&caller, &requirement).unwrap();
    }
}
