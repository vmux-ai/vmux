use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use super::Native;
use super::keychain::Keychain;
use crate::Result;
use crate::execution::Execution;
use crate::temporary::Temporary;

impl Native {
    pub fn ios_release(&mut self) -> Result<()> {
        let product = self.product()?;
        let team = self
            .environment
            .required("APPLE_TEAM_ID")?
            .to_string_lossy()
            .into_owned();
        if !team.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err("Invalid Apple team identifier".into());
        }
        let identity = self
            .environment
            .required("APPLE_IOS_SIGNING_IDENTITY")?
            .to_owned();
        let provisioning = self
            .environment
            .required("APPLE_IOS_PROVISIONING_PROFILE")?
            .to_owned();
        self.environment.set("APPLE_SIGNING_IDENTITY", identity);
        self.environment.set("VMUX_BUILD_PROFILE", "release");
        let mut keychain = Keychain::capture(&self.checkout.root, &self.environment)?;
        if self.environment.present("APPLE_IOS_CERTIFICATE")
            || self.environment.present("APPLE_IOS_CERTIFICATE_PASSWORD")
        {
            self.environment.set(
                "APPLE_CERTIFICATE",
                self.environment
                    .required("APPLE_IOS_CERTIFICATE")?
                    .to_owned(),
            );
            self.environment.set(
                "APPLE_CERTIFICATE_PASSWORD",
                self.environment
                    .required("APPLE_IOS_CERTIFICATE_PASSWORD")?
                    .to_owned(),
            );
            let path = keychain.install(&self.checkout.root, &self.environment)?;
            self.environment.set("CODESIGN_KEYCHAIN", path.as_os_str());
        }
        let temporary = Temporary::create(&std::env::temp_dir(), "vmux-ios-release")?;
        let profile = temporary.path().join("profile.mobileprovision");
        let mut decode = self.command("base64");
        decode.arg("--decode");
        let bytes = Execution::new(decode)
            .output(Some(provisioning.as_encoded_bytes()))?
            .stdout;
        fs::write(&profile, bytes)?;
        let profile_xml = self.output(
            "security",
            &[
                OsStr::new("cms"),
                OsStr::new("-D"),
                OsStr::new("-i"),
                profile.as_os_str(),
            ],
        )?;
        let mut inspect = self.command("plutil");
        inspect.args([
            "-extract",
            "Entitlements.application-identifier",
            "raw",
            "-",
        ]);
        let identifier = Execution::new(inspect).output(Some(&profile_xml))?.stdout;
        if std::str::from_utf8(&identifier)?.trim()
            != format!("{team}.{}", product.value("VMUX_HOST_MOBILE_ID"))
        {
            return Err("Provisioning profile does not match the selected application".into());
        }
        self.tool(
            OsStr::new("dx"),
            &[
                "build",
                "--platform",
                "ios",
                "--release",
                "-p",
                "vmux_mobile",
                "--target",
                "aarch64-apple-ios",
            ]
            .map(Into::into),
        )?;
        self.resources_ios()?;
        let app = self.ios_bundle();
        self.copy(&profile, &app.join("embedded.mobileprovision"))?;
        let entitlements = temporary.path().join("host.entitlements");
        let mut projected =
            plist::Value::from_file(product.value("VMUX_HOST_MOBILE_ENTITLEMENTS"))?;
        Self::project_mobile_entitlements(
            &mut projected,
            &team,
            product.value("VMUX_HOST_MOBILE_ID"),
        );
        let values = projected
            .as_dictionary_mut()
            .ok_or("Mobile entitlements must be a dictionary")?;
        values.insert(
            "application-identifier".into(),
            format!("{team}.{}", product.value("VMUX_HOST_MOBILE_ID")).into(),
        );
        values.insert(
            "com.apple.developer.team-identifier".into(),
            team.clone().into(),
        );
        projected.to_file_xml(&entitlements)?;
        let mut sign = self.command("codesign");
        sign.args([
            "--force",
            "--timestamp",
            "--options",
            "runtime",
            "--entitlements",
        ])
        .arg(&entitlements)
        .arg("--sign")
        .arg(self.environment.required("APPLE_IOS_SIGNING_IDENTITY")?);
        if let Some(path) = self.environment.get("CODESIGN_KEYCHAIN") {
            sign.arg("--keychain").arg(path);
        }
        sign.arg(&app);
        Execution::new(sign).run()?;
        self.run_command(
            "codesign",
            &[
                OsStr::new("--verify"),
                OsStr::new("--deep"),
                OsStr::new("--strict"),
                app.as_os_str(),
            ],
        )?;
        self.verify_ios(true)?;
        let manifest: toml::Table =
            fs::read_to_string(self.checkout.root.join("Cargo.toml"))?.parse()?;
        let version = manifest
            .get("workspace")
            .and_then(|value| value.get("package"))
            .and_then(|value| value.get("version"))
            .and_then(toml::Value::as_str)
            .ok_or("Missing workspace version")?;
        let out = app.parent().unwrap().join("ipa");
        fs::create_dir_all(&out)?;
        let ipa = out.join(format!(
            "{}_{version}_aarch64.ipa",
            product.value("VMUX_HOST_NAME")
        ));
        let payload = temporary.path().join("Payload");
        fs::create_dir(&payload)?;
        self.run_command(
            "cp",
            &[OsStr::new("-R"), app.as_os_str(), payload.as_os_str()],
        )?;
        let mut archive = self.command("ditto");
        archive
            .current_dir(temporary.path())
            .args(["-c", "-k", "--sequesterRsrc", "--keepParent", "Payload"])
            .arg(&ipa);
        Execution::new(archive).run()?;
        println!("IPA ready: {}", ipa.display());
        Ok(())
    }

    fn project_mobile_entitlements(value: &mut plist::Value, team: &str, identifier: &str) {
        match value {
            plist::Value::String(value) => {
                for (token, replacement) in [
                    ("{{TEAM_ID}}", team.to_string()),
                    ("{{BUNDLE_ID}}", identifier.to_string()),
                    ("$(AppIdentifierPrefix)", format!("{team}.")),
                    ("$(TeamIdentifierPrefix)", format!("{team}.")),
                    ("$(PRODUCT_BUNDLE_IDENTIFIER)", identifier.to_string()),
                ] {
                    *value = value.replace(token, &replacement);
                }
            }
            plist::Value::Array(values) => {
                for value in values {
                    Self::project_mobile_entitlements(value, team, identifier);
                }
            }
            plist::Value::Dictionary(values) => {
                for (_, value) in values.iter_mut() {
                    Self::project_mobile_entitlements(value, team, identifier);
                }
            }
            _ => {}
        }
    }

    pub fn ios_upload(&self) -> Result<()> {
        let product = self.product()?;
        let directory = self.checkout.target.join("dx/vmux_mobile/release/ios/ipa");
        let ipa = directory.join(format!(
            "{}_{}_aarch64.ipa",
            product.value("VMUX_HOST_NAME"),
            self.workspace_version()?
        ));
        if !ipa.is_file() {
            return Err("No IPA was produced for the current version".into());
        }
        let mut upload = self.command("xcrun");
        upload
            .args(["altool", "--upload-app", "-f"])
            .arg(ipa)
            .args(["-t", "ios", "-u"])
            .arg(self.environment.required("APPLE_ID")?)
            .arg("-p")
            .arg(self.environment.required("APPLE_APP_PASSWORD")?)
            .arg("--team-id")
            .arg(self.environment.required("APPLE_TEAM_ID")?);
        Execution::new(upload).run()
    }

    fn ios_bundle(&self) -> PathBuf {
        if let Some(path) = self.environment.get("VMUX_IOS_APP_BUNDLE") {
            return path.into();
        }
        let profile = if self.environment.get("VMUX_BUILD_PROFILE") == Some(OsStr::new("dev")) {
            "debug"
        } else {
            "release"
        };
        self.checkout
            .target
            .join("dx/vmux_mobile")
            .join(profile)
            .join("ios/VmuxMobile.app")
    }

    pub fn resources_ios(&self) -> Result<()> {
        let product = self.product()?;
        let app = self.ios_bundle();
        if !app.join("Info.plist").is_file() {
            return Err("iOS build produced no bundle Info.plist".into());
        }
        let packaging = self.checkout.root.join("packaging/ios");
        let stage = Temporary::create(&std::env::temp_dir(), "vmux-ios-assets")?;
        let catalog = stage.path().join("Assets.xcassets");
        self.run_command(
            "cp",
            &[
                OsStr::new("-R"),
                packaging.join("Assets.xcassets").as_os_str(),
                catalog.as_os_str(),
            ],
        )?;
        self.copy(
            &PathBuf::from(product.value("VMUX_HOST_MOBILE_ICON")),
            &catalog.join("AppIcon.appiconset/AppIcon-1024.png"),
        )?;
        let mut actool = self.command("xcrun");
        actool
            .arg("actool")
            .arg(&catalog)
            .arg("--compile")
            .arg(stage.path())
            .args([
                "--platform",
                "iphoneos",
                "--target-device",
                "iphone",
                "--minimum-deployment-target",
                "15.0",
                "--app-icon",
                "AppIcon",
                "--output-partial-info-plist",
            ])
            .arg(stage.path().join("partial.plist"))
            .args(["--output-format", "human-readable-text"]);
        Execution::new(actool).run()?;
        let mut ibtool = self.command("xcrun");
        ibtool
            .arg("ibtool")
            .arg(product.value("VMUX_HOST_MOBILE_LAUNCH"))
            .arg("--compile")
            .arg(app.join("LaunchScreen.storyboardc"))
            .args([
                "--target-device",
                "iphone",
                "--minimum-deployment-target",
                "15.0",
                "--output-format",
                "human-readable-text",
            ]);
        Execution::new(ibtool).run()?;
        self.copy(&stage.path().join("Assets.car"), &app.join("Assets.car"))?;
        for entry in fs::read_dir(stage.path())? {
            let entry = entry?;
            let name = entry.file_name();
            let text = name.to_string_lossy();
            if text.starts_with("AppIcon") && text.ends_with(".png") && !text.ends_with("~ipad.png")
            {
                self.copy(&entry.path(), &app.join(name))?;
            }
        }
        self.copy(
            &PathBuf::from(product.value("VMUX_HOST_MOBILE_PRIVACY")),
            &app.join("PrivacyInfo.xcprivacy"),
        )?;
        let manifest: toml::Table =
            fs::read_to_string(self.checkout.root.join("Cargo.toml"))?.parse()?;
        let version = manifest
            .get("workspace")
            .and_then(|value| value.get("package"))
            .and_then(|value| value.get("version"))
            .and_then(toml::Value::as_str)
            .ok_or("Missing workspace version")?;
        let plist = app.join("Info.plist");
        let mut info = plist::Value::from_file(&plist)?;
        let selected = plist::Value::from_file(product.value("VMUX_HOST_MOBILE_PLIST"))?;
        let template = selected
            .as_dictionary()
            .ok_or("Mobile Info.plist must be a dictionary")?;
        let values = info
            .as_dictionary_mut()
            .ok_or("Bundle Info.plist must be a dictionary")?;
        for (key, value) in template {
            values.insert(key.clone(), value.clone());
        }
        for (key, value) in [
            (
                "CFBundleIdentifier",
                product.value("VMUX_HOST_MOBILE_ID").to_string(),
            ),
            ("CFBundleName", product.value("VMUX_HOST_NAME").to_string()),
            (
                "CFBundleDisplayName",
                product.value("VMUX_HOST_NAME").to_string(),
            ),
            ("CFBundleShortVersionString", version.to_string()),
            (
                "CFBundleVersion",
                self.get("VMUX_IOS_BUILD_NUMBER", "1")
                    .to_string_lossy()
                    .into_owned(),
            ),
        ] {
            values.insert(key.into(), value.into());
        }
        if let Some(plist::Value::Array(types)) = values.get_mut("CFBundleURLTypes") {
            for entry in types {
                if let Some(entry) = entry.as_dictionary_mut() {
                    entry.insert(
                        "CFBundleURLName".into(),
                        product.value("VMUX_HOST_MOBILE_ID").into(),
                    );
                }
            }
        }
        info.to_file_xml(&plist)?;
        let sdk = self.text(
            "xcrun",
            &[
                OsStr::new("--sdk"),
                OsStr::new("iphoneos"),
                OsStr::new("--show-sdk-version"),
            ],
        )?;
        let sdk_build = self.text(
            "xcrun",
            &[
                OsStr::new("--sdk"),
                OsStr::new("iphoneos"),
                OsStr::new("--show-sdk-build-version"),
            ],
        )?;
        let xcode = self.text("xcodebuild", &[OsStr::new("-version")])?;
        let xcode_version = xcode
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .ok_or("Missing Xcode version")?;
        let mut parts = xcode_version.split('.');
        let major = parts.next().ok_or("Missing Xcode major version")?;
        let minor = parts.next().unwrap_or("0");
        let xcode_build = xcode
            .lines()
            .last()
            .and_then(|line| line.split_whitespace().last())
            .ok_or("Missing Xcode build")?;
        let system_build = self.text("sw_vers", &[OsStr::new("-buildVersion")])?;
        for (key, value) in [
            ("CFBundleIconName", "AppIcon".into()),
            ("MinimumOSVersion", "15.0".into()),
            ("DTPlatformName", "iphoneos".into()),
            ("DTPlatformVersion", sdk.trim().into()),
            ("DTPlatformBuild", sdk_build.trim().into()),
            ("DTSDKName", format!("iphoneos{}", sdk.trim())),
            ("DTSDKBuild", sdk_build.trim().into()),
            ("DTXcode", format!("{major}{minor}0")),
            ("DTXcodeBuild", xcode_build.into()),
            ("BuildMachineOSBuild", system_build.trim().into()),
        ] {
            if self.plist(&plist, &format!("Set :{key} {value}")).is_err() {
                self.plist(&plist, &format!("Add :{key} string {value}"))?;
            }
        }
        Ok(())
    }

    pub fn verify_ios(&self, signed: bool) -> Result<()> {
        let product = self.product()?;
        let app = if signed && !self.environment.present("VMUX_IOS_APP_BUNDLE") {
            self.checkout
                .target
                .join("dx/vmux_mobile/release/ios/VmuxMobile.app")
        } else {
            self.ios_bundle()
        };
        for name in [
            "vmux_mobile",
            "Info.plist",
            "Assets.car",
            "AppIcon60x60@2x.png",
            "LaunchScreen.storyboardc",
            "PrivacyInfo.xcprivacy",
        ] {
            if !app.join(name).exists() {
                return Err(format!("Missing iOS bundle resource: {name}").into());
            }
        }
        if app.join("AppIcon76x76@2x~ipad.png").exists() {
            return Err("iPad icon must not be bundled in the iPhone app".into());
        }
        if signed {
            for name in ["embedded.mobileprovision", "_CodeSignature"] {
                if !app.join(name).exists() {
                    return Err(format!("Missing signed iOS resource: {name}").into());
                }
            }
        }
        let plist = app.join("Info.plist");
        for (key, expected) in [
            ("CFBundleIdentifier", product.value("VMUX_HOST_MOBILE_ID")),
            ("CFBundleName", product.value("VMUX_HOST_NAME")),
            ("CFBundleDisplayName", product.value("VMUX_HOST_NAME")),
            (
                "CFBundleIcons:CFBundlePrimaryIcon:CFBundleIconName",
                "AppIcon",
            ),
            ("CFBundleIconName", "AppIcon"),
            ("ITSAppUsesNonExemptEncryption", "false"),
        ] {
            let value = self.text(
                self.get("PLIST_BUDDY", "/usr/libexec/PlistBuddy"),
                &[
                    OsStr::new("-c"),
                    OsStr::new(&format!("Print :{key}")),
                    plist.as_os_str(),
                ],
            )?;
            if value.trim() != expected {
                return Err(format!("Invalid iOS bundle property: {key}").into());
            }
        }
        for key in [
            "MinimumOSVersion",
            "DTPlatformName",
            "DTPlatformVersion",
            "DTPlatformBuild",
            "DTSDKName",
            "DTSDKBuild",
            "DTXcode",
            "DTXcodeBuild",
            "BuildMachineOSBuild",
        ] {
            let value = self.text(
                self.get("PLIST_BUDDY", "/usr/libexec/PlistBuddy"),
                &[
                    OsStr::new("-c"),
                    OsStr::new(&format!("Print :{key}")),
                    plist.as_os_str(),
                ],
            )?;
            if value.trim().is_empty() {
                return Err(format!("Missing iOS bundle metadata: {key}").into());
            }
        }
        Ok(())
    }
}
