use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use crate::Result;
use crate::environment::Environment;
use crate::execution::Execution;

pub struct Debug {
    root: PathBuf,
    target: PathBuf,
    environment: Environment,
}

impl Debug {
    pub fn new(root: PathBuf, target: PathBuf) -> Self {
        let mut environment = Environment::current();
        if !environment.present("CARGO_INCREMENTAL") {
            environment.set("CARGO_INCREMENTAL", "1");
        }
        environment.set("VMUX_BUILD_PROFILE", "dev");
        Self {
            root,
            target,
            environment,
        }
    }

    pub fn prepare(&mut self, features: &[OsString]) -> Result<()> {
        self.environment.load(&self.root)?;
        self.environment.set("VMUX_BUILD_PROFILE", "dev");
        let debug = self.directory("debug");
        let stamp = debug.join(".vmux-dev-ready");
        fs::create_dir_all(&debug)?;
        if stamp.exists() {
            fs::remove_file(&stamp)?;
        }
        let product = self.environment.product(&self.root)?;
        let bundle = format!("{} Dev.app", product.value("VMUX_HOST_NAME"));
        let alias = self.directory("release").join(&bundle);
        if fs::symlink_metadata(&alias).is_ok_and(|metadata| metadata.is_symlink()) {
            fs::remove_file(&alias)?;
        }
        self.cargo(&["build", "-p", "vmux_cli"], &[])?;
        self.cargo(
            &[
                "build",
                "-p",
                "vmux_app",
                "--bin",
                "vmux_service",
                "--features",
                "service-daemon",
            ],
            &[],
        )?;
        let mut desktop = vec!["build", "-p", "vmux_desktop"];
        if cfg!(target_os = "macos") {
            desktop = vec!["rustc", "-p", "vmux_desktop", "--bin", "vmux_desktop"];
        }
        let mut args = features.to_vec();
        if cfg!(target_os = "macos") {
            args.extend(["--", "-C", "link-arg=-Wl,-headerpad,1000"].map(OsString::from));
        }
        self.cargo(&desktop, &args)?;
        if cfg!(target_os = "macos") {
            self.sign(&debug)?;
        }
        let mut hashes = format!("{}\n", self.root.display());
        for binary in ["vmux", "vmux_service", "vmux_desktop"] {
            let path = debug.join(binary);
            if fs::metadata(&path)?.permissions().mode() & 0o111 == 0 {
                return Err(format!("Missing executable: {binary}").into());
            }
            hashes.push_str(&self.checksum(&path)?);
        }
        if cfg!(target_os = "macos") {
            hashes.push_str(
                &self.checksum(&debug.join(&bundle).join("Contents/MacOS/vmux_desktop"))?,
            );
            fs::create_dir_all(alias.parent().ok_or("Missing release directory")?)?;
            if alias.exists() {
                return Err("Cannot publish debug app over an existing bundle".into());
            }
            symlink(debug.join(&bundle), &alias)?;
        }
        let temporary = stamp.with_extension("tmp");
        fs::write(&temporary, hashes)?;
        fs::rename(temporary, stamp)?;
        println!(
            "Debug build ready: {}\nRun: make run-dev",
            debug.join("vmux_desktop").display()
        );
        Ok(())
    }

    fn directory(&self, profile: &str) -> PathBuf {
        let mut directory = self.target.clone();
        if let Some(target) = self.environment.get("CARGO_BUILD_TARGET")
            && !target.is_empty()
        {
            directory.push(target);
        }
        directory.join(profile)
    }

    fn cargo(&self, prefix: &[&str], args: &[OsString]) -> Result<()> {
        let mut command = self.environment.command(
            self.root.join("scripts/cargo-with-cef-cache.sh"),
            &self.root,
        );
        command.args(prefix).args(args);
        Execution::new(command).run()
    }

    fn checksum(&self, path: &Path) -> Result<String> {
        let mut command = self.environment.command("shasum", &self.root);
        command.args(["-a", "256"]).arg(path);
        Ok(String::from_utf8(
            Execution::new(command).output(None)?.stdout,
        )?)
    }

    fn sign(&mut self, debug: &Path) -> Result<()> {
        let command = self.environment.command(
            self.root.join("scripts/ensure-local-codesign-identity.sh"),
            &self.root,
        );
        let identity = String::from_utf8(Execution::new(command).output(None)?.stdout)?;
        self.environment
            .set("APPLE_SIGNING_IDENTITY", identity.trim());
        let mut sign = self
            .environment
            .command(self.root.join("scripts/sign-dev-mac.sh"), &self.root);
        let helper = match self.environment.get("CEF_DEBUG_RENDER") {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            _ => PathBuf::from(self.environment.required("HOME")?)
                .join(".local/share/Chromium Embedded Framework.framework/Libraries/bevy_cef_debug_render_process"),
        };
        sign.env("APP_BINARY", debug.join("vmux_desktop"))
            .env("HELPER_BINARY", helper);
        Execution::new(sign).run()?;
        let bundle = self
            .environment
            .command(self.root.join("scripts/bundle-dev.sh"), &self.root);
        Execution::new(bundle).run()
    }
}
