use std::collections::BTreeMap;

use bevy::prelude::*;
use serde_json::Value;

use crate::lsp::package_path::{PackageName, PackagePath, Sha256Digest};
use crate::lsp::target::Asset;

pub(crate) struct CatalogPlugin;

impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(super::catalog_runtime::CatalogRuntimePlugin);
    }
}

#[derive(Debug, Clone, Component, PartialEq, Eq)]
pub struct Package {
    pub name: PackageName,
    pub description: String,
    pub languages: Vec<String>,
    pub categories: Vec<String>,
    pub source_id: String,
    pub assets: Vec<Asset>,
    pub bin: BTreeMap<PackageName, String>,
}

#[derive(Component)]
pub(crate) struct CatalogReady;

impl Package {
    pub(crate) fn parse_catalog(source: &str) -> Result<Vec<Self>, String> {
        let entries: Vec<Value> =
            serde_json::from_str(source).map_err(|error| error.to_string())?;
        let mut packages = Vec::with_capacity(entries.len());
        for entry in &entries {
            if let Ok(package) = Self::parse(entry) {
                packages.push(package);
            }
        }
        Ok(packages)
    }

    fn parse(value: &Value) -> Result<Self, String> {
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "package name is missing".to_string())?;
        let source = value
            .get("source")
            .ok_or_else(|| format!("{name}: source is missing"))?;
        let source_id = source
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{name}: source id is missing"))?
            .to_string();
        Ok(Self {
            name: PackageName::parse(name)?,
            description: value
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string(),
            languages: Self::strings(value.get("languages")),
            categories: Self::strings(value.get("categories")),
            source_id,
            assets: Self::assets(source.get("asset"))?,
            bin: Self::bin(value.get("bin"))?,
        })
    }

    fn strings(value: Option<&Value>) -> Vec<String> {
        match value {
            Some(Value::Array(values)) => {
                let mut strings = Vec::new();
                for value in values {
                    if let Some(value) = value.as_str() {
                        strings.push(value.to_string());
                    }
                }
                strings
            }
            Some(Value::String(value)) => vec![value.clone()],
            _ => Vec::new(),
        }
    }

    fn digest(value: &Value) -> Result<Option<Sha256Digest>, String> {
        for key in ["sha256", "digest", "checksum", "integrity"] {
            let Some(value) = value.get(key).and_then(Value::as_str) else {
                continue;
            };
            return Sha256Digest::parse(value).map(Some);
        }
        Ok(None)
    }

    fn asset(value: &Value) -> Result<Vec<Asset>, String> {
        let targets = Self::strings(value.get("target"));
        let Some(file) = value.get("file").and_then(Value::as_str) else {
            return Ok(Vec::new());
        };
        let file = PackagePath::parse(file)?;
        let bin = value
            .get("bin")
            .and_then(Value::as_str)
            .map(PackagePath::parse)
            .transpose()?;
        let sha256 = Self::digest(value)?;
        let mut assets = Vec::new();
        for target in targets {
            assets.push(Asset {
                target,
                file: file.clone(),
                bin: bin.clone(),
                sha256: sha256.clone(),
            });
        }
        Ok(assets)
    }

    fn assets(value: Option<&Value>) -> Result<Vec<Asset>, String> {
        match value {
            Some(Value::Array(values)) => {
                let mut assets = Vec::new();
                for value in values {
                    assets.extend(Self::asset(value)?);
                }
                Ok(assets)
            }
            Some(value @ Value::Object(_)) => Self::asset(value),
            _ => Ok(Vec::new()),
        }
    }

    fn bin(value: Option<&Value>) -> Result<BTreeMap<PackageName, String>, String> {
        let mut bin = BTreeMap::new();
        match value {
            Some(Value::Object(values)) => {
                for (name, value) in values {
                    if let Some(path) = value.as_str() {
                        bin.insert(PackageName::parse(name)?, path.to_string());
                    }
                }
            }
            Some(Value::String(value)) => {
                let (name, path) = value
                    .split_once(':')
                    .unwrap_or((value.as_str(), value.as_str()));
                bin.insert(PackageName::parse(name)?, path.to_string());
            }
            _ => {}
        }
        Ok(bin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
      {
        "name": "rust-analyzer",
        "description": "  Rust LSP  ",
        "languages": ["Rust"],
        "categories": ["LSP"],
        "source": {
          "id": "pkg:github/rust-lang/rust-analyzer@2026-05-25",
          "asset": [
            {"target": "darwin_arm64", "file": "rust-analyzer-aarch64-apple-darwin.gz", "bin": "rust-analyzer-aarch64-apple-darwin"},
            {"target": ["linux_x64_gnu","linux_x64"], "file": "rust-analyzer-x86_64-unknown-linux-gnu.gz", "bin": "rust-analyzer-x86_64-unknown-linux-gnu"}
          ]
        },
        "bin": {"rust-analyzer": "{{source.asset.bin}}"}
      },
      {
        "name": "typescript-language-server",
        "description": "TS LSP",
        "languages": ["TypeScript","JavaScript"],
        "categories": ["LSP"],
        "source": {"id": "pkg:npm/typescript-language-server@4.0.0"},
        "bin": {"typescript-language-server": "node_modules/.bin/typescript-language-server"}
      },
      {
        "name": "ruff",
        "description": "Python linter",
        "languages": ["Python"],
        "categories": ["Linter","Formatter"],
        "source": {"id": "pkg:pypi/ruff@0.5.0"}
      }
    ]"#;

    #[test]
    fn parses_three_packages() {
        let packages = Package::parse_catalog(SAMPLE).unwrap();
        assert_eq!(packages.len(), 3);
        let ra = packages
            .iter()
            .find(|package| package.name.as_str() == "rust-analyzer")
            .unwrap();
        assert_eq!(ra.description, "Rust LSP");
        assert!(ra.categories.contains(&"LSP".to_string()));
        assert_eq!(ra.assets.len(), 3);
        assert_eq!(ra.assets[0].target, "darwin_arm64");
        assert_eq!(ra.assets[1].target, "linux_x64_gnu");
        assert_eq!(ra.assets[2].target, "linux_x64");
        assert_eq!(
            ra.bin
                .iter()
                .find(|(name, _)| name.as_str() == "rust-analyzer")
                .map(|(_, path)| path.as_str()),
            Some("{{source.asset.bin}}")
        );
    }

    #[test]
    fn npm_and_pypi_have_no_github_assets() {
        let packages = Package::parse_catalog(SAMPLE).unwrap();
        let ts = packages
            .iter()
            .find(|package| package.name.as_str() == "typescript-language-server")
            .unwrap();
        assert!(ts.assets.is_empty());
        assert!(ts.source_id.starts_with("pkg:npm/"));
    }
}
