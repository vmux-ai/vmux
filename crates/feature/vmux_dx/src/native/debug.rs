use sha2::Digest;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::Path;

use crate::Result;
use crate::execution::Execution;
use crate::temporary::Temporary;

use super::Native;

impl Native {
    pub fn identity(&self) -> Result<String> {
        if self.environment.present("APPLE_SIGNING_IDENTITY") {
            return Ok(self
                .environment
                .required("APPLE_SIGNING_IDENTITY")?
                .to_str()
                .ok_or("Invalid signing identity")?
                .into());
        }
        if !cfg!(target_os = "macos") {
            return Ok(String::new());
        }
        let identity = self.get("VMUX_LOCAL_SIGNING_IDENTITY", "Vmux Dev");
        let identity = identity.to_str().ok_or("Invalid signing identity")?;
        if identity.contains(['\r', '\n']) {
            return Err("Signing identity must be a single line".into());
        }
        let keychain = match self.environment.get("VMUX_LOCAL_CODESIGN_KEYCHAIN") {
            Some(path) => path.to_owned(),
            None => self
                .text(
                    "security",
                    &[
                        OsStr::new("default-keychain"),
                        OsStr::new("-d"),
                        OsStr::new("user"),
                    ],
                )?
                .trim()
                .trim_matches('"')
                .into(),
        };
        if keychain.is_empty() {
            return Err("Default user keychain not found".into());
        }
        let identities = self.text(
            "security",
            &[
                OsStr::new("find-identity"),
                OsStr::new("-v"),
                OsStr::new("-p"),
                OsStr::new("codesigning"),
                &keychain,
            ],
        )?;
        if identities.contains(&format!("\"{identity}\"")) {
            return Ok(identity.into());
        }
        let temporary = Temporary::create(&std::env::temp_dir(), "vmux-local-codesign")?;
        let config = temporary.path().join("openssl.cnf");
        let key = temporary.path().join("key.pem");
        let cert = temporary.path().join("cert.pem");
        let p12 = temporary.path().join("identity.p12");
        fs::write(
            &config,
            format!(
                "[req]\ndistinguished_name=req_distinguished_name\nx509_extensions=v3_codesign\nprompt=no\n[req_distinguished_name]\nCN={identity}\n[v3_codesign]\nbasicConstraints=critical,CA:false\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=critical,codeSigning\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid:always\n"
            ),
        )?;
        let password = self.text("uuidgen", &[])?;
        let password = password.trim();
        self.run_command(
            "openssl",
            &[
                OsStr::new("req"),
                OsStr::new("-new"),
                OsStr::new("-newkey"),
                OsStr::new("rsa:2048"),
                OsStr::new("-nodes"),
                OsStr::new("-x509"),
                OsStr::new("-days"),
                OsStr::new("3650"),
                OsStr::new("-sha256"),
                OsStr::new("-config"),
                config.as_os_str(),
                OsStr::new("-keyout"),
                key.as_os_str(),
                OsStr::new("-out"),
                cert.as_os_str(),
            ],
        )?;
        self.run_command(
            "openssl",
            &[
                OsStr::new("pkcs12"),
                OsStr::new("-export"),
                OsStr::new("-out"),
                p12.as_os_str(),
                OsStr::new("-inkey"),
                key.as_os_str(),
                OsStr::new("-in"),
                cert.as_os_str(),
                OsStr::new("-passout"),
                OsStr::new(&format!("pass:{password}")),
                OsStr::new("-keypbe"),
                OsStr::new("PBE-SHA1-3DES"),
                OsStr::new("-certpbe"),
                OsStr::new("PBE-SHA1-3DES"),
                OsStr::new("-macalg"),
                OsStr::new("sha1"),
            ],
        )?;
        self.run_command(
            "security",
            &[
                OsStr::new("import"),
                p12.as_os_str(),
                OsStr::new("-f"),
                OsStr::new("pkcs12"),
                OsStr::new("-k"),
                &keychain,
                OsStr::new("-P"),
                OsStr::new(password),
                OsStr::new("-T"),
                OsStr::new("/usr/bin/codesign"),
            ],
        )?;
        self.run_command(
            "security",
            &[
                OsStr::new("add-trusted-cert"),
                OsStr::new("-r"),
                OsStr::new("trustRoot"),
                OsStr::new("-p"),
                OsStr::new("codeSign"),
                OsStr::new("-k"),
                &keychain,
                cert.as_os_str(),
            ],
        )?;
        let mut partition = self.command("security");
        partition
            .args([
                "set-key-partition-list",
                "-S",
                "apple-tool:,apple:,codesign:",
                "-s",
                "-l",
            ])
            .arg(identity)
            .arg(&keychain);
        if !Execution::new(partition).output_any(None)?.status.success() {
            eprintln!("codesign may request keychain access");
        }
        let identities = self.text(
            "security",
            &[
                OsStr::new("find-identity"),
                OsStr::new("-v"),
                OsStr::new("-p"),
                OsStr::new("codesigning"),
                &keychain,
            ],
        )?;
        if !identities.contains(&format!("\"{identity}\"")) {
            return Err("Created identity is not valid for codesigning".into());
        }
        Ok(identity.into())
    }

    pub fn clear_ready(&self) -> Result<()> {
        let name = format!("{} Dev.app", self.product()?.value("VMUX_HOST_NAME"));
        let debug = self.checkout.directory("debug");
        fs::create_dir_all(&debug)?;
        let stamp = debug.join(".vmux-dev-ready");
        if stamp.exists() {
            fs::remove_file(stamp)?;
        }
        let alias = self.checkout.directory("release").join(name);
        if fs::symlink_metadata(&alias).is_ok_and(|metadata| metadata.is_symlink()) {
            fs::remove_file(alias)?;
        }
        Ok(())
    }

    fn sign(&self, path: &Path, identifier: Option<&str>) -> Result<()> {
        let product = self.product()?;
        let mut command = self.command("codesign");
        command
            .args(["--force", "--sign"])
            .arg(self.environment.required("APPLE_SIGNING_IDENTITY")?)
            .args(["--options", "runtime", "--entitlements"])
            .arg(product.value("VMUX_HOST_DEV_ENTITLEMENTS"));
        if let Some(keychain) = self.environment.get("CODESIGN_KEYCHAIN") {
            command.arg("--keychain").arg(keychain);
        }
        if let Some(identifier) = identifier {
            command.arg("--identifier").arg(identifier);
        }
        command.arg(path);
        Execution::new(command).run()
    }

    fn verify_signature(&self, path: &Path) -> Result<()> {
        self.run_command(
            "codesign",
            &[
                OsStr::new("--verify"),
                OsStr::new("--strict"),
                path.as_os_str(),
            ],
        )
    }

    fn signature_current(
        &self,
        stamp: &Path,
        inputs: &str,
        binary: &Path,
        bundle: &Path,
    ) -> Result<bool> {
        if !stamp.is_file() || !binary.is_file() {
            return Ok(false);
        }
        let expected = format!("{inputs}\n{}\n", self.checksum(binary)?);
        if fs::read_to_string(stamp)? != expected {
            return Ok(false);
        }
        let mut command = self.command("codesign");
        command.args(["--verify", "--strict"]).arg(bundle);
        Ok(Execution::new(command).output_any(None)?.status.success())
    }

    fn signing_inputs(&self, paths: &[&Path]) -> Result<String> {
        let mut input = self
            .environment
            .required("APPLE_SIGNING_IDENTITY")?
            .as_encoded_bytes()
            .to_vec();
        input.extend(
            self.checksum(Path::new(
                self.product()?.value("VMUX_HOST_DEV_ENTITLEMENTS"),
            ))?
            .bytes(),
        );
        for path in paths {
            input.extend(self.checksum(path)?.bytes());
        }
        Ok(format!("{:x}", sha2::Sha256::digest(input)))
    }

    fn record_signature(&self, stamp: &Path, inputs: &str, binary: &Path) -> Result<()> {
        let temporary = stamp.with_extension("tmp");
        fs::write(
            &temporary,
            format!("{inputs}\n{}\n", self.checksum(binary)?),
        )?;
        fs::rename(temporary, stamp)?;
        Ok(())
    }

    pub fn sign_debug(&self) -> Result<()> {
        let product = self.product()?;
        let helper = self
            .framework()?
            .join("Libraries/bevy_cef_debug_render_process");
        let binary = self.checkout.directory("debug").join("vmux_desktop");
        for (path, id) in [
            (
                &helper,
                format!(
                    "{}.dev.helper.bevy-cef-debug-render-process",
                    product.value("VMUX_HOST_BUNDLE_ID")
                ),
            ),
            (
                &binary,
                format!("{}.dev", product.value("VMUX_HOST_BUNDLE_ID")),
            ),
        ] {
            let stamp = path.with_extension("dev-signed");
            let inputs = format!("{}\n{id}", self.signing_inputs(&[path])?);
            if self.signature_current(&stamp, &inputs, path, path)? {
                continue;
            }
            let format = self.text("file", &[path.as_os_str()])?;
            if !format.contains("Mach-O") {
                return Err("Cannot sign a non-Mach-O development binary".into());
            }
            self.sign(path, Some(&id))?;
            self.verify_signature(path)?;
            let inputs = format!("{}\n{id}", self.signing_inputs(&[path])?);
            self.record_signature(&stamp, &inputs, path)?;
        }
        Ok(())
    }

    pub(super) fn plist(&self, path: &Path, command: &str) -> Result<()> {
        self.run_command(
            self.get("PLIST_BUDDY", "/usr/libexec/PlistBuddy"),
            &[OsStr::new("-c"), OsStr::new(command), path.as_os_str()],
        )
    }

    pub fn bundle_debug(&self) -> Result<()> {
        let product = self.product()?;
        let debug = self.checkout.directory("debug");
        let app = debug.join(format!("{} Dev.app", product.value("VMUX_HOST_NAME")));
        let contents = app.join("Contents");
        fs::create_dir_all(contents.join("MacOS"))?;
        fs::create_dir_all(contents.join("Resources"))?;
        fs::create_dir_all(contents.join("Frameworks"))?;
        let plist = contents.join("Info.plist");
        self.copy(
            &self.checkout.root.join("packaging/macos/Info.plist"),
            &plist,
        )?;
        self.plist(
            &plist,
            &format!(
                "Set :CFBundleIdentifier {}.dev",
                product.value("VMUX_HOST_BUNDLE_ID")
            ),
        )?;
        let title = serde_json::to_string(
            &self
                .checkout
                .title("dev", product.value("VMUX_HOST_NAME"))?,
        )?;
        for key in ["CFBundleDisplayName", "CFBundleName"] {
            self.plist(&plist, &format!("Set :{key} {title}"))?;
        }
        self.plist(
            &plist,
            &format!(
                "Merge {}",
                serde_json::to_string(product.value("VMUX_HOST_USAGE_DESCRIPTIONS"))?
            ),
        )?;
        let manifest: toml::Table =
            fs::read_to_string(self.checkout.root.join("Cargo.toml"))?.parse()?;
        let version = manifest
            .get("workspace")
            .and_then(|workspace| workspace.get("package"))
            .and_then(|package| package.get("version"))
            .and_then(toml::Value::as_str)
            .ok_or("Missing workspace version")?;
        for key in ["CFBundleShortVersionString", "CFBundleVersion"] {
            self.plist(&plist, &format!("Set :{key} {version}"))?;
        }
        self.copy(
            Path::new(product.value("VMUX_HOST_ICON")),
            &contents.join("Resources/Vmux.icns"),
        )?;
        self.copy(
            Path::new(product.value("VMUX_HOST_TRAY_ICON")),
            &contents.join("Resources/tray.png"),
        )?;
        self.copy(&debug.join("vmux"), &contents.join("MacOS/vmux"))?;
        let service = contents
            .join("Library/LoginItems")
            .join(product.value("VMUX_HOST_SERVICE_BUNDLE"));
        let service_contents = service.join("Contents");
        let service_plist = service_contents.join("Info.plist");
        self.copy(&plist, &service_plist)?;
        for key in ["CFBundleExecutable", "CFBundleName", "CFBundleDisplayName"] {
            self.plist(
                &service_plist,
                &format!(
                    "Set :{key} {}",
                    serde_json::to_string(product.value("VMUX_HOST_SERVICE_NAME"))?
                ),
            )?;
        }
        self.plist(
            &service_plist,
            &format!(
                "Set :CFBundleIdentifier {}.dev",
                product.value("VMUX_HOST_SERVICE_ID")
            ),
        )?;
        self.plist(&service_plist, "Set :LSUIElement true")?;
        let service_binary = service_contents
            .join("MacOS")
            .join(product.value("VMUX_HOST_SERVICE_NAME"));
        let service_inputs = self.signing_inputs(&[&debug.join("vmux_service"), &service_plist])?;
        let service_stamp = debug.join(".vmux-dev-service-signed");
        if !self.signature_current(&service_stamp, &service_inputs, &service_binary, &service)? {
            self.copy(&debug.join("vmux_service"), &service_binary)?;
            self.sign(&service, None)?;
            self.record_signature(&service_stamp, &service_inputs, &service_binary)?;
        }
        let binary = contents.join("MacOS/vmux_desktop");
        let stdlib = self.text(
            self.get("RUSTC", "rustc"),
            &[OsStr::new("--print"), OsStr::new("target-libdir")],
        )?;
        let inputs = format!(
            "{}\n{}\n{}",
            self.signing_inputs(&[
                &debug.join("vmux_desktop"),
                &contents.join("MacOS/vmux"),
                &plist,
                &contents.join("Resources/Vmux.icns"),
                &service_stamp
            ])?,
            debug.join("deps").display(),
            stdlib.trim()
        );
        let stamp = debug.join(".vmux-dev-desktop-signed");
        if self.signature_current(&stamp, &inputs, &binary, &app)? {
            return Ok(());
        }
        self.copy(&debug.join("vmux_desktop"), &binary)?;
        let existing = self.text("otool", &[OsStr::new("-l"), binary.as_os_str()])?;
        for directory in [debug.join("deps"), stdlib.trim().into()] {
            if !existing.contains(&format!("path {} (", directory.display())) {
                self.run_command(
                    "install_name_tool",
                    &[
                        OsStr::new("-add_rpath"),
                        directory.as_os_str(),
                        binary.as_os_str(),
                    ],
                )?;
            }
        }
        let libraries = self.text("otool", &[OsStr::new("-L"), binary.as_os_str()])?;
        for line in libraries.lines() {
            if !line.contains("libbevy_dylib") {
                continue;
            }
            let library = line
                .trim()
                .split(" (compatibility")
                .next()
                .ok_or("Malformed library path")?;
            let name = Path::new(library)
                .file_name()
                .ok_or("Missing library name")?;
            if !debug.join("deps").join(name).is_file() {
                return Err("Missing shared Bevy library".into());
            }
            let new = format!("@rpath/{}", name.to_string_lossy());
            if library != new {
                self.run_command(
                    "install_name_tool",
                    &[
                        OsStr::new("-change"),
                        OsStr::new(library),
                        OsStr::new(&new),
                        binary.as_os_str(),
                    ],
                )?;
            }
        }
        self.sign(&app, None)?;
        self.verify_signature(&app)?;
        self.record_signature(&stamp, &inputs, &binary)?;
        Ok(())
    }

    pub fn publish_debug(&self) -> Result<()> {
        let debug = self.checkout.directory("debug");
        let mut stamp = format!("{}\n", self.checkout.root.display());
        let mut paths = Vec::new();
        for name in ["vmux", "vmux_service", "vmux_desktop"] {
            let path = debug.join(name);
            if fs::metadata(&path)?.permissions().mode() & 0o111 == 0 {
                return Err(format!("Missing executable: {name}").into());
            }
            paths.push(path);
        }
        if cfg!(target_os = "macos") {
            let name = format!("{} Dev.app", self.product()?.value("VMUX_HOST_NAME"));
            paths.push(debug.join(&name).join("Contents/MacOS/vmux_desktop"));
            let alias = self.checkout.directory("release").join(&name);
            fs::create_dir_all(alias.parent().unwrap())?;
            if fs::symlink_metadata(&alias).is_ok() {
                return Err("Cannot publish debug app over existing artifact".into());
            }
            symlink(debug.join(name), alias)?;
        }
        for path in paths {
            stamp.push_str(&format!("{}  {}\n", self.checksum(&path)?, path.display()));
        }
        let temporary = debug.join(".vmux-dev-ready.tmp");
        fs::write(&temporary, stamp)?;
        fs::rename(temporary, debug.join(".vmux-dev-ready"))?;
        println!("Debug build ready: {}\nRun: vmux run", debug.display());
        Ok(())
    }

    fn ready(&self) -> Result<()> {
        let stamp = fs::read_to_string(self.checkout.directory("debug").join(".vmux-dev-ready"))?;
        let mut lines = stamp.lines();
        if lines.next() != self.checkout.root.to_str() {
            return Err("Artifact belongs to another checkout".into());
        }
        let mut count = 0;
        for line in lines {
            let (checksum, path) = line.split_once("  ").ok_or("Invalid readiness stamp")?;
            let path = Path::new(path);
            if !path.starts_with(self.checkout.directory("debug"))
                || self.checksum(path)? != checksum
            {
                return Err(
                    "Development artifact is incomplete or has changed; run vmux build".into(),
                );
            }
            count += 1;
        }
        if count != if cfg!(target_os = "macos") { 4 } else { 3 } {
            return Err("Incomplete readiness stamp".into());
        }
        Ok(())
    }

    pub fn run(&self, profile: &str, platform: &str) -> Result<()> {
        if platform == "web" {
            let mut watcher = self.command(self.get("TAILWINDCSS_BIN", "tailwindcss"));
            watcher
                .current_dir(self.checkout.root.join("website"))
                .args([
                    "-i",
                    "tailwind.input.css",
                    "-o",
                    "public/style.css",
                    "--watch",
                ])
                .process_group(0);
            let _watcher = Watcher(watcher.spawn()?);
            let mut command = self.command(self.get("DX_BIN", "dx"));
            command
                .current_dir(self.checkout.root.join("website"))
                .args(["serve", "--platform", "web"]);
            return Execution::new(command).run();
        }
        if platform != "desktop" {
            let mut command = self.command(self.get("DX_BIN", "dx"));
            command
                .arg("serve")
                .args(["-p", "vmux_mobile"])
                .arg(format!("--{platform}"));
            return Execution::new(command).run();
        }
        if profile != "dev" {
            let name = if profile == "local" {
                format!(
                    "{} ({}).app",
                    self.product()?.value("VMUX_HOST_NAME"),
                    self.text(
                        "git",
                        &[
                            OsStr::new("rev-parse"),
                            OsStr::new("--short=7"),
                            OsStr::new("HEAD")
                        ]
                    )?
                    .trim()
                )
            } else {
                format!("{}.app", self.product()?.value("VMUX_HOST_NAME"))
            };
            let app = self.checkout.directory("release").join(name);
            self.verify_signature(&app)?;
            return self.run_command("open", &[app.as_os_str()]);
        }
        self.ready()?;
        if cfg!(target_os = "macos") {
            let app = self.checkout.directory("debug").join(format!(
                "{} Dev.app",
                self.product()?.value("VMUX_HOST_NAME")
            ));
            self.verify_signature(&app)?;
            let mut command = self.command("open");
            for key in ["VMUX_PROFILE", "VMUX_TEST", "VMUX_DATA_DIR"] {
                if let Some(value) = self.environment.get(key) {
                    command
                        .arg("--env")
                        .arg(format!("{key}={}", value.to_string_lossy()));
                }
            }
            command.arg(app);
            return Execution::new(command).run();
        }
        let mut command = self.command(self.checkout.directory("debug").join("vmux_desktop"));
        command.env_remove("CEF_PATH");
        Execution::new(command).run()
    }

    pub fn stop(&self) -> Result<()> {
        let debug = self.checkout.directory("debug");
        let stdlib = self.text(
            self.get("RUSTC", "rustc"),
            &[OsStr::new("--print"), OsStr::new("target-libdir")],
        )?;
        let mut command = self.command(debug.join("vmux"));
        command
            .args(["service", "stop"])
            .env_remove("CEF_PATH")
            .env(
                "DYLD_LIBRARY_PATH",
                format!(
                    "{}:{}:{}",
                    stdlib.trim(),
                    debug.join("deps").display(),
                    self.get("DYLD_LIBRARY_PATH", "").to_string_lossy()
                ),
            );
        Execution::new(command).run()
    }
}

struct Watcher(std::process::Child);

impl Drop for Watcher {
    fn drop(&mut self) {
        crate::signals::Signals::terminate_group(self.0.id());
        let _ = self.0.wait();
    }
}
