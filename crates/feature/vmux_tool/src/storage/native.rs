use bevy::prelude::*;
use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::ToolStore;

#[derive(Component, Clone)]
pub(super) struct Paths {
    pub(super) root: PathBuf,
}

impl Paths {
    pub(super) fn from_store(store: &ToolStore) -> Self {
        Self { root: store.root() }
    }

    pub(super) fn read_manifest(&self) -> Result<ManifestBytes, String> {
        match std::fs::read_to_string(self.root.join("tools.toml")) {
            Ok(source) => Ok(ManifestBytes(Some(source))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ManifestBytes(None)),
            Err(error) => Err(error.to_string()),
        }
    }

    pub(super) fn read_brewfile(&self) -> Result<Brewfile, String> {
        let path = self.root.join("Brewfile");
        Ok(Brewfile {
            exists: path.exists(),
            is_file: path.is_file(),
            source: std::fs::read_to_string(path).map_err(|error| error.to_string()),
        })
    }

    pub(super) fn write_manifest(
        &self,
        source: String,
        receipt: super::write::ManifestReceipt,
    ) -> Result<ManifestWritten, String> {
        let publication =
            vmux_path::AtomicFile::write_observed(self.root.join("tools.toml"), source.as_bytes());
        if publication.published {
            receipt.record();
        }
        publication
            .result
            .map(|_| ManifestWritten)
            .map_err(|error| error.to_string())
    }

    pub(super) fn write_brewfile(&self, source: String) -> Result<BrewfileWritten, String> {
        vmux_path::AtomicFile::write(self.root.join("Brewfile"), source.as_bytes())
            .map(|_| BrewfileWritten)
            .map_err(|error| error.to_string())
    }
}

#[derive(Component, Clone)]
pub(super) struct MovePaths {
    pub(super) source: PathBuf,
    pub(super) destination: PathBuf,
}

impl MovePaths {
    pub(super) fn probe(&self) -> Result<Probe, String> {
        Ok(Probe {
            source: self.source.symlink_metadata().is_ok(),
            destination: self.destination.symlink_metadata().is_ok(),
        })
    }

    pub(super) fn rename(&self) -> Result<Moved, String> {
        std::fs::rename(&self.source, &self.destination)
            .map(|_| Moved)
            .map_err(|error| error.to_string())
    }
}

#[derive(Component)]
pub(super) struct Probe {
    pub(super) source: bool,
    pub(super) destination: bool,
}

#[derive(Component)]
pub(super) struct Moved;

#[derive(Component)]
pub(super) struct ManifestBytes(pub(super) Option<String>);

#[derive(Component)]
pub(super) struct ManifestWritten;

#[derive(Component)]
pub(super) struct BrewfileWritten;

#[derive(Component)]
pub(super) struct Brewfile {
    pub(super) exists: bool,
    pub(super) is_file: bool,
    pub(super) source: Result<String, String>,
}

impl Brewfile {
    pub(super) fn merge(&self, formulae: &[String], casks: &[String]) -> String {
        let desired_formulae = formulae.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let desired_casks = casks.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let mut seen_formulae = BTreeSet::new();
        let mut seen_casks = BTreeSet::new();
        let mut lines = Vec::new();
        for line in self.source.as_deref().unwrap_or_default().lines() {
            if let Some(name) = Self::package(line, "brew") {
                if desired_formulae.contains(name.as_str()) {
                    seen_formulae.insert(name);
                    lines.push(line.to_string());
                }
            } else if let Some(name) = Self::package(line, "cask") {
                if desired_casks.contains(name.as_str()) {
                    seen_casks.insert(name);
                    lines.push(line.to_string());
                }
            } else {
                lines.push(line.to_string());
            }
        }
        for package in formulae {
            if !seen_formulae.contains(package) {
                lines.push(format!("brew {package:?}"));
            }
        }
        for package in casks {
            if !seen_casks.contains(package) {
                lines.push(format!("cask {package:?}"));
            }
        }
        if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }

    fn package(line: &str, call: &str) -> Option<String> {
        let rest = line.trim_start().strip_prefix(call)?;
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        let rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if !matches!(quote, '\'' | '"') {
            return None;
        }
        let mut escaped = false;
        let mut name = String::new();
        for character in rest[quote.len_utf8()..].chars() {
            if escaped {
                name.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                return (!name.is_empty()).then_some(name);
            } else {
                name.push(character);
            }
        }
        None
    }
}
