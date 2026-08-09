use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;

fn serve_gz_once(payload: &'static [u8]) -> (String, String) {
    let mut gz = Vec::new();
    {
        let mut enc = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::default());
        enc.write_all(payload).unwrap();
        enc.finish().unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut req = [0u8; 1024];
            let _ = s.read(&mut req);
            let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", gz.len());
            let _ = s.write_all(header.as_bytes());
            let _ = s.write_all(&gz);
        }
    });
    (format!("http://{addr}/server.gz"), "server.gz".to_string())
}

#[test]
fn asset_url_builds_github_release_url() {
    let pkg = Package {
        name: "rust-analyzer".into(),
        description: String::new(),
        languages: vec![],
        categories: vec![],
        source_id: "pkg:github/rust-lang/rust-analyzer@2026-05-25".into(),
        assets: vec![],
        bin: Default::default(),
    };
    let asset = Asset {
        target: "darwin_arm64".into(),
        file: "ra.gz".into(),
        bin: Some("ra".into()),
    };
    assert_eq!(
        asset_url(&pkg, &asset).unwrap(),
        "https://github.com/rust-lang/rust-analyzer/releases/download/2026-05-25/ra.gz"
    );
}

#[test]
fn install_from_url_extracts_links_and_writes_receipt() {
    let (url, file) = serve_gz_once(b"#!/bin/sh\necho hi\n");
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let mut bin = BTreeMap::new();
    bin.insert("myserver".to_string(), "{{source.asset.bin}}".to_string());
    let pkg = Package {
        name: "myserver".into(),
        description: String::new(),
        languages: vec![],
        categories: vec![],
        source_id: "pkg:github/acme/myserver@1.2.3".into(),
        assets: vec![],
        bin,
    };
    let asset = Asset {
        target: "darwin_arm64".into(),
        file,
        bin: Some("myserver-bin".into()),
    };
    let mut phases = Vec::new();
    let receipt = install_from_url(&pkg, &asset, &url, root, |ph, _, _| phases.push(ph)).unwrap();

    assert_eq!(receipt.name, "myserver");
    assert_eq!(receipt.version.as_deref(), Some("1.2.3"));
    assert!(store::is_installed(root, "myserver"));
    let binp = store::bin_path(root, "myserver").unwrap();
    assert_eq!(std::fs::read(&binp).unwrap(), b"#!/bin/sh\necho hi\n");
    assert!(phases.contains(&InstallPhase::Done));
}

#[test]
fn asset_file_template_links_extracted_binary() {
    assert_eq!(
        resolve_bin_template("{{source.asset.file}}", "marksman"),
        "marksman"
    );
}

#[test]
fn toolchain_mapping() {
    assert_eq!(toolchain_for("npm"), Some("npm"));
    assert_eq!(toolchain_for("pypi"), Some("python3"));
    assert_eq!(toolchain_for("cargo"), Some("cargo"));
    assert_eq!(toolchain_for("golang"), Some("go"));
    assert_eq!(toolchain_for("github"), None);
}

#[test]
fn source_argv_builders() {
    let pkgdir = std::path::Path::new("/tmp/pkg");
    let npm = purl::parse("pkg:npm/typescript-language-server@4.0.0").unwrap();
    let (prog, args) = npm_argv(pkgdir, &npm);
    assert_eq!(prog, "npm");
    assert!(args.contains(&"typescript-language-server@4.0.0".to_string()));
    assert!(args.contains(&"--prefix".to_string()));

    let cargo = purl::parse("pkg:cargo/taplo-cli@0.9.0").unwrap();
    let (_, cargs) = cargo_argv(pkgdir, &cargo);
    assert!(cargs.contains(&"--version".to_string()));
    assert!(cargs.contains(&"0.9.0".to_string()));
    assert!(cargs.contains(&"taplo-cli".to_string()));

    let go = purl::parse("pkg:golang/golang.org/x/tools/gopls@v0.16.0").unwrap();
    assert_eq!(golang_module(&go), "golang.org/x/tools/gopls");
    let (_, gargs) = golang_argv(&go);
    assert!(gargs.contains(&"golang.org/x/tools/gopls@v0.16.0".to_string()));

    let pypi = purl::parse("pkg:pypi/ruff@0.5.0").unwrap();
    assert_eq!(pip_spec(&pypi), "ruff==0.5.0");
    let pypi_nv = purl::parse("pkg:pypi/ruff").unwrap();
    assert_eq!(pip_spec(&pypi_nv), "ruff");
}

#[test]
fn source_links_prefixes() {
    let mut bin = BTreeMap::new();
    bin.insert("ts".to_string(), "{{x}}".to_string());
    let pkg = Package {
        name: "ts".into(),
        description: String::new(),
        languages: vec![],
        categories: vec![],
        source_id: "pkg:npm/ts@1".into(),
        assets: vec![],
        bin,
    };
    assert_eq!(
        source_links("npm", &pkg).get("ts").unwrap(),
        "node_modules/.bin/ts"
    );
    assert_eq!(source_links("pypi", &pkg).get("ts").unwrap(), "venv/bin/ts");
    assert_eq!(source_links("cargo", &pkg).get("ts").unwrap(), "bin/ts");
    assert_eq!(source_links("golang", &pkg).get("ts").unwrap(), "bin/ts");
}
