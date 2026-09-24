use std::collections::BTreeMap;
use std::path::Path;

use vmux_core::event::InstallPhase;

use crate::lsp::package_path::{PackageName, PackagePath, Sha256Digest};
use crate::lsp::target::Asset;
use crate::lsp::{archive, catalog::Package, download, purl, store, target};

fn resolve_bin_template(tmpl: &str, asset_bin: &PackagePath) -> Result<PackagePath, String> {
    PackagePath::parse(
        &tmpl
            .replace("{{source.asset.bin}}", asset_bin.as_str())
            .replace("{{source.asset.file}}", asset_bin.as_str()),
    )
}

pub fn asset_url(pkg: &Package, asset: &Asset) -> Result<String, String> {
    let p = purl::parse(&pkg.source_id).ok_or("bad purl")?;
    if p.kind != "github" {
        return Err(format!("not a github source: {}", pkg.source_id));
    }
    let ns = p.namespace.ok_or("github purl missing owner")?;
    let ver = p.version.ok_or("github purl missing version")?;
    Ok(format!(
        "https://github.com/{}/{}/releases/download/{}/{}",
        ns,
        p.name,
        ver,
        asset.file.as_str()
    ))
}

pub fn install_from_url(
    pkg: &Package,
    asset: &Asset,
    url: &str,
    digest: &Sha256Digest,
    store_root: &Path,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<store::Receipt, String> {
    let asset_bin = asset
        .bin
        .clone()
        .unwrap_or_else(|| PackagePath::from(&pkg.name));

    let staging_root = store::staging_dir(store_root);
    std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix(pkg.name.as_str())
        .tempdir_in(&staging_root)
        .map_err(|e| e.to_string())?;
    let dl = staging.path().join(asset.file.as_path());

    emit(InstallPhase::Downloading, Some(0), url);
    download::download_to(url, &dl, download::PACKAGE_MAX_BYTES, digest, |d, total| {
        let pct = total.and_then(|t| (t > 0).then(|| ((d * 100) / t) as u8));
        emit(InstallPhase::Downloading, pct, "downloading");
    })?;

    let pkgdir = staging.path().join("package");
    std::fs::create_dir_all(&pkgdir).map_err(|e| e.to_string())?;
    emit(InstallPhase::Extracting, None, "extracting");
    archive::extract(
        &dl,
        archive::kind_for(asset.file.as_str()),
        &pkgdir,
        asset_bin.as_str(),
    )?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let p = pkgdir.join(asset_bin.as_path());
        if let Ok(meta) = std::fs::metadata(&p) {
            let mut perm = meta.permissions();
            perm.set_mode(0o755);
            let _ = std::fs::set_permissions(&p, perm);
        }
    }

    emit(InstallPhase::Linking, None, "linking");
    let mut links: BTreeMap<PackageName, PackagePath> = BTreeMap::new();
    if pkg.bin.is_empty() {
        links.insert(pkg.name.clone(), asset_bin.clone());
    } else {
        for (link_name, tmpl) in &pkg.bin {
            links.insert(link_name.clone(), resolve_bin_template(tmpl, &asset_bin)?);
        }
    }
    for file in links.values() {
        if !pkgdir.join(file.as_path()).is_file() {
            return Err(format!("package binary is missing: {file}"));
        }
    }

    let receipt = store::Receipt {
        name: pkg.name.as_str().to_string(),
        version: purl::parse(&pkg.source_id).and_then(|p| p.version),
        source_id: pkg.source_id.clone(),
        bin: links
            .iter()
            .map(|(name, path)| (name.as_str().to_string(), path.as_str().to_string()))
            .collect(),
    };
    store::write_receipt_in(&pkgdir, &receipt).map_err(|e| e.to_string())?;
    store::activate_package(store_root, &pkg.name, &pkgdir).map_err(|e| e.to_string())?;
    for (link_name, file) in &links {
        store::link_bin(store_root, &pkg.name, file, link_name).map_err(|e| e.to_string())?;
    }
    emit(InstallPhase::Done, Some(100), "installed");
    Ok(receipt)
}

pub fn install_github(
    pkg: &Package,
    store_root: &Path,
    target_id: &str,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<store::Receipt, String> {
    emit(InstallPhase::Resolving, None, "selecting asset");
    let asset = target::pick_asset(&pkg.assets, target_id)
        .ok_or_else(|| format!("no prebuilt asset for {target_id}"))?
        .clone();
    let p = purl::parse(&pkg.source_id).ok_or("bad purl")?;
    let owner = p.namespace.as_deref().ok_or("github purl missing owner")?;
    let version = p.version.as_deref().ok_or("github purl missing version")?;
    let remote = download::github_release_asset(
        owner,
        &p.name,
        Some(version),
        asset.file.as_str(),
        download::PACKAGE_MAX_BYTES,
    )?;
    if let Some(expected) = asset.sha256.as_ref()
        && expected != &remote.sha256
    {
        return Err("catalog and GitHub asset digests disagree".to_string());
    }
    install_from_url(pkg, &asset, &remote.url, &remote.sha256, store_root, emit)
}

pub fn toolchain_for(kind: &str) -> Option<&'static str> {
    match kind {
        "npm" => Some("npm"),
        "pypi" => Some("python3"),
        "cargo" => Some("cargo"),
        "golang" => Some("go"),
        _ => None,
    }
}

fn version_or_latest(p: &purl::Purl) -> String {
    p.version.clone().unwrap_or_else(|| "latest".into())
}

fn npm_spec(p: &purl::Purl) -> String {
    match &p.namespace {
        Some(ns) => format!("{ns}/{}", p.name),
        None => p.name.clone(),
    }
}

pub fn npm_argv(pkgdir: &Path, p: &purl::Purl) -> (String, Vec<String>) {
    (
        "npm".into(),
        vec![
            "install".into(),
            "--prefix".into(),
            pkgdir.to_string_lossy().into_owned(),
            format!("{}@{}", npm_spec(p), version_or_latest(p)),
        ],
    )
}

pub fn cargo_argv(pkgdir: &Path, p: &purl::Purl) -> (String, Vec<String>) {
    let mut args = vec![
        "install".into(),
        "--root".into(),
        pkgdir.to_string_lossy().into_owned(),
    ];
    if let Some(v) = &p.version {
        args.push("--version".into());
        args.push(v.clone());
    }
    args.push(p.name.clone());
    ("cargo".into(), args)
}

pub fn golang_module(p: &purl::Purl) -> String {
    match &p.namespace {
        Some(ns) => format!("{ns}/{}", p.name),
        None => p.name.clone(),
    }
}

pub fn golang_argv(p: &purl::Purl) -> (String, Vec<String>) {
    (
        "go".into(),
        vec![
            "install".into(),
            format!("{}@{}", golang_module(p), version_or_latest(p)),
        ],
    )
}

pub fn pip_spec(p: &purl::Purl) -> String {
    match &p.version {
        Some(v) => format!("{}=={}", p.name, v),
        None => p.name.clone(),
    }
}

pub fn source_links(
    kind: &str,
    pkg: &Package,
) -> Result<BTreeMap<PackageName, PackagePath>, String> {
    let keys: Vec<PackageName> = if pkg.bin.is_empty() {
        vec![pkg.name.clone()]
    } else {
        pkg.bin.keys().cloned().collect()
    };
    let prefix = match kind {
        "npm" => "node_modules/.bin/",
        "pypi" => "venv/bin/",
        "cargo" | "golang" => "bin/",
        _ => "",
    };
    let mut links = BTreeMap::new();
    for key in keys {
        links.insert(
            key.clone(),
            PackagePath::parse(&format!("{prefix}{}", key.as_str()))?,
        );
    }
    Ok(links)
}

fn run(program: &str, args: &[String], envs: &[(&str, String)]) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let status = cmd.status().map_err(|e| format!("{program}: {e}"))?;
    if !status.success() {
        return Err(format!("{program} failed ({status})"));
    }
    Ok(())
}

fn finalize_links(
    pkg: &Package,
    store_root: &Path,
    staged_package: &Path,
    kind: &str,
    p: &purl::Purl,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<store::Receipt, String> {
    emit(InstallPhase::Linking, None, "linking");
    let links = source_links(kind, pkg)?;
    for file in links.values() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let bin = staged_package.join(file.as_path());
            if let Ok(meta) = std::fs::metadata(&bin) {
                let mut perm = meta.permissions();
                perm.set_mode(0o755);
                let _ = std::fs::set_permissions(&bin, perm);
            }
        }
    }
    let receipt = store::Receipt {
        name: pkg.name.as_str().to_string(),
        version: p.version.clone(),
        source_id: pkg.source_id.clone(),
        bin: links
            .iter()
            .map(|(name, path)| (name.as_str().to_string(), path.as_str().to_string()))
            .collect(),
    };
    store::write_receipt_in(staged_package, &receipt).map_err(|e| e.to_string())?;
    store::activate_package(store_root, &pkg.name, staged_package).map_err(|e| e.to_string())?;
    for (link_name, file) in &links {
        store::link_bin(store_root, &pkg.name, file, link_name).map_err(|e| e.to_string())?;
    }
    emit(InstallPhase::Done, Some(100), "installed");
    Ok(receipt)
}

fn install_toolchain(
    pkg: &Package,
    store_root: &Path,
    p: &purl::Purl,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<store::Receipt, String> {
    let tool = toolchain_for(&p.kind).ok_or("unknown source")?;
    if !crate::lsp::registry::executable_on_path(tool) {
        return Err(format!("requires {tool}"));
    }
    let staging_root = store::staging_dir(store_root);
    std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix(pkg.name.as_str())
        .tempdir_in(&staging_root)
        .map_err(|e| e.to_string())?;
    let pkgdir = staging.path().join("package");
    std::fs::create_dir_all(&pkgdir).map_err(|e| e.to_string())?;
    emit(InstallPhase::Downloading, None, tool);
    match p.kind.as_str() {
        "npm" => {
            let (prog, args) = npm_argv(&pkgdir, p);
            run(&prog, &args, &[])?;
        }
        "cargo" => {
            let (prog, args) = cargo_argv(&pkgdir, p);
            run(&prog, &args, &[])?;
        }
        "golang" => {
            let (prog, args) = golang_argv(p);
            run(
                &prog,
                &args,
                &[("GOBIN", pkgdir.join("bin").to_string_lossy().into_owned())],
            )?;
        }
        "pypi" => {
            let venv = pkgdir.join("venv");
            run(
                "python3",
                &[
                    "-m".into(),
                    "venv".into(),
                    venv.to_string_lossy().into_owned(),
                ],
                &[],
            )?;
            let pip = venv.join("bin").join("pip");
            run(
                &pip.to_string_lossy(),
                &["install".into(), pip_spec(p)],
                &[],
            )?;
        }
        other => return Err(format!("source '{other}' not supported")),
    }
    finalize_links(pkg, store_root, &pkgdir, &p.kind, p, &mut emit)
}

pub fn install(
    pkg: &Package,
    store_root: &Path,
    target_id: &str,
    emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<store::Receipt, String> {
    let p = purl::parse(&pkg.source_id).ok_or("bad purl")?;
    match p.kind.as_str() {
        "github" => install_github(pkg, store_root, target_id, emit),
        "npm" | "pypi" | "cargo" | "golang" => install_toolchain(pkg, store_root, &p, emit),
        other => Err(format!("install source '{other}' not yet supported")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn serve_gz_once(payload: &'static [u8]) -> (String, PackagePath, Sha256Digest) {
        let mut gz = Vec::new();
        {
            let mut enc = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::default());
            enc.write_all(payload).unwrap();
            enc.finish().unwrap();
        }
        use sha2::Digest;
        let digest = Sha256Digest::parse(&format!("{:x}", sha2::Sha256::digest(&gz))).unwrap();
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
        (
            format!("http://{addr}/server.gz"),
            PackagePath::parse("server.gz").unwrap(),
            digest,
        )
    }

    #[test]
    fn asset_url_builds_github_release_url() {
        let pkg = Package {
            name: PackageName::parse("rust-analyzer").unwrap(),
            description: String::new(),
            languages: vec![],
            categories: vec![],
            source_id: "pkg:github/rust-lang/rust-analyzer@2026-05-25".into(),
            assets: vec![],
            bin: Default::default(),
        };
        let asset = Asset {
            target: "darwin_arm64".into(),
            file: PackagePath::parse("ra.gz").unwrap(),
            bin: Some(PackagePath::parse("ra").unwrap()),
            sha256: None,
        };
        assert_eq!(
            asset_url(&pkg, &asset).unwrap(),
            "https://github.com/rust-lang/rust-analyzer/releases/download/2026-05-25/ra.gz"
        );
    }

    #[test]
    fn install_from_url_extracts_links_and_writes_receipt() {
        let (url, file, digest) = serve_gz_once(b"#!/bin/sh\necho hi\n");
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mut bin = BTreeMap::new();
        bin.insert(
            PackageName::parse("myserver").unwrap(),
            "{{source.asset.bin}}".to_string(),
        );
        let pkg = Package {
            name: PackageName::parse("myserver").unwrap(),
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
            bin: Some(PackagePath::parse("myserver-bin").unwrap()),
            sha256: Some(digest.clone()),
        };
        let mut phases = Vec::new();
        let receipt = install_from_url(&pkg, &asset, &url, &digest, root, |ph, _, _| {
            phases.push(ph)
        })
        .unwrap();

        assert_eq!(receipt.name, "myserver");
        assert_eq!(receipt.version.as_deref(), Some("1.2.3"));
        assert!(store::is_installed(root, &pkg.name));
        let binp = store::bin_path(root, &pkg.name).unwrap();
        assert_eq!(std::fs::read(&binp).unwrap(), b"#!/bin/sh\necho hi\n");
        assert!(phases.contains(&InstallPhase::Done));
    }

    #[test]
    fn asset_file_template_links_extracted_binary() {
        assert_eq!(
            resolve_bin_template(
                "{{source.asset.file}}",
                &PackagePath::parse("marksman").unwrap()
            )
            .unwrap()
            .as_str(),
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
        bin.insert(PackageName::parse("ts").unwrap(), "{{x}}".to_string());
        let pkg = Package {
            name: PackageName::parse("ts").unwrap(),
            description: String::new(),
            languages: vec![],
            categories: vec![],
            source_id: "pkg:npm/ts@1".into(),
            assets: vec![],
            bin,
        };
        let first = |kind| {
            source_links(kind, &pkg)
                .unwrap()
                .values()
                .next()
                .unwrap()
                .as_str()
                .to_string()
        };
        assert_eq!(first("npm"), "node_modules/.bin/ts");
        assert_eq!(first("pypi"), "venv/bin/ts");
        assert_eq!(first("cargo"), "bin/ts");
        assert_eq!(first("golang"), "bin/ts");
    }
}
