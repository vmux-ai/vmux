use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::Result;
use crate::environment::Environment;
use crate::execution::Execution;
use crate::product::ProductMetadata;
use crate::repository::Checkout;

mod cache;
mod cef;
mod cef_source;
mod conformance;
mod debug;
mod dependencies;
mod integration;
mod ios;
mod keychain;
mod maintenance;
mod package;
mod release;
mod workspace;

pub struct Native {
    checkout: Checkout,
    environment: Environment,
}

impl Native {
    pub fn new(checkout: &Checkout, environment: Environment) -> Self {
        Self {
            checkout: checkout.clone(),
            environment,
        }
    }

    fn product(&self) -> Result<ProductMetadata> {
        self.environment.product(&self.checkout.root)
    }

    fn command(&self, program: impl AsRef<OsStr>) -> Command {
        self.environment.command(program, &self.checkout.root)
    }

    fn run_command(&self, program: impl AsRef<OsStr>, args: &[&OsStr]) -> Result<()> {
        let mut command = self.command(program);
        command.args(args);
        Execution::new(command).run()
    }

    fn output(&self, program: impl AsRef<OsStr>, args: &[&OsStr]) -> Result<Vec<u8>> {
        let name = program.as_ref().to_string_lossy().into_owned();
        let mut command = self.command(program);
        command.args(args);
        match Execution::new(command).output(None) {
            Ok(output) => Ok(output.stdout),
            Err(error) => Err(crate::execution::Failure::from_error(error)
                .context(&name)
                .into()),
        }
    }

    fn text(&self, program: impl AsRef<OsStr>, args: &[&OsStr]) -> Result<String> {
        Ok(String::from_utf8(self.output(program, args)?)?)
    }

    fn get(&self, name: &str, default: impl AsRef<OsStr>) -> OsString {
        self.environment
            .get(name)
            .unwrap_or(default.as_ref())
            .to_owned()
    }

    fn home(&self) -> Result<PathBuf> {
        Ok(PathBuf::from(self.environment.required("HOME")?))
    }

    fn copy(&self, source: &Path, destination: &Path) -> Result<()> {
        if destination.exists() && source.canonicalize()? == destination.canonicalize()? {
            return Ok(());
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
        Ok(())
    }

    fn checksum(&self, path: &Path) -> Result<String> {
        let mut file = fs::File::open(path)?;
        let mut hash = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    pub fn tool(&self, program: &OsStr, args: &[OsString]) -> Result<()> {
        let key = match program.to_str() {
            Some("cargo") => Some("CARGO_BIN"),
            Some("dx") => Some("DX_BIN"),
            _ => None,
        };
        let mut command = self.command(
            key.and_then(|key| self.environment.get(key))
                .unwrap_or(program),
        );
        command.args(args);
        if program == "dx"
            && args
                .windows(2)
                .any(|pair| pair[0] == "--platform" && pair[1] == "web")
        {
            command.current_dir(self.checkout.root.join("website"));
        }
        Execution::new(command).run()
    }

    pub fn cargo(&mut self, args: &[OsString]) -> Result<()> {
        self.cargo_environment()?;
        self.tool(OsStr::new("cargo"), args)?;
        if matches!(
            args.first().and_then(|arg| arg.to_str()),
            Some("bench" | "build" | "check" | "clippy" | "doc" | "rustc" | "test")
        ) && self.environment.get("CI") != Some(OsStr::new("true"))
        {
            self.record_seed()?;
        }
        Ok(())
    }

    pub fn css(&self) -> Result<()> {
        let mut command = self.command(self.get("TAILWINDCSS_BIN", "tailwindcss"));
        command
            .current_dir(self.checkout.root.join("website"))
            .args([
                "-i",
                "tailwind.input.css",
                "-o",
                "public/style.css",
                "--minify",
            ]);
        Execution::new(command).run()
    }

    pub fn flatten_web(&self) -> Result<()> {
        let public = self
            .checkout
            .target
            .join("dx/vmux_website/release/web/public");
        self.copy(&public.join("_home/index.html"), &public.join("index.html"))?;
        fs::remove_dir_all(public.join("_home"))?;
        Ok(())
    }
}
