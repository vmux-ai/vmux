use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::Result;
use crate::product::ProductMetadata;

pub struct Environment {
    values: BTreeMap<OsString, OsString>,
}

impl Environment {
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
        Self {
            values: std::env::vars_os().collect(),
        }
    }

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
                self.set(key, value);
            }
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&OsStr> {
        self.values.get(OsStr::new(name)).map(OsString::as_os_str)
    }

    pub fn present(&self, name: &str) -> bool {
        self.get(name).is_some_and(|value| !value.is_empty())
    }

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
        command.envs(&self.values);
    }

    pub fn command(&self, program: impl AsRef<OsStr>, root: &Path) -> Command {
        let mut command = Command::new(program);
        self.apply(&mut command);
        command.current_dir(root);
        command
    }
}
