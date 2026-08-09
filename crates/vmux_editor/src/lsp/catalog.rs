use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::lsp::archive::{self, ArchiveKind};
use crate::lsp::target::Asset;
use crate::lsp::{download, store};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub description: String,
    pub languages: Vec<String>,
    pub categories: Vec<String>,
    pub source_id: String,
    pub assets: Vec<Asset>,
    pub bin: BTreeMap<String, String>,
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

fn parse_asset(v: &Value) -> Vec<Asset> {
    let targets = str_array(v.get("target"));
    let Some(file) = v.get("file").and_then(Value::as_str) else {
        return Vec::new();
    };
    let bin = v.get("bin").and_then(Value::as_str).map(String::from);
    targets
        .into_iter()
        .map(|target| Asset {
            target,
            file: file.to_string(),
            bin: bin.clone(),
        })
        .collect()
}

fn parse_assets(v: Option<&Value>) -> Vec<Asset> {
    match v {
        Some(Value::Array(assets)) => assets.iter().flat_map(parse_asset).collect(),
        Some(asset @ Value::Object(_)) => parse_asset(asset),
        _ => Vec::new(),
    }
}

fn parse_bin(v: Option<&Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    match v {
        Some(Value::Object(m)) => {
            for (k, val) in m {
                if let Some(s) = val.as_str() {
                    out.insert(k.clone(), s.to_string());
                }
            }
        }
        Some(Value::String(s)) => {
            let (k, f) = s.split_once(':').unwrap_or((s.as_str(), s.as_str()));
            out.insert(k.to_string(), f.to_string());
        }
        _ => {}
    }
    out
}

fn parse_one(v: &Value) -> Option<Package> {
    let name = v.get("name")?.as_str()?.to_string();
    let source = v.get("source")?;
    let source_id = source.get("id")?.as_str()?.to_string();
    Some(Package {
        name,
        description: v
            .get("description")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        languages: str_array(v.get("languages")),
        categories: str_array(v.get("categories")),
        source_id,
        assets: parse_assets(source.get("asset")),
        bin: parse_bin(v.get("bin")),
    })
}

pub fn parse_registry(json: &str) -> Result<Vec<Package>, String> {
    let arr: Vec<Value> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(arr.iter().filter_map(parse_one).collect())
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
                || p.name.to_ascii_lowercase().contains(&q)
                || p.description.to_ascii_lowercase().contains(&q))
                && (lang.is_empty() || p.languages.iter().any(|l| l.to_ascii_lowercase() == lang))
                && (cat.is_empty() || p.categories.iter().any(|c| c.to_ascii_lowercase() == cat))
        })
        .collect()
}

pub fn registry_url() -> String {
    "https://github.com/mason-org/mason-registry/releases/latest/download/registry.json.zip".into()
}

pub fn cached_path(store_root: &Path) -> PathBuf {
    store::registries_dir(store_root).join("registry.json")
}

pub fn fetch_catalog(url: &str, store_root: &Path) -> Result<Vec<Package>, String> {
    let regdir = store::registries_dir(store_root);
    std::fs::create_dir_all(&regdir).map_err(|e| e.to_string())?;
    let zip = regdir.join("registry.json.zip");
    download::download_to(url, &zip, |_, _| {})?;
    archive::extract(&zip, ArchiveKind::Zip, &regdir, "registry.json")?;
    let json = std::fs::read_to_string(cached_path(store_root)).map_err(|e| e.to_string())?;
    parse_registry(&json)
}

pub fn ensure_catalog(store_root: &Path, refresh: bool) -> Result<Vec<Package>, String> {
    if !refresh && cached_path(store_root).is_file() {
        let json = std::fs::read_to_string(cached_path(store_root)).map_err(|e| e.to_string())?;
        return parse_registry(&json);
    }
    fetch_catalog(&registry_url(), store_root)
}

#[cfg(test)]
#[path = "catalog.test.rs"]
mod tests;
