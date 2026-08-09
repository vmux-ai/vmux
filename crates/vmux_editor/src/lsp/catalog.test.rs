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
    assert_eq!(ra.description, "Rust LSP");
    assert!(ra.categories.contains(&"LSP".to_string()));
    assert_eq!(ra.assets.len(), 3);
    assert_eq!(ra.assets[0].target, "darwin_arm64");
    assert_eq!(ra.assets[1].target, "linux_x64_gnu");
    assert_eq!(ra.assets[2].target, "linux_x64");
    assert_eq!(ra.bin.get("rust-analyzer").unwrap(), "{{source.asset.bin}}");
}

#[test]
fn npm_and_pypi_have_no_github_assets() {
    let pkgs = parse_registry(SAMPLE).unwrap();
    let ts = pkgs
        .iter()
        .find(|p| p.name == "typescript-language-server")
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
    let url = format!("http://{addr}/registry.json.zip");
    let pkgs = fetch_catalog(&url, tmp.path()).unwrap();
    assert_eq!(pkgs.len(), 3);
    assert!(cached_path(tmp.path()).is_file());
}
