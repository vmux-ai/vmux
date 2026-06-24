use std::collections::BTreeMap;

use serde_json::Value;

use crate::lsp::target::Asset;

/// A catalog package (normalized from mason-registry's heterogeneous JSON).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub description: String,
    pub languages: Vec<String>,
    pub categories: Vec<String>,
    /// PURL, e.g. `pkg:github/rust-lang/rust-analyzer@2026-05-25`.
    pub source_id: String,
    /// github assets (empty for npm/pypi/cargo/golang sources).
    pub assets: Vec<Asset>,
    /// link name -> file template (e.g. `{{source.asset.bin}}`).
    pub bin: BTreeMap<String, String>,
}

fn str_array(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn first_target(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(a)) => a.first().and_then(|x| x.as_str()).map(String::from),
        _ => None,
    }
}

fn parse_asset(v: &Value) -> Option<Asset> {
    Some(Asset {
        target: first_target(v.get("target"))?,
        file: v.get("file")?.as_str()?.to_string(),
        bin: v.get("bin").and_then(|x| x.as_str()).map(String::from),
    })
}

fn parse_assets(v: Option<&Value>) -> Vec<Asset> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(parse_asset).collect(),
        Some(obj @ Value::Object(_)) => parse_asset(obj).into_iter().collect(),
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
            // "name:file" or just "name"
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

/// Parse a mason-registry `registry.json` (top-level array). Entries that don't
/// match the supported shape are skipped (not fatal).
pub fn parse_registry(json: &str) -> Result<Vec<Package>, String> {
    let arr: Vec<Value> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(arr.iter().filter_map(parse_one).collect())
}

/// Case-insensitive search/filter over the catalog.
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
                && (lang.is_empty()
                    || p.languages.iter().any(|l| l.to_ascii_lowercase() == lang))
                && (cat.is_empty() || p.categories.iter().any(|c| c.to_ascii_lowercase() == cat))
        })
        .collect()
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
        let ra = pkgs.iter().find(|p| p.name == "rust-analyzer").unwrap();
        assert_eq!(ra.description, "Rust LSP"); // trimmed
        assert!(ra.categories.contains(&"LSP".to_string()));
        assert_eq!(ra.assets.len(), 2);
        assert_eq!(ra.assets[0].target, "darwin_arm64");
        assert_eq!(ra.assets[1].target, "linux_x64_gnu"); // first of array
        assert_eq!(ra.bin.get("rust-analyzer").unwrap(), "{{source.asset.bin}}");
    }

    #[test]
    fn npm_and_pypi_have_no_github_assets() {
        let pkgs = parse_registry(SAMPLE).unwrap();
        let ts = pkgs.iter().find(|p| p.name == "typescript-language-server").unwrap();
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
        assert_eq!(search(&pkgs, "lsp", "", "").len(), 2); // "LSP" appears in two descriptions
        assert_eq!(search(&pkgs, "linter", "", "").len(), 1); // ruff's description
        assert_eq!(search(&pkgs, "zzz", "", "").len(), 0); // genuinely absent
    }
}
