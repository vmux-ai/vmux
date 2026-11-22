use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
#[cfg(feature = "development")]
use std::fs;
use std::path::Path;
use std::process::Command;

#[cfg(feature = "development")]
use crate::Result;
#[cfg(feature = "development")]
use crate::product::ProductMetadata;

#[derive(Clone)]
pub struct Environment {
    values: BTreeMap<OsString, OsString>,
}

impl Environment {
    #[cfg(feature = "development")]
    pub fn product(&self, root: &Path) -> Result<ProductMetadata> {
        let mut input = BTreeMap::new();
        for (key, value) in &self.values {
            let Some(name) = key.to_str() else {
                continue;
            };
            if !name.starts_with("VMUX_HOST_") {
                continue;
            }
            let value = value
                .to_str()
                .ok_or_else(|| format!("{name} must be UTF-8"))?;
            input.insert(name.into(), value.into());
        }
        Ok(ProductMetadata::parse(root, &input)?)
    }

    pub fn current() -> Self {
        let mut environment = Self {
            values: std::env::vars_os().collect(),
        };
        for name in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_COMMON_DIR",
            "GIT_PREFIX",
        ] {
            environment.remove(name);
        }
        environment
    }

    #[cfg(feature = "development")]
    pub fn load(&mut self, root: &Path) -> Result<()> {
        let text = match fs::read_to_string(root.join(".env")) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let mut bytes = key.bytes();
            let valid = bytes
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
            if valid {
                if matches!(
                    key,
                    "VMUX_PROFILE"
                        | "VMUX_TEST"
                        | "VMUX_DATA_DIR"
                        | "VMUX_REMOTE_RELAY_URL"
                        | "VMUX_ACCOUNT_HOST"
                        | "VMUX_ACCOUNT_PORT"
                        | "VMUX_LOCAL_QUIC_FINGERPRINT"
                        | "VMUX_RELEASE_FLAGS_ENDPOINT"
                        | "VMUX_RELEASE_FLAGS_ENVIRONMENT"
                        | "VMUX_FEATURE_ADMIN_ENDPOINT"
                        | "VMUX_FEATURE_ADMIN_ENVIRONMENT"
                        | "VMUX_FEATURE_ADMIN_TOKEN"
                ) && self.get(key).is_some()
                {
                    continue;
                }
                self.set(key, value);
            }
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&OsStr> {
        self.values.get(OsStr::new(name)).map(OsString::as_os_str)
    }

    pub fn names(&self) -> impl Iterator<Item = &OsString> {
        self.values.keys()
    }

    pub fn present(&self, name: &str) -> bool {
        self.get(name).is_some_and(|value| !value.is_empty())
    }

    #[cfg(any(feature = "development", test))]
    pub fn incremental(&self, command: Option<&OsStr>) -> &OsStr {
        if let Some(value) = self.get("CARGO_INCREMENTAL")
            && !value.is_empty()
        {
            return value;
        }
        if self.get("CI") == Some(OsStr::new("true"))
            || matches!(
                command.and_then(OsStr::to_str),
                Some("check" | "clippy" | "test" | "bench")
            )
        {
            return OsStr::new("0");
        }
        if self.get("VMUX_BUILD_PROFILE") == Some(OsStr::new("dev")) {
            return OsStr::new("1");
        }
        OsStr::new("0")
    }

    #[cfg(feature = "development")]
    pub fn required(&self, name: &str) -> Result<&OsStr> {
        match self.get(name) {
            Some(value) if !value.is_empty() => Ok(value),
            _ => Err(format!("Missing {name}").into()),
        }
    }

    pub fn set(&mut self, name: &str, value: impl Into<OsString>) {
        self.values.insert(name.into(), value.into());
    }

    pub fn apply(&self, command: &mut Command) {
        command.env_clear().envs(&self.values);
    }

    pub fn remove(&mut self, name: &str) {
        self.values.remove(OsStr::new(name));
    }

    pub fn command(&self, program: impl AsRef<OsStr>, root: &Path) -> Command {
        let mut command = Command::new(program);
        self.apply(&mut command);
        command.current_dir(root);
        command
    }
}

#[cfg(test)]
mod tests {
    use super::Environment;
    use std::collections::BTreeMap;
    use std::ffi::OsStr;

    #[test]
    fn cargo_incremental_defaults_follow_command_purpose_and_explicit_override() {
        let mut environment = Environment {
            values: BTreeMap::new(),
        };
        environment.set("VMUX_BUILD_PROFILE", "dev");
        assert_eq!(
            environment.incremental(Some(OsStr::new("build"))),
            OsStr::new("1")
        );
        for command in ["test", "clippy", "check", "bench"] {
            assert_eq!(
                environment.incremental(Some(OsStr::new(command))),
                OsStr::new("0")
            );
        }
        environment.set("CI", "true");
        assert_eq!(
            environment.incremental(Some(OsStr::new("build"))),
            OsStr::new("0")
        );
        environment.set("CARGO_INCREMENTAL", "1");
        assert_eq!(
            environment.incremental(Some(OsStr::new("test"))),
            OsStr::new("1")
        );
        environment.set("CARGO_INCREMENTAL", "0");
        assert_eq!(
            environment.incremental(Some(OsStr::new("build"))),
            OsStr::new("0")
        );
    }
}
