use std::collections::BTreeMap;
use std::path::Path;

use vmux_core::event::InstallPhase;

use crate::lsp::target::Asset;
use crate::lsp::{archive, catalog::Package, download, purl, store, target};

fn resolve_bin_template(tmpl: &str, asset_bin: &str) -> String {
    tmpl.replace("{{source.asset.bin}}", asset_bin)
}

/// GitHub release download URL for `asset` given the package's PURL.
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

/// Download `url` (the `asset`), extract into the package dir, link bins, write a
/// receipt. Split out from `install_github` so it can be tested against a local
/// HTTP fixture without hitting github.com.
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

/// Dispatch install by PURL source kind. Non-github sources land in B2.
pub fn install(
    pkg: &Package,
    store_root: &Path,
    target_id: &str,
    emit: impl FnMut(InstallPhase, Option<u8>, &str),
) -> Result<store::Receipt, String> {
    let kind = purl::parse(&pkg.source_id).map(|p| p.kind).unwrap_or_default();
    match kind.as_str() {
        "github" => install_github(pkg, store_root, target_id, emit),
        other => Err(format!("install source '{other}' not yet supported")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn serve_gz_once(payload: &'static [u8]) -> (String, String) {
        // gzip the payload, serve it, return (url, file_name)
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
                let header =
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", gz.len());
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
        let asset = Asset { target: "darwin_arm64".into(), file: "ra.gz".into(), bin: Some("ra".into()) };
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
        let asset = Asset { target: "darwin_arm64".into(), file, bin: Some("myserver-bin".into()) };
        let mut phases = Vec::new();
        let receipt = install_from_url(&pkg, &asset, &url, root, |ph, _, _| phases.push(ph)).unwrap();

        // receipt + payload extracted + bin symlink present and executable-ish
        assert_eq!(receipt.name, "myserver");
        assert_eq!(receipt.version.as_deref(), Some("1.2.3"));
        assert!(store::is_installed(root, "myserver"));
        let binp = store::bin_path(root, "myserver").unwrap();
        assert_eq!(std::fs::read(&binp).unwrap(), b"#!/bin/sh\necho hi\n");
        assert!(phases.contains(&InstallPhase::Done));
    }

    #[test]
    fn non_github_source_is_unsupported_in_b1() {
        let pkg = Package {
            name: "ruff".into(),
            description: String::new(),
            languages: vec![],
            categories: vec![],
            source_id: "pkg:pypi/ruff@0.5.0".into(),
            assets: vec![],
            bin: Default::default(),
        };
        let tmp = tempfile::tempdir().unwrap();
        let err = install(&pkg, tmp.path(), "darwin_arm64", |_, _, _| {}).unwrap_err();
        assert!(err.contains("pypi"));
    }
}
