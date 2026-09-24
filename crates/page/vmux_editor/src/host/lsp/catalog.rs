use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::lsp::archive::{self, ArchiveKind};
use crate::lsp::package_path::{PackageName, PackagePath, Sha256Digest};
use crate::lsp::target::Asset;
use crate::lsp::{download, store};

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

fn str_array(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn parse_digest(v: &Value) -> Result<Option<Sha256Digest>, String> {
    for key in ["sha256", "digest", "checksum", "integrity"] {
        let Some(value) = v.get(key).and_then(Value::as_str) else {
            continue;
        };
        return Sha256Digest::parse(value).map(Some);
    }
    Ok(None)
}

fn parse_asset(v: &Value) -> Result<Vec<Asset>, String> {
    let targets = str_array(v.get("target"));
    let Some(file) = v.get("file").and_then(Value::as_str) else {
        return Ok(Vec::new());
    };
    let file = PackagePath::parse(file)?;
    let bin = v
        .get("bin")
        .and_then(Value::as_str)
        .map(PackagePath::parse)
        .transpose()?;
    let sha256 = parse_digest(v)?;
    Ok(targets
        .into_iter()
        .map(|target| Asset {
            target,
            file: file.clone(),
            bin: bin.clone(),
            sha256: sha256.clone(),
        })
        .collect())
}

fn parse_assets(v: Option<&Value>) -> Result<Vec<Asset>, String> {
    match v {
        Some(Value::Array(assets)) => {
            let mut parsed = Vec::new();
            for asset in assets {
                parsed.extend(parse_asset(asset)?);
            }
            Ok(parsed)
        }
        Some(asset @ Value::Object(_)) => parse_asset(asset),
        _ => Ok(Vec::new()),
    }
}

fn parse_bin(v: Option<&Value>) -> Result<BTreeMap<PackageName, String>, String> {
    let mut out = BTreeMap::new();
    match v {
        Some(Value::Object(m)) => {
            for (k, val) in m {
                if let Some(s) = val.as_str() {
                    out.insert(PackageName::parse(k)?, s.to_string());
                }
            }
        }
        Some(Value::String(s)) => {
            let (k, f) = s.split_once(':').unwrap_or((s.as_str(), s.as_str()));
            out.insert(PackageName::parse(k)?, f.to_string());
        }
        _ => {}
    }
    Ok(out)
}

fn parse_one(v: &Value) -> Result<Package, String> {
    let name = v
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "package name is missing".to_string())?;
    let source = v
        .get("source")
        .ok_or_else(|| format!("{name}: source is missing"))?;
    let source_id = source
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{name}: source id is missing"))?
        .to_string();
    Ok(Package {
        name: PackageName::parse(name)?,
        description: v
            .get("description")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        languages: str_array(v.get("languages")),
        categories: str_array(v.get("categories")),
        source_id,
        assets: parse_assets(source.get("asset"))?,
        bin: parse_bin(v.get("bin"))?,
    })
}

pub fn parse_registry(json: &str) -> Result<Vec<Package>, String> {
    let arr: Vec<Value> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    arr.iter().map(parse_one).collect()
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

    fn packages(&self) -> Result<Vec<Package>, String> {
        let source = std::str::from_utf8(&self.0).map_err(|error| error.to_string())?;
        parse_registry(source)
    }

    fn bytes(&self) -> &[u8] {
        &self.0
    }
}

pub fn search<'a>(
    pkgs: &'a [Package],
    query: &str,
    language: &str,
    category: &str,
) -> Vec<&'a Package> {
    let q = query.to_ascii_lowercase();
    let lang = language.to_ascii_lowercase();
    let cat = category.to_ascii_lowercase();
    pkgs.iter()
        .filter(|p| {
            (q.is_empty()
                || p.name.as_str().to_ascii_lowercase().contains(&q)
                || p.description.to_ascii_lowercase().contains(&q))
                && (lang.is_empty() || p.languages.iter().any(|l| l.to_ascii_lowercase() == lang))
                && (cat.is_empty() || p.categories.iter().any(|c| c.to_ascii_lowercase() == cat))
        })
        .collect()
}

pub fn cached_path(store_root: &Path) -> PathBuf {
    store::registries_dir(store_root).join("registry.json")
}

pub fn fetch_catalog(
    artifact: &download::RemoteArtifact,
    store_root: &Path,
) -> Result<Vec<Package>, String> {
    let regdir = store::registries_dir(store_root);
    std::fs::create_dir_all(&regdir).map_err(|e| e.to_string())?;
    let staging = tempfile::tempdir_in(&regdir).map_err(|e| e.to_string())?;
    let zip = staging.path().join("registry.json.zip");
    download::download_to(
        &artifact.url,
        &zip,
        download::CATALOG_MAX_BYTES,
        &artifact.sha256,
        |_, _| {},
    )?;
    archive::extract(&zip, ArchiveKind::Zip, staging.path(), "registry.json")?;
    let source = CatalogSource::read(&staging.path().join("registry.json"))?;
    let parsed = source.packages()?;
    vmux_path::AtomicFile::write(cached_path(store_root), source.bytes())
        .map_err(|e| e.to_string())?;
    Ok(parsed)
}

pub fn ensure_catalog(store_root: &Path, refresh: bool) -> Result<Vec<Package>, String> {
    if !refresh && cached_path(store_root).is_file() {
        return CatalogSource::read(&cached_path(store_root))?.packages();
    }
    let artifact = download::github_release_asset(
        "mason-org",
        "mason-registry",
        None,
        "registry.json.zip",
        download::CATALOG_MAX_BYTES,
    )?;
    fetch_catalog(&artifact, store_root)
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
        let pkgs = parse_registry(SAMPLE).unwrap();
        assert_eq!(pkgs.len(), 3);
        let ra = pkgs
            .iter()
            .find(|p| p.name.as_str() == "rust-analyzer")
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
        let pkgs = parse_registry(SAMPLE).unwrap();
        let ts = pkgs
            .iter()
            .find(|p| p.name.as_str() == "typescript-language-server")
            .unwrap();
        assert!(ts.assets.is_empty());
        assert!(ts.source_id.starts_with("pkg:npm/"));
    }

    #[test]
    fn search_filters() {
        let pkgs = parse_registry(SAMPLE).unwrap();
        assert_eq!(search(&pkgs, "rust", "", "").len(), 1);
        assert_eq!(search(&pkgs, "", "python", "").len(), 1);
        assert_eq!(search(&pkgs, "", "", "lsp").len(), 2);
        assert_eq!(search(&pkgs, "", "", "formatter").len(), 1);
        assert_eq!(search(&pkgs, "lsp", "", "").len(), 2);
        assert_eq!(search(&pkgs, "linter", "", "").len(), 1);
        assert_eq!(search(&pkgs, "zzz", "", "").len(), 0);
    }

    #[test]
    fn ensure_catalog_reads_cache_without_network() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(store::registries_dir(root)).unwrap();
        std::fs::write(cached_path(root), SAMPLE).unwrap();
        let pkgs = ensure_catalog(root, false).unwrap();
        assert_eq!(pkgs.len(), 3);
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
        let pkgs = fetch_catalog(&artifact, tmp.path()).unwrap();
        assert_eq!(pkgs.len(), 3);
        assert!(cached_path(tmp.path()).is_file());
    }
}
