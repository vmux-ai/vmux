use std::collections::BTreeMap;
use std::path::Path;

use vmux_core::event::InstallPhase;

use crate::lsp::target::Asset;
use crate::lsp::{archive, catalog::Package, download, purl, store, target};

fn resolve_bin_template(tmpl: &str, asset_bin: &str) -> String {
    tmpl.replace("{{source.asset.bin}}", asset_bin)
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
        ns, p.name, ver, asset.file
    ))
}

pub fn install_from_url(
    pkg: &Package,
    asset: &Asset,
    url: &str,
    store_root: &Path,
    mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<store::Receipt, String> {
    let asset_bin = asset.bin.clone().unwrap_or_else(|| pkg.name.clone());

    let staging = store::staging_dir(store_root).join(&pkg.name);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let dl = staging.join(&asset.file);

    emit(InstallPhase::Downloading, Some(0), url);
    download::download_to(url, &dl, |d, total| {
        let pct = total.and_then(|t| (t > 0).then(|| ((d * 100) / t) as u8));
        emit(InstallPhase::Downloading, pct, "downloading");
    })?;

    let pkgdir = store::packages_dir(store_root).join(&pkg.name);
    let _ = std::fs::remove_dir_all(&pkgdir);
    std::fs::create_dir_all(&pkgdir).map_err(|e| e.to_string())?;
    emit(InstallPhase::Extracting, None, "extracting");
    archive::extract(&dl, archive::kind_for(&asset.file), &pkgdir, &asset_bin)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let p = pkgdir.join(&asset_bin);
        if let Ok(meta) = std::fs::metadata(&p) {
            let mut perm = meta.permissions();
            perm.set_mode(0o755);
            let _ = std::fs::set_permissions(&p, perm);
        }
    }

    emit(InstallPhase::Linking, None, "linking");
    let mut links: BTreeMap<String, String> = BTreeMap::new();
    if pkg.bin.is_empty() {
        links.insert(pkg.name.clone(), asset_bin.clone());
    } else {
        for (link_name, tmpl) in &pkg.bin {
            links.insert(link_name.clone(), resolve_bin_template(tmpl, &asset_bin));
        }
    }
    for (link_name, file) in &links {
        store::link_bin(store_root, &pkg.name, file, link_name).map_err(|e| e.to_string())?;
    }

    let receipt = store::Receipt {
        name: pkg.name.clone(),
        version: purl::parse(&pkg.source_id).and_then(|p| p.version),
        source_id: pkg.source_id.clone(),
        bin: links,
    };
    store::write_receipt(store_root, &receipt).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&staging);
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
    let url = asset_url(pkg, &asset)?;
    install_from_url(pkg, &asset, &url, store_root, emit)
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

pub fn source_links(kind: &str, pkg: &Package) -> BTreeMap<String, String> {
    let keys: Vec<String> = if pkg.bin.is_empty() {
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
    keys.into_iter()
        .map(|k| (k.clone(), format!("{prefix}{k}")))
        .collect()
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
    kind: &str,
    p: &purl::Purl,
    emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<store::Receipt, String> {
    emit(InstallPhase::Linking, None, "linking");
    let pkgdir = store::packages_dir(store_root).join(&pkg.name);
    let links = source_links(kind, pkg);
    for (link_name, file) in &links {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let bin = pkgdir.join(file);
            if let Ok(meta) = std::fs::metadata(&bin) {
                let mut perm = meta.permissions();
                perm.set_mode(0o755);
                let _ = std::fs::set_permissions(&bin, perm);
            }
        }
        store::link_bin(store_root, &pkg.name, file, link_name).map_err(|e| e.to_string())?;
    }
    let receipt = store::Receipt {
        name: pkg.name.clone(),
        version: p.version.clone(),
        source_id: pkg.source_id.clone(),
        bin: links,
    };
    store::write_receipt(store_root, &receipt).map_err(|e| e.to_string())?;
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
    let pkgdir = store::packages_dir(store_root).join(&pkg.name);
    let _ = std::fs::remove_dir_all(&pkgdir);
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
    finalize_links(pkg, store_root, &p.kind, p, &mut emit)
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
        let receipt =
            install_from_url(&pkg, &asset, &url, root, |ph, _, _| phases.push(ph)).unwrap();

        assert_eq!(receipt.name, "myserver");
        assert_eq!(receipt.version.as_deref(), Some("1.2.3"));
        assert!(store::is_installed(root, "myserver"));
        let binp = store::bin_path(root, "myserver").unwrap();
        assert_eq!(std::fs::read(&binp).unwrap(), b"#!/bin/sh\necho hi\n");
        assert!(phases.contains(&InstallPhase::Done));
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
}
