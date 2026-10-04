use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, block_on, futures_lite::future};
use serde_json::Value;
use vmux_ecs::event::{LspPackage, LspPkgStatus};
use vmux_path::Executable;

use crate::lsp::archive::ArchiveKind;
use crate::lsp::download::{self, RemoteArtifact};
use crate::lsp::package_path::{PackageName, PackagePath, Sha256Digest};
use crate::lsp::purl::Purl;
use crate::lsp::store;
use crate::lsp::target::Asset;

pub(crate) struct CatalogPlugin;

impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(Update, poll);
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

#[derive(Component)]
struct CatalogRoot;

#[derive(Component)]
struct CatalogTask {
    task: Task<Result<Vec<Package>, String>>,
}

impl CatalogTask {
    fn new() -> Self {
        Self {
            task: IoTaskPool::get()
                .spawn(async move { CatalogSource::load(&store::PackageStore::lsp(), false) }),
        }
    }
}

impl Package {
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

    pub(crate) fn snapshot(&self, store: &store::PackageStore) -> LspPackage {
        let source = Purl::parse(&self.source_id);
        let kind = source
            .as_ref()
            .map(|source| source.kind.as_str())
            .unwrap_or_default();
        let installed = store.is_installed(&self.name);
        let on_path = !installed
            && matches!(
                store.resolve_command(self.name.as_str()),
                store::Resolution::OnPath
            );
        let catalog_version = source.as_ref().and_then(|source| source.version.clone());
        let installed_version = if installed {
            store
                .read_receipt(&self.name)
                .and_then(|receipt| receipt.version)
        } else {
            None
        };
        let outdated = installed
            && installed_version.is_some()
            && catalog_version.is_some()
            && installed_version != catalog_version;
        let status = if outdated {
            LspPkgStatus::Outdated
        } else if installed {
            LspPkgStatus::Installed
        } else if on_path {
            LspPkgStatus::OnPath
        } else {
            LspPkgStatus::Available
        };
        let toolchain = source.as_ref().and_then(Purl::toolchain);
        let installable = kind == "github"
            || toolchain.is_some_and(|command| Executable::find(command).is_some());
        let requires = if installable {
            None
        } else {
            toolchain.map(String::from)
        };
        let version = if installed {
            installed_version
        } else {
            catalog_version
        };
        LspPackage {
            name: self.name.as_str().to_string(),
            description: self.description.clone(),
            languages: self.languages.clone(),
            categories: self.categories.clone(),
            status,
            version,
            installable,
            requires,
        }
    }
}

pub(crate) struct CatalogSource(Vec<u8>);

impl CatalogSource {
    fn read(path: &Path) -> Result<Self, String> {
        Self::read_with_limit(path, download::CATALOG_MAX_BYTES)
    }

    fn read_with_limit(path: &Path, max_bytes: u64) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
        if file.metadata().map_err(|error| error.to_string())?.len() > max_bytes {
            return Err(format!("catalog exceeds {max_bytes} bytes"));
        }
        let mut bytes = Vec::new();
        file.take(max_bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > max_bytes {
            return Err(format!("catalog exceeds {max_bytes} bytes"));
        }
        Ok(Self(bytes))
    }

    fn packages(&self) -> Result<Vec<Package>, String> {
        let source = std::str::from_utf8(&self.0).map_err(|error| error.to_string())?;
        Self::parse(source)
    }

    fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub(crate) fn parse(source: &str) -> Result<Vec<Package>, String> {
        let entries: Vec<Value> =
            serde_json::from_str(source).map_err(|error| error.to_string())?;
        let mut packages = Vec::with_capacity(entries.len());
        for entry in &entries {
            if let Ok(package) = Package::parse(entry) {
                packages.push(package);
            }
        }
        Ok(packages)
    }

    pub(crate) fn load(store: &store::PackageStore, refresh: bool) -> Result<Vec<Package>, String> {
        if !refresh && store.catalog_path().is_file() {
            return Self::read(&store.catalog_path())?.packages();
        }
        let artifact = RemoteArtifact::github_release(
            "mason-org",
            "mason-registry",
            None,
            "registry.json.zip",
            download::CATALOG_MAX_BYTES,
        )?;
        Self::fetch(&artifact, store)
    }

    fn fetch(
        artifact: &RemoteArtifact,
        store: &store::PackageStore,
    ) -> Result<Vec<Package>, String> {
        let registry_dir = store.registries_dir();
        std::fs::create_dir_all(&registry_dir).map_err(|error| error.to_string())?;
        let staging = tempfile::tempdir_in(&registry_dir).map_err(|error| error.to_string())?;
        let archive_path = staging.path().join("registry.json.zip");
        artifact.download_to(&archive_path, download::CATALOG_MAX_BYTES, |_, _| {})?;
        ArchiveKind::Zip.extract(&archive_path, staging.path(), "registry.json")?;
        let source = Self::read(&staging.path().join("registry.json"))?;
        let packages = source.packages()?;
        vmux_path::AtomicFile::write(store.catalog_path(), source.bytes())
            .map_err(|error| error.to_string())?;
        Ok(packages)
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("LSP catalog"), CatalogRoot, CatalogTask::new()));
}

fn poll(
    mut catalogs: Query<(Entity, &mut CatalogTask), With<CatalogRoot>>,
    packages: Query<(Entity, &ChildOf), With<Package>>,
    mut commands: Commands,
) {
    let Ok((entity, mut task)) = catalogs.single_mut() else {
        return;
    };
    let Some(result) = block_on(future::poll_once(&mut task.task)) else {
        return;
    };
    match result {
        Ok(loaded) => {
            for (package, parent) in &packages {
                if parent.parent() == entity {
                    commands.entity(package).despawn();
                }
            }
            for package in loaded {
                commands.spawn((
                    Name::new(package.name.as_str().to_string()),
                    package,
                    ChildOf(entity),
                ));
            }
        }
        Err(error) => {
            bevy::log::warn!("LSP catalog load failed: {error}");
        }
    }
    commands
        .entity(entity)
        .remove::<CatalogTask>()
        .insert(CatalogReady);
}

#[cfg(test)]
mod tests {
    use super::*;

    use sha2::Digest;
    use std::io::{Read, Write};
    use std::net::TcpListener;

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
        let packages = CatalogSource::parse(SAMPLE).unwrap();
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
        let packages = CatalogSource::parse(SAMPLE).unwrap();
        let ts = packages
            .iter()
            .find(|package| package.name.as_str() == "typescript-language-server")
            .unwrap();
        assert!(ts.assets.is_empty());
        assert!(ts.source_id.starts_with("pkg:npm/"));
    }

    #[test]
    fn ensure_catalog_reads_cache_without_network() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store::PackageStore::at(tmp.path());
        std::fs::create_dir_all(store.registries_dir()).unwrap();
        std::fs::write(store.catalog_path(), SAMPLE).unwrap();
        let packages = CatalogSource::load(&store, false).unwrap();
        assert_eq!(packages.len(), 3);
    }

    #[test]
    fn oversized_cached_catalog_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("registry.json");
        std::fs::write(&path, b"1234").unwrap();
        let Err(error) = CatalogSource::read_with_limit(&path, 3) else {
            panic!("oversized catalog was accepted");
        };
        assert_eq!(error, "catalog exceeds 3 bytes");
    }

    #[test]
    fn fetch_catalog_downloads_unzips_parses() {
        let mut zbuf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut zbuf));
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            w.start_file("registry.json", opts).unwrap();
            w.write_all(SAMPLE.as_bytes()).unwrap();
            w.finish().unwrap();
        }
        let digest = Sha256Digest::parse(&format!("{:x}", sha2::Sha256::digest(&zbuf))).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut req = [0u8; 1024];
                let _ = s.read(&mut req);
                let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", zbuf.len());
                let _ = s.write_all(header.as_bytes());
                let _ = s.write_all(&zbuf);
            }
        });
        let tmp = tempfile::tempdir().unwrap();
        let artifact = download::RemoteArtifact {
            url: format!("http://{addr}/registry.json.zip"),
            sha256: digest,
        };
        let store = store::PackageStore::at(tmp.path());
        let packages = CatalogSource::fetch(&artifact, &store).unwrap();
        assert_eq!(packages.len(), 3);
        assert!(store.catalog_path().is_file());
    }
}
