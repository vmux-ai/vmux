use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use serde_json::Value;

use crate::lsp::archive::ArchiveKind;
use crate::lsp::download::{self, RemoteArtifact};
use crate::lsp::package_path::{PackageName, PackagePath, Sha256Digest};
use crate::lsp::store;
use crate::lsp::target::Asset;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: PackageName,
    pub description: String,
    pub languages: Vec<String>,
    pub categories: Vec<String>,
    pub source_id: String,
    pub assets: Vec<Asset>,
    pub bin: BTreeMap<PackageName, String>,
}

#[derive(Default)]
pub struct Catalog {
    packages: Vec<Package>,
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
}

struct CatalogSource(Vec<u8>);

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

    fn catalog(&self) -> Result<Catalog, String> {
        let source = std::str::from_utf8(&self.0).map_err(|error| error.to_string())?;
        Catalog::parse(source)
    }

    fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Catalog {
    pub fn parse(source: &str) -> Result<Self, String> {
        let entries: Vec<Value> =
            serde_json::from_str(source).map_err(|error| error.to_string())?;
        let mut packages = Vec::with_capacity(entries.len());
        for entry in &entries {
            packages.push(Package::parse(entry)?);
        }
        Ok(Self { packages })
    }

    pub fn load(store: &store::PackageStore, refresh: bool) -> Result<Self, String> {
        if !refresh && store.catalog_path().is_file() {
            return CatalogSource::read(&store.catalog_path())?.catalog();
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

    fn fetch(artifact: &RemoteArtifact, store: &store::PackageStore) -> Result<Self, String> {
        let registry_dir = store.registries_dir();
        std::fs::create_dir_all(&registry_dir).map_err(|error| error.to_string())?;
        let staging = tempfile::tempdir_in(&registry_dir).map_err(|error| error.to_string())?;
        let archive_path = staging.path().join("registry.json.zip");
        artifact.download_to(&archive_path, download::CATALOG_MAX_BYTES, |_, _| {})?;
        ArchiveKind::Zip.extract(&archive_path, staging.path(), "registry.json")?;
        let source = CatalogSource::read(&staging.path().join("registry.json"))?;
        let catalog = source.catalog()?;
        vmux_path::AtomicFile::write(store.catalog_path(), source.bytes())
            .map_err(|error| error.to_string())?;
        Ok(catalog)
    }

    pub fn packages(&self) -> &[Package] {
        &self.packages
    }

    pub fn find(&self, name: &str) -> Option<&Package> {
        self.packages
            .iter()
            .find(|package| package.name.as_str() == name)
    }

    pub fn search(&self, query: &str, language: &str, category: &str) -> Vec<&Package> {
        let query = query.to_ascii_lowercase();
        let language = language.to_ascii_lowercase();
        let category = category.to_ascii_lowercase();
        self.packages
            .iter()
            .filter(|package| {
                (query.is_empty()
                    || package.name.as_str().to_ascii_lowercase().contains(&query)
                    || package.description.to_ascii_lowercase().contains(&query))
                    && (language.is_empty()
                        || package
                            .languages
                            .iter()
                            .any(|item| item.to_ascii_lowercase() == language))
                    && (category.is_empty()
                        || package
                            .categories
                            .iter()
                            .any(|item| item.to_ascii_lowercase() == category))
            })
            .collect()
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
        let catalog = Catalog::parse(SAMPLE).unwrap();
        assert_eq!(catalog.packages().len(), 3);
        let ra = catalog.find("rust-analyzer").unwrap();
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
        let catalog = Catalog::parse(SAMPLE).unwrap();
        let ts = catalog.find("typescript-language-server").unwrap();
        assert!(ts.assets.is_empty());
        assert!(ts.source_id.starts_with("pkg:npm/"));
    }

    #[test]
    fn search_filters() {
        let catalog = Catalog::parse(SAMPLE).unwrap();
        assert_eq!(catalog.search("rust", "", "").len(), 1);
        assert_eq!(catalog.search("", "python", "").len(), 1);
        assert_eq!(catalog.search("", "", "lsp").len(), 2);
        assert_eq!(catalog.search("", "", "formatter").len(), 1);
        assert_eq!(catalog.search("lsp", "", "").len(), 2);
        assert_eq!(catalog.search("linter", "", "").len(), 1);
        assert_eq!(catalog.search("zzz", "", "").len(), 0);
    }

    #[test]
    fn ensure_catalog_reads_cache_without_network() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store::PackageStore::at(tmp.path());
        std::fs::create_dir_all(store.registries_dir()).unwrap();
        std::fs::write(store.catalog_path(), SAMPLE).unwrap();
        let catalog = Catalog::load(&store, false).unwrap();
        assert_eq!(catalog.packages().len(), 3);
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
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let mut zbuf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut zbuf));
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            w.start_file("registry.json", opts).unwrap();
            w.write_all(SAMPLE.as_bytes()).unwrap();
            w.finish().unwrap();
        }
        use sha2::Digest;
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
        let catalog = Catalog::fetch(&artifact, &store).unwrap();
        assert_eq!(catalog.packages().len(), 3);
        assert!(store.catalog_path().is_file());
    }
}
