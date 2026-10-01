use std::collections::BTreeMap;
use std::path::Path;

use vmux_ecs::event::InstallPhase;

use crate::lsp::download::RemoteArtifact;
#[cfg(test)]
use crate::lsp::package_path::Sha256Digest;
use crate::lsp::package_path::{PackageName, PackagePath};
use crate::lsp::purl::Purl;
use crate::lsp::target::{Asset, PlatformTarget};
use crate::lsp::{archive, catalog::Package, download, store};

fn resolve_bin_template(tmpl: &str, asset_bin: &PackagePath) -> Result<PackagePath, String> {
    PackagePath::parse(
        &tmpl
            .replace("{{source.asset.bin}}", asset_bin.as_str())
            .replace("{{source.asset.file}}", asset_bin.as_str()),
    )
}

#[derive(Debug, PartialEq, Eq)]
struct InstallCommand {
    program: String,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
}

impl InstallCommand {
    fn for_source(source: &Purl, package_dir: &Path) -> Result<Vec<Self>, String> {
        let version = source.version.as_deref().unwrap_or("latest");
        let namespaced_name = match &source.namespace {
            Some(namespace) => format!("{namespace}/{}", source.name),
            None => source.name.clone(),
        };
        match source.kind.as_str() {
            "npm" => Ok(vec![Self {
                program: "npm".to_string(),
                arguments: vec![
                    "install".to_string(),
                    "--prefix".to_string(),
                    package_dir.to_string_lossy().into_owned(),
                    format!("{namespaced_name}@{version}"),
                ],
                environment: Vec::new(),
            }]),
            "cargo" => {
                let mut arguments = vec![
                    "install".to_string(),
                    "--root".to_string(),
                    package_dir.to_string_lossy().into_owned(),
                ];
                if let Some(version) = &source.version {
                    arguments.push("--version".to_string());
                    arguments.push(version.clone());
                }
                arguments.push(source.name.clone());
                Ok(vec![Self {
                    program: "cargo".to_string(),
                    arguments,
                    environment: Vec::new(),
                }])
            }
            "golang" => Ok(vec![Self {
                program: "go".to_string(),
                arguments: vec![
                    "install".to_string(),
                    format!("{namespaced_name}@{version}"),
                ],
                environment: vec![(
                    "GOBIN".to_string(),
                    package_dir.join("bin").to_string_lossy().into_owned(),
                )],
            }]),
            "pypi" => {
                let venv = package_dir.join("venv");
                let spec = match &source.version {
                    Some(version) => format!("{}=={version}", source.name),
                    None => source.name.clone(),
                };
                Ok(vec![
                    Self {
                        program: "python3".to_string(),
                        arguments: vec![
                            "-m".to_string(),
                            "venv".to_string(),
                            venv.to_string_lossy().into_owned(),
                        ],
                        environment: Vec::new(),
                    },
                    Self {
                        program: venv.join("bin/pip").to_string_lossy().into_owned(),
                        arguments: vec!["install".to_string(), spec],
                        environment: Vec::new(),
                    },
                ])
            }
            other => Err(format!("source '{other}' not supported")),
        }
    }

    fn run(&self) -> Result<(), String> {
        let mut command = std::process::Command::new(&self.program);
        command.args(&self.arguments);
        for (key, value) in &self.environment {
            command.env(key, value);
        }
        let status = command
            .status()
            .map_err(|error| format!("{}: {error}", self.program))?;
        if !status.success() {
            return Err(format!("{} failed ({status})", self.program));
        }
        Ok(())
    }
}

impl Package {
    fn source(&self) -> Result<Purl, String> {
        Purl::parse(&self.source_id).ok_or_else(|| "bad purl".to_string())
    }

    fn source_links(&self, source: &Purl) -> Result<BTreeMap<PackageName, PackagePath>, String> {
        let names = if self.bin.is_empty() {
            vec![self.name.clone()]
        } else {
            self.bin.keys().cloned().collect()
        };
        let prefix = match source.kind.as_str() {
            "npm" => "node_modules/.bin/",
            "pypi" => "venv/bin/",
            "cargo" | "golang" => "bin/",
            _ => "",
        };
        let mut links = BTreeMap::new();
        for name in names {
            links.insert(
                name.clone(),
                PackagePath::parse(&format!("{prefix}{}", name.as_str()))?,
            );
        }
        Ok(links)
    }

    fn install_from_url(
        &self,
        source: &Purl,
        asset: &Asset,
        artifact: &RemoteArtifact,
        store: &store::PackageStore,
        mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<store::Receipt, String> {
        let asset_bin = asset
            .bin
            .clone()
            .unwrap_or_else(|| PackagePath::from(&self.name));
        let staging_root = store.staging_dir();
        std::fs::create_dir_all(&staging_root).map_err(|error| error.to_string())?;
        let staging = tempfile::Builder::new()
            .prefix(self.name.as_str())
            .tempdir_in(&staging_root)
            .map_err(|error| error.to_string())?;
        let download_path = staging.path().join(asset.file.as_path());

        emit(InstallPhase::Downloading, Some(0), &artifact.url);
        artifact.download_to(
            &download_path,
            download::PACKAGE_MAX_BYTES,
            |downloaded, total| {
                let percent =
                    total.and_then(|total| (total > 0).then(|| ((downloaded * 100) / total) as u8));
                emit(InstallPhase::Downloading, percent, "downloading");
            },
        )?;

        let package_dir = staging.path().join("package");
        std::fs::create_dir_all(&package_dir).map_err(|error| error.to_string())?;
        emit(InstallPhase::Extracting, None, "extracting");
        archive::ArchiveKind::for_file(asset.file.as_str()).extract(
            &download_path,
            &package_dir,
            asset_bin.as_str(),
        )?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = package_dir.join(asset_bin.as_path());
            if let Ok(metadata) = std::fs::metadata(&path) {
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o755);
                let _ = std::fs::set_permissions(&path, permissions);
            }
        }

        emit(InstallPhase::Linking, None, "linking");
        let mut links = BTreeMap::new();
        if self.bin.is_empty() {
            links.insert(self.name.clone(), asset_bin.clone());
        } else {
            for (link_name, template) in &self.bin {
                links.insert(
                    link_name.clone(),
                    resolve_bin_template(template, &asset_bin)?,
                );
            }
        }
        for file in links.values() {
            if !package_dir.join(file.as_path()).is_file() {
                return Err(format!("package binary is missing: {file}"));
            }
        }

        let receipt = store::Receipt {
            name: self.name.clone(),
            version: source.version.clone(),
            source_id: self.source_id.clone(),
            bin: links.clone(),
        };
        receipt
            .write_to(&package_dir)
            .map_err(|error| error.to_string())?;
        store
            .activate_package(&self.name, &package_dir)
            .map_err(|error| error.to_string())?;
        for (link_name, file) in &links {
            store
                .link_bin(&self.name, file, link_name)
                .map_err(|error| error.to_string())?;
        }
        emit(InstallPhase::Done, Some(100), "installed");
        Ok(receipt)
    }

    fn install_github(
        &self,
        source: &Purl,
        store: &store::PackageStore,
        target: PlatformTarget,
        mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<store::Receipt, String> {
        emit(InstallPhase::Resolving, None, "selecting asset");
        let asset = target
            .select(&self.assets)
            .ok_or_else(|| format!("no prebuilt asset for {}", target.as_str()))?
            .clone();
        let owner = source
            .namespace
            .as_deref()
            .ok_or("github purl missing owner")?;
        let version = source
            .version
            .as_deref()
            .ok_or("github purl missing version")?;
        let artifact = RemoteArtifact::github_release(
            owner,
            &source.name,
            Some(version),
            asset.file.as_str(),
            download::PACKAGE_MAX_BYTES,
        )?;
        if let Some(expected) = asset.sha256.as_ref()
            && expected != &artifact.sha256
        {
            return Err("catalog and GitHub asset digests disagree".to_string());
        }
        self.install_from_url(source, &asset, &artifact, store, emit)
    }

    fn finalize_links(
        &self,
        source: &Purl,
        store: &store::PackageStore,
        staged_package: &Path,
        emit: &mut impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<store::Receipt, String> {
        emit(InstallPhase::Linking, None, "linking");
        let links = self.source_links(source)?;
        for file in links.values() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let binary = staged_package.join(file.as_path());
                if let Ok(metadata) = std::fs::metadata(&binary) {
                    let mut permissions = metadata.permissions();
                    permissions.set_mode(0o755);
                    let _ = std::fs::set_permissions(&binary, permissions);
                }
            }
        }
        let receipt = store::Receipt {
            name: self.name.clone(),
            version: source.version.clone(),
            source_id: self.source_id.clone(),
            bin: links.clone(),
        };
        receipt
            .write_to(staged_package)
            .map_err(|error| error.to_string())?;
        store
            .activate_package(&self.name, staged_package)
            .map_err(|error| error.to_string())?;
        for (link_name, file) in &links {
            store
                .link_bin(&self.name, file, link_name)
                .map_err(|error| error.to_string())?;
        }
        emit(InstallPhase::Done, Some(100), "installed");
        Ok(receipt)
    }

    fn install_toolchain(
        &self,
        source: &Purl,
        store: &store::PackageStore,
        mut emit: impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<store::Receipt, String> {
        let toolchain = source.toolchain().ok_or("unknown source")?;
        if !crate::lsp::registry::executable_on_path(toolchain) {
            return Err(format!("requires {toolchain}"));
        }
        let staging_root = store.staging_dir();
        std::fs::create_dir_all(&staging_root).map_err(|error| error.to_string())?;
        let staging = tempfile::Builder::new()
            .prefix(self.name.as_str())
            .tempdir_in(&staging_root)
            .map_err(|error| error.to_string())?;
        let package_dir = staging.path().join("package");
        std::fs::create_dir_all(&package_dir).map_err(|error| error.to_string())?;
        emit(InstallPhase::Downloading, None, toolchain);
        for command in InstallCommand::for_source(source, &package_dir)? {
            command.run()?;
        }
        self.finalize_links(source, store, &package_dir, &mut emit)
    }

    pub(crate) fn install(
        &self,
        store: &store::PackageStore,
        target: PlatformTarget,
        emit: impl FnMut(InstallPhase, Option<u8>, &str),
    ) -> Result<store::Receipt, String> {
        let source = self.source()?;
        match source.kind.as_str() {
            "github" => self.install_github(&source, store, target, emit),
            "npm" | "pypi" | "cargo" | "golang" => self.install_toolchain(&source, store, emit),
            other => Err(format!("install source '{other}' not yet supported")),
        }
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
    fn install_from_url_extracts_links_and_writes_receipt() {
        let (url, file, digest) = serve_gz_once(b"#!/bin/sh\necho hi\n");
        let tmp = tempfile::tempdir().unwrap();
        let store = store::PackageStore::at(tmp.path());
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
        let source = pkg.source().unwrap();
        let artifact = RemoteArtifact::new(url, digest);
        let receipt = pkg
            .install_from_url(&source, &asset, &artifact, &store, |phase, _, _| {
                phases.push(phase)
            })
            .unwrap();

        assert_eq!(receipt.name.as_str(), "myserver");
        assert_eq!(receipt.version.as_deref(), Some("1.2.3"));
        assert!(store.is_installed(&pkg.name));
        let binp = store.bin_path(&pkg.name).unwrap();
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
        assert_eq!(
            Purl::parse("pkg:npm/tool").unwrap().toolchain(),
            Some("npm")
        );
        assert_eq!(
            Purl::parse("pkg:pypi/tool").unwrap().toolchain(),
            Some("python3")
        );
        assert_eq!(
            Purl::parse("pkg:cargo/tool").unwrap().toolchain(),
            Some("cargo")
        );
        assert_eq!(
            Purl::parse("pkg:golang/tool").unwrap().toolchain(),
            Some("go")
        );
        assert_eq!(Purl::parse("pkg:github/x/tool").unwrap().toolchain(), None);
    }

    #[test]
    fn source_install_commands() {
        let package_dir = std::path::Path::new("/tmp/pkg");
        let npm = Purl::parse("pkg:npm/typescript-language-server@4.0.0").unwrap();
        let commands = InstallCommand::for_source(&npm, package_dir).unwrap();
        assert_eq!(commands[0].program, "npm");
        assert!(
            commands[0]
                .arguments
                .contains(&"typescript-language-server@4.0.0".to_string())
        );
        assert!(commands[0].arguments.contains(&"--prefix".to_string()));

        let cargo = Purl::parse("pkg:cargo/taplo-cli@0.9.0").unwrap();
        let commands = InstallCommand::for_source(&cargo, package_dir).unwrap();
        assert!(commands[0].arguments.contains(&"--version".to_string()));
        assert!(commands[0].arguments.contains(&"0.9.0".to_string()));
        assert!(commands[0].arguments.contains(&"taplo-cli".to_string()));

        let go = Purl::parse("pkg:golang/golang.org/x/tools/gopls@v0.16.0").unwrap();
        let commands = InstallCommand::for_source(&go, package_dir).unwrap();
        assert!(
            commands[0]
                .arguments
                .contains(&"golang.org/x/tools/gopls@v0.16.0".to_string())
        );

        let pypi = Purl::parse("pkg:pypi/ruff@0.5.0").unwrap();
        let commands = InstallCommand::for_source(&pypi, package_dir).unwrap();
        assert_eq!(commands[1].arguments, ["install", "ruff==0.5.0"]);
        let pypi = Purl::parse("pkg:pypi/ruff").unwrap();
        let commands = InstallCommand::for_source(&pypi, package_dir).unwrap();
        assert_eq!(commands[1].arguments, ["install", "ruff"]);
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
            let source = Purl::parse(&format!("pkg:{kind}/ts@1")).unwrap();
            pkg.source_links(&source)
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
