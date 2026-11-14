use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::Result;
use crate::execution::Execution;
use crate::temporary::Temporary;

use super::Native;
use super::keychain::Keychain;
use plist::Value;

impl Native {
    pub fn verify_bundle(&self, app: &Path) -> Result<()> {
        let product = self.product()?;
        let mut required = Vec::new();
        for name in [
            "Contents/MacOS/vmux_desktop",
            "Contents/MacOS/vmux",
            "Contents/Frameworks/vmux_desktop Helper.app/Contents/MacOS/vmux_desktop Helper",
            "Contents/Frameworks/vmux_desktop Helper.app/Contents/Resources/Vmux.icns",
            "Contents/Frameworks/vmux_desktop Helper (Renderer).app/Contents/MacOS/vmux_desktop Helper (Renderer)",
            "Contents/Info.plist",
            "Contents/Resources/Vmux.icns",
            "Contents/Resources/crash_reporter.cfg",
        ] {
            required.push(name.to_owned());
        }
        required.extend([
            format!(
                "Contents/Library/LoginItems/{}/Contents/MacOS/{}",
                product.value("VMUX_HOST_SERVICE_BUNDLE"),
                product.value("VMUX_HOST_SERVICE_NAME")
            ),
            format!(
                "Contents/Library/LoginItems/{}/Contents/Resources/Vmux.icns",
                product.value("VMUX_HOST_SERVICE_BUNDLE")
            ),
            format!(
                "Contents/Library/LaunchAgents/{}",
                product.value("VMUX_HOST_AGENT_PLIST")
            ),
        ]);
        for name in required {
            if !app.join(&name).exists() {
                return Err(format!("Missing bundle resource: {name}").into());
            }
        }
        for name in [
            "Contents/MacOS/vmux_service",
            "Contents/Frameworks/Vmux Helper.app",
        ] {
            if app.join(name).exists() {
                return Err(format!("Unexpected bundle resource: {name}").into());
            }
        }
        if fs::read(
            app.join("Contents/Library/LaunchAgents")
                .join(product.value("VMUX_HOST_AGENT_PLIST")),
        )?
        .windows(11)
        .any(|window| window == b"{{PROFILE}}")
        {
            return Err("LaunchAgent still contains its profile placeholder".into());
        }
        if self.checksum(&app.join("Contents/MacOS/vmux"))?
            == self.checksum(&app.join("Contents/MacOS/vmux_desktop"))?
        {
            return Err("CLI and desktop executables collide".into());
        }
        Ok(())
    }

    pub fn package(&mut self, profile: &str) -> Result<Vec<PathBuf>> {
        let fast = self.fast_package(profile)?;
        if !cfg!(target_os = "macos") {
            return Err("Desktop packaging requires macOS".into());
        }
        let product = self.product()?;
        for (&key, value) in product.values() {
            self.environment.set(key, value.as_str());
        }
        let sha = self.text(
            "git",
            &[
                OsStr::new("rev-parse"),
                OsStr::new("--short=7"),
                OsStr::new("HEAD"),
            ],
        )?;
        let sha = sha.trim();
        if sha.len() != 7 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Invalid Git revision".into());
        }
        let name = if profile == "local" {
            format!("{} ({sha})", product.value("VMUX_HOST_NAME"))
        } else {
            product.value("VMUX_HOST_NAME").to_owned()
        };
        let identifier = if profile == "local" {
            format!("{}.{sha}", product.value("VMUX_HOST_BUNDLE_ID"))
        } else {
            product.value("VMUX_HOST_BUNDLE_ID").to_owned()
        };
        if (profile == "local" || fast) && !self.environment.present("SKIP_NOTARIZE") {
            self.environment.set("SKIP_NOTARIZE", "1");
        }
        self.environment.set("VMUX_GIT_HASH", sha);
        self.environment.set("VMUX_BUNDLE_ID", &identifier);
        self.environment.set("VMUX_BUILD_PROFILE", profile);
        let release = self.checkout.directory("release");
        self.environment
            .set("VMUX_CARGO_RELEASE_DIR", release.as_os_str());
        fs::create_dir_all(&release)?;
        let staging = Temporary::create(&release, ".vmux-package")?;
        let app = staging.path().join(format!("{name}.app"));
        self.environment.set("VMUX_APP_BUNDLE", app.as_os_str());
        self.environment.set("APP_BUNDLE", app.as_os_str());
        self.cargo_environment()?;
        let mut keychain = Keychain::capture(&self.checkout.root, &self.environment)?;
        if self.environment.present("APPLE_CERTIFICATE")
            || self.environment.present("APPLE_CERTIFICATE_PASSWORD")
        {
            let path = keychain.install(&self.checkout.root, &self.environment)?;
            self.environment.set("CODESIGN_KEYCHAIN", path.as_os_str());
        }
        let manifest: toml::Table = fs::read_to_string(
            self.checkout
                .root
                .join("crates/app/vmux_desktop/Cargo.toml"),
        )?
        .parse()?;
        let metadata = manifest
            .get("package")
            .and_then(|value| value.get("metadata"))
            .and_then(|value| value.get("packager"))
            .ok_or("Missing packaging configuration")?;
        let mut config = serde_json::to_value(metadata)?;
        let workspace: toml::Table =
            fs::read_to_string(self.checkout.root.join("Cargo.toml"))?.parse()?;
        let version = workspace
            .get("workspace")
            .and_then(|value| value.get("package"))
            .and_then(|value| value.get("version"))
            .and_then(toml::Value::as_str)
            .ok_or("Missing workspace version")?;
        config["name"] = "vmux_desktop".into();
        config["product-name"] = name.into();
        config["identifier"] = identifier.into();
        config["version"] = version.into();
        config["out-dir"] = staging.path().to_string_lossy().into_owned().into();
        config["binaries-dir"] = release.to_string_lossy().into_owned().into();
        config["icons"] = serde_json::json!([product.value("VMUX_HOST_ICON")]);
        config["dmg"]["background"] = product.value("VMUX_HOST_DMG_BACKGROUND").into();
        config["macos"]["entitlements"] = product.value("VMUX_HOST_ENTITLEMENTS").into();
        config["macos"]["info-plist-path"] = product.value("VMUX_HOST_USAGE_DESCRIPTIONS").into();
        for binary in config["binaries"]
            .as_array_mut()
            .ok_or("Missing packaging binaries")?
        {
            if binary["path"] == "Vmux Service" {
                binary["path"] = product.value("VMUX_HOST_SERVICE_NAME").into();
            }
        }
        config["before-packaging-command"] = "\"$VMUX_DX_BIN\" internal package-binaries".into();
        config["before-each-package-command"] = "\"$VMUX_DX_BIN\" internal before-package".into();
        let mut args = vec![
            "packager".into(),
            "--config".into(),
            serde_json::to_string(&config)?.into(),
        ];
        if let Some(target) = self.environment.get("CARGO_BUILD_TARGET") {
            args.extend(["--target".into(), target.to_owned()]);
        }
        if profile == "local" || fast {
            args.extend(["--formats".into(), "app".into()]);
        }
        self.cargo(&args)?;
        if !app.is_dir() {
            return Err("Packaging produced no app".into());
        }
        let title = self
            .checkout
            .title(profile, product.value("VMUX_HOST_NAME"))?;
        let info_path = app.join("Contents/Info.plist");
        let mut info = Value::from_file(&info_path)?;
        let dictionary = info
            .as_dictionary_mut()
            .ok_or("Invalid application plist")?;
        for key in ["CFBundleName", "CFBundleDisplayName"] {
            dictionary.insert(key.into(), Value::String(title.clone()));
        }
        info.to_file_xml(&info_path)?;
        if profile == "local" || fast {
            self.inject_cef(&app)?;
            self.embed_agent(&app)?;
            self.sign_package(&app)?;
        }
        self.verify_bundle(&app)?;
        self.run_command(
            "codesign",
            &[
                OsStr::new("--verify"),
                OsStr::new("--deep"),
                OsStr::new("--strict"),
                app.as_os_str(),
            ],
        )?;
        let temporary = Temporary::create(&std::env::temp_dir(), "vmux-signature-check")?;
        let mut extract = self.command("codesign");
        extract
            .current_dir(temporary.path())
            .args(["--display", "--extract-certificates"])
            .arg(app.join("Contents/MacOS/vmux"));
        Execution::new(extract).output(None)?;
        if fs::metadata(temporary.path().join("codesign0"))?.len() == 0 {
            return Err("Packaged CLI has no signing certificate".into());
        }
        let mut outputs = Vec::new();
        for entry in fs::read_dir(staging.path())? {
            let entry = entry?;
            if entry.file_name() != ".cargo-packager" {
                outputs.push(entry.path());
            }
        }
        let mut publication = Publication::new(&staging);
        let mut artifacts = Vec::new();
        let processes = crate::processes::Processes::capture()?;
        for source in outputs {
            crate::signals::Signals::check()?;
            let destination = release.join(source.file_name().ok_or("Missing artifact filename")?);
            if processes.uses(&destination) {
                return Err("An artifact being replaced is running; quit that build first".into());
            }
            publication.publish(&source, &destination)?;
            artifacts.push(destination);
        }
        crate::signals::Signals::check()?;
        publication.commit();
        println!("Package ready: {}", release.display());
        Ok(artifacts)
    }

    fn fast_package(&self, profile: &str) -> Result<bool> {
        if self.environment.get("VMUX_CI_FAST_PACKAGE") != Some(OsStr::new("1")) {
            return Ok(false);
        }
        let event = self.environment.get("GITHUB_EVENT_NAME");
        let branch = self
            .environment
            .get("GITHUB_REF")
            .and_then(OsStr::to_str)
            .and_then(|reference| reference.strip_prefix("refs/heads/"));
        let feature = event == Some(OsStr::new("push"))
            && self.environment.get("VMUX_CI_PREFLIGHT") == Some(OsStr::new("true"))
            && branch.is_some_and(|branch| {
                !branch.is_empty()
                    && !matches!(branch, "main" | "master")
                    && !branch.starts_with("release/")
            });
        if profile != "release"
            || self.environment.get("IS_RELEASE_PR") == Some(OsStr::new("1"))
            || !(event == Some(OsStr::new("pull_request")) || feature)
        {
            return Err("Fast package mode requires a non-release pull request or verified feature preflight".into());
        }
        Ok(true)
    }

    pub fn verify_web(&self, dist: &Path) -> Result<()> {
        for name in [
            "assets/index.css",
            "assets/theme.css",
            "assets/favicons/tools.svg",
            "assets/tool-logos/acp.png",
            "assets/tool-logos/homebrew.svg",
            "assets/tool-logos/mcp.png",
            "assets/tool-logos/npm.svg",
        ] {
            if !dist.join(name).is_file() {
                return Err(format!("Incomplete stylesheet bundle: {name}").into());
            }
        }
        let source = fs::read_to_string(dist.join(".bundle-stamp"))?;
        let mut expected = BTreeSet::new();
        for line in source.lines() {
            let (hash, name) = line.split_once("  ").ok_or("Invalid stylesheet stamp")?;
            if hash.len() != 64
                || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                || name.is_empty()
                || Path::new(name).is_absolute()
                || name.split('/').any(|part| matches!(part, "" | "." | ".."))
                || !expected.insert(PathBuf::from(name))
            {
                return Err("Invalid or unsafe stylesheet stamp entry".into());
            }
            if self.checksum(&dist.join(name))? != hash {
                return Err(format!("Stylesheet checksum mismatch: {name}").into());
            }
        }
        let mut files = Vec::new();
        Self::files(dist, &mut files)?;
        let mut actual = BTreeSet::new();
        for path in files {
            if path.file_name() == Some(OsStr::new(".bundle-stamp")) {
                continue;
            }
            actual.insert(path.strip_prefix(dist)?.to_path_buf());
        }
        if actual != expected {
            return Err("Stylesheet stamp does not cover all bundle files".into());
        }
        Ok(())
    }

    pub fn package_binaries(&mut self) -> Result<()> {
        let mut desktop = vec![
            "build".into(),
            "-p".into(),
            "vmux_desktop".into(),
            "--release".into(),
        ];
        if let Some(features) = self.environment.get("VMUX_DESKTOP_FEATURE_SET") {
            desktop.extend([
                "--no-default-features".into(),
                "--features".into(),
                format!("package,{}", features.to_string_lossy()).into(),
            ]);
        } else {
            desktop.extend(["--features".into(), "vmux_desktop/package".into()]);
        }
        self.cargo(&["build", "-p", "vmux_cli", "--release"].map(Into::into))?;
        self.cargo(&desktop)?;
        for args in [
            vec![
                "build",
                "-p",
                "vmux_app",
                "--bin",
                "vmux_service",
                "--release",
                "--features",
                "service-daemon",
            ],
            vec![
                "build",
                "-p",
                "bevy_cef_debug_render_process",
                "--profile",
                "cef-helper",
            ],
        ] {
            self.cargo(&args.into_iter().map(Into::into).collect::<Vec<_>>())?;
        }
        self.verify_web(&self.checkout.root.join("crates/util/vmux_ui/dist"))?;
        let release = self.checkout.directory("release");
        self.copy(
            &self
                .checkout
                .directory("cef-helper")
                .join("bevy_cef_debug_render_process"),
            &release.join("bevy_cef_debug_render_process"),
        )?;
        self.copy(
            &release.join("vmux_service"),
            &release.join(self.product()?.value("VMUX_HOST_SERVICE_NAME")),
        )?;
        Ok(())
    }

    pub fn before_package(&self) -> Result<()> {
        if self.environment.get("CARGO_PACKAGER_FORMAT") != Some(OsStr::new("dmg")) {
            return Ok(());
        }
        let app = PathBuf::from(self.environment.required("VMUX_APP_BUNDLE")?);
        self.inject_cef(&app)?;
        self.embed_agent(&app)?;
        self.sign_package(&app)
    }

    fn inject_cef(&self, app: &Path) -> Result<()> {
        let framework = self.framework()?;
        self.validate_cef(&framework)?;
        let source = self.checkout.root.join("crates/util/vmux_ui/dist");
        self.verify_web(&source)?;
        let resources = app.join("Contents/Resources");
        let webview = resources.join("webview-apps/_shared");
        if webview.parent().unwrap().exists() {
            fs::remove_dir_all(webview.parent().unwrap())?;
        }
        fs::create_dir_all(&webview)?;
        self.run_command(
            "cp",
            &[
                OsStr::new("-R"),
                source.join(".").as_os_str(),
                webview.as_os_str(),
            ],
        )?;
        self.verify_web(&webview)?;
        let embedded = app.join("Contents/Frameworks/Chromium Embedded Framework.framework");
        if !embedded.is_dir() {
            let mut command =
                self.command(self.get("BEVY_CEF_BUNDLE_APP_BIN", "bevy_cef_bundle_app"));
            command
                .arg("--app")
                .arg(app)
                .arg("--bundle-id-base")
                .arg(self.get("VMUX_BUNDLE_ID", "ai.vmux.desktop"))
                .args(["--bin-name", "vmux_desktop", "--cef-framework"])
                .arg(&framework)
                .arg("--helper-bin")
                .arg(
                    self.checkout
                        .directory("release")
                        .join("bevy_cef_debug_render_process"),
                )
                .arg("--no-sign");
            Execution::new(command).run()?;
        }
        self.validate_cef(&embedded)?;
        let locales = embedded.join("Resources");
        for entry in fs::read_dir(&locales)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_string_lossy().ends_with(".lproj")
                && !matches!(
                    name.to_str(),
                    Some("en.lproj" | "en_GB.lproj" | "Base.lproj")
                )
            {
                fs::remove_dir_all(entry.path())?;
            }
        }
        if !locales.join("en.lproj").is_dir() {
            return Err("CEF English locale is missing".into());
        }
        let product = self.product()?;
        let icon = PathBuf::from(product.value("VMUX_HOST_ICON"));
        self.copy(&icon, &resources.join("Vmux.icns"))?;
        self.copy(
            Path::new(product.value("VMUX_HOST_TRAY_ICON")),
            &resources.join("tray.png"),
        )?;
        self.copy(
            &self
                .checkout
                .root
                .join("packaging/macos/crash_reporter.cfg"),
            &resources.join("crash_reporter.cfg"),
        )?;
        for entry in fs::read_dir(app.join("Contents/Frameworks"))? {
            let entry = entry?;
            if entry.path().extension() != Some(OsStr::new("app")) {
                continue;
            }
            let contents = entry.path().join("Contents");
            self.copy(&icon, &contents.join("Resources/Vmux.icns"))?;
            self.plist(&contents.join("Info.plist"), "Set :CFBundleIconFile Vmux")
                .or_else(|_| {
                    self.plist(
                        &contents.join("Info.plist"),
                        "Add :CFBundleIconFile string Vmux",
                    )
                })?;
        }
        Ok(())
    }

    fn embed_agent(&self, app: &Path) -> Result<()> {
        let product = self.product()?;
        let profile = self
            .environment
            .required("VMUX_BUILD_PROFILE")?
            .to_str()
            .ok_or("Invalid build profile")?;
        let sha = self
            .environment
            .get("VMUX_GIT_HASH")
            .unwrap_or(OsStr::new(""))
            .to_string_lossy();
        let label = if profile == "local" {
            format!("{}.{sha}", product.value("VMUX_HOST_SERVICE_ID"))
        } else if profile != "release" {
            format!("{}.{profile}", product.value("VMUX_HOST_SERVICE_ID"))
        } else {
            product.value("VMUX_HOST_SERVICE_ID").to_owned()
        };
        let service = app
            .join("Contents/Library/LoginItems")
            .join(product.value("VMUX_HOST_SERVICE_BUNDLE"))
            .join("Contents");
        let executable = service
            .join("MacOS")
            .join(product.value("VMUX_HOST_SERVICE_NAME"));
        let source = app
            .join("Contents/MacOS")
            .join(product.value("VMUX_HOST_SERVICE_NAME"));
        if source.is_file() {
            self.copy(&source, &executable)?;
            fs::remove_file(source)?;
        } else if !executable.is_file() {
            return Err("Missing packaged service executable".into());
        }
        self.copy(
            Path::new(product.value("VMUX_HOST_ICON")),
            &service.join("Resources/Vmux.icns"),
        )?;
        let plist = service.join("Info.plist");
        let mut info = plist::Dictionary::new();
        for (key, value) in [
            ("CFBundleDevelopmentRegion", "en"),
            (
                "CFBundleExecutable",
                product.value("VMUX_HOST_SERVICE_NAME"),
            ),
            ("CFBundleName", product.value("VMUX_HOST_SERVICE_NAME")),
            (
                "CFBundleDisplayName",
                product.value("VMUX_HOST_SERVICE_NAME"),
            ),
            ("CFBundleIdentifier", label.as_str()),
            ("CFBundleInfoDictionaryVersion", "6.0"),
            ("CFBundlePackageType", "APPL"),
            ("CFBundleIconFile", "Vmux"),
        ] {
            info.insert(key.into(), Value::String(value.into()));
        }
        info.insert("LSUIElement".into(), Value::Boolean(true));
        Value::Dictionary(info).to_file_xml(&plist)?;
        let mut launch = Value::from_file(
            self.checkout
                .root
                .join("packaging/macos/ai.vmux.service.plist"),
        )?;
        let dictionary = launch
            .as_dictionary_mut()
            .ok_or("Invalid service launch plist")?;
        let identifier = self.get("VMUX_BUNDLE_ID", "ai.vmux.desktop");
        dictionary.insert("Label".into(), Value::String(label));
        dictionary.insert(
            "BundleProgram".into(),
            Value::String(format!(
                "Contents/Library/LoginItems/{}/Contents/MacOS/{}",
                product.value("VMUX_HOST_SERVICE_BUNDLE"),
                product.value("VMUX_HOST_SERVICE_NAME")
            )),
        );
        dictionary.insert(
            "AssociatedBundleIdentifiers".into(),
            Value::Array(vec![Value::String(
                identifier.to_string_lossy().into_owned(),
            )]),
        );
        let environment = dictionary
            .get_mut("EnvironmentVariables")
            .and_then(Value::as_dictionary_mut)
            .ok_or("Missing launch environment")?;
        environment.insert("VMUX_BUILD_PROFILE".into(), Value::String(profile.into()));
        let log = Value::String(format!(
            "/tmp/{}-service.log",
            product.value("VMUX_HOST_RUNTIME_STEM")
        ));
        dictionary.insert("StandardOutPath".into(), log.clone());
        dictionary.insert("StandardErrorPath".into(), log);
        let destination = app
            .join("Contents/Library/LaunchAgents")
            .join(product.value("VMUX_HOST_AGENT_PLIST"));
        fs::create_dir_all(destination.parent().unwrap())?;
        launch.to_file_xml(destination)?;
        Ok(())
    }

    fn sign_package(&self, app: &Path) -> Result<()> {
        let product = self.product()?;
        let mut files = Vec::new();
        Self::files(&app.join("Contents/Frameworks"), &mut files)?;
        for path in files {
            if self.text("file", &[path.as_os_str()])?.contains("Mach-O") {
                self.sign_release(&path, None)?;
            }
        }
        let framework = app.join("Contents/Frameworks/Chromium Embedded Framework.framework");
        self.sign_release(&framework, None)?;
        Self::sign_apps(self, &app.join("Contents/Frameworks"))?;
        if app.join("Contents/Library").is_dir() {
            Self::sign_apps(self, &app.join("Contents/Library"))?;
        }
        for entry in fs::read_dir(app.join("Contents/MacOS"))? {
            let entry = entry?;
            if !entry.file_type()?.is_file() || entry.file_name() == "vmux_desktop" {
                continue;
            }
            if !self
                .text("file", &[entry.path().as_os_str()])?
                .contains("Mach-O")
            {
                continue;
            }
            let name = entry.file_name();
            let identifier = if name == "vmux" {
                product.value("VMUX_HOST_CLI_ID").to_owned()
            } else {
                format!(
                    "{}.{}",
                    product.value("VMUX_HOST_NAMESPACE"),
                    name.to_string_lossy()
                )
            };
            self.sign_release(&entry.path(), Some(&identifier))?;
        }
        self.sign_release(app, None)?;
        self.run_command(
            "codesign",
            &[
                OsStr::new("--verify"),
                OsStr::new("--deep"),
                OsStr::new("--strict"),
                app.as_os_str(),
            ],
        )?;
        if self.environment.get("SKIP_NOTARIZE") == Some(OsStr::new("1")) {
            return Ok(());
        }
        let zip = app.with_extension("notarize.zip");
        self.run_command(
            "ditto",
            &[
                OsStr::new("-c"),
                OsStr::new("-k"),
                OsStr::new("--keepParent"),
                app.as_os_str(),
                zip.as_os_str(),
            ],
        )?;
        let mut submit = self.command("xcrun");
        submit
            .args(["notarytool", "submit"])
            .arg(&zip)
            .arg("--apple-id")
            .arg(self.environment.required("APPLE_ID")?)
            .arg("--password")
            .arg(self.environment.required("APPLE_APP_PASSWORD")?)
            .arg("--team-id")
            .arg(self.environment.required("APPLE_TEAM_ID")?)
            .args(["--wait", "--output-format", "json"]);
        let output = Execution::new(submit).output(None)?;
        let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        if receipt.get("status").and_then(serde_json::Value::as_str) != Some("Accepted") {
            return Err("Notarization did not accept the app".into());
        }
        self.run_command(
            "xcrun",
            &[OsStr::new("stapler"), OsStr::new("staple"), app.as_os_str()],
        )?;
        self.run_command(
            "spctl",
            &[
                OsStr::new("--assess"),
                OsStr::new("--type"),
                OsStr::new("execute"),
                app.as_os_str(),
            ],
        )?;
        fs::remove_file(zip)?;
        Ok(())
    }

    fn sign_apps(&self, directory: &Path) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            self.sign_apps(&entry.path())?;
            if entry.path().extension() == Some(OsStr::new("app")) {
                self.sign_release(&entry.path(), None)?;
            }
        }
        Ok(())
    }

    fn sign_release(&self, path: &Path, identifier: Option<&str>) -> Result<()> {
        let product = self.product()?;
        let mut command = self.command("codesign");
        command
            .args(["--force", "--verify", "--sign"])
            .arg(self.environment.required("APPLE_SIGNING_IDENTITY")?)
            .args(["--options", "runtime", "--entitlements"])
            .arg(product.value("VMUX_HOST_ENTITLEMENTS"));
        if let Some(keychain) = self.environment.get("CODESIGN_KEYCHAIN") {
            command.arg("--keychain").arg(keychain);
        }
        if let Some(identifier) = identifier {
            command.arg("--identifier").arg(identifier);
        }
        command.arg(path);
        Execution::new(command).run()
    }
}

pub(super) struct Publication {
    changes: Vec<(PathBuf, Option<PathBuf>)>,
    retained: std::sync::Arc<std::sync::atomic::AtomicBool>,
    staging: PathBuf,
}

impl Publication {
    pub(super) fn new(staging: &Temporary) -> Self {
        Self {
            changes: Vec::new(),
            retained: staging.retention(),
            staging: staging.path().to_path_buf(),
        }
    }

    pub(super) fn publish(&mut self, source: &Path, destination: &Path) -> Result<()> {
        let backup = if destination.exists() {
            if fs::symlink_metadata(destination)?.is_symlink() {
                return Err("Refusing to replace a symlink artifact".into());
            }
            let backup = source.with_file_name(format!(".previous-{}", self.changes.len()));
            fs::rename(destination, &backup)?;
            Some(backup)
        } else {
            None
        };
        self.changes.push((destination.into(), backup));
        fs::rename(source, destination)?;
        Ok(())
    }

    pub(super) fn commit(&mut self) {
        self.changes.clear();
    }
}

impl Drop for Publication {
    fn drop(&mut self) {
        for (destination, backup) in self.changes.iter().rev() {
            let removed = if destination.is_dir() {
                fs::remove_dir_all(destination)
            } else {
                fs::remove_file(destination)
            };
            if let Err(error) = removed
                && error.kind() != std::io::ErrorKind::NotFound
            {
                self.retained
                    .store(true, std::sync::atomic::Ordering::Release);
                eprintln!(
                    "Artifact rollback failed at {}: {error}; retained {}",
                    destination.display(),
                    self.staging.display()
                );
            }
            if let Some(backup) = backup
                && let Err(error) = fs::rename(backup, destination)
            {
                self.retained
                    .store(true, std::sync::atomic::Ordering::Release);
                eprintln!(
                    "Artifact restore failed at {}: {error}; retained {}",
                    backup.display(),
                    self.staging.display()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::Environment;
    use crate::repository::Checkout;

    #[test]
    fn fast_packages_refuse_release_local_and_unverified_inputs_before_filesystem_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let checkout = Checkout {
            root: directory.path().join("checkout"),
            target: directory.path().join("target"),
            build_target: None,
        };
        for (profile, event, reference, preflight, release) in [
            ("local", "pull_request", "", "", "0"),
            ("release", "pull_request", "", "", "1"),
            ("release", "push", "refs/heads/feature", "", "0"),
            ("release", "push", "refs/heads/main", "true", "0"),
            ("release", "push", "refs/heads/master", "true", "0"),
            ("release", "push", "refs/heads/release/1.0", "true", "0"),
            ("release", "push", "refs/tags/v1.0", "true", "0"),
            ("release", "workflow_dispatch", "", "", "0"),
        ] {
            let mut environment = Environment::current();
            environment.set("VMUX_CI_FAST_PACKAGE", "1");
            environment.set("GITHUB_EVENT_NAME", event);
            environment.set("GITHUB_REF", reference);
            environment.set("VMUX_CI_PREFLIGHT", preflight);
            environment.set("IS_RELEASE_PR", release);
            let error = Native::new(&checkout, environment)
                .package(profile)
                .unwrap_err();
            assert!(error.to_string().contains("Fast package mode requires"));
            assert!(!checkout.root.exists());
            assert!(!checkout.target.exists());
        }
    }
}
