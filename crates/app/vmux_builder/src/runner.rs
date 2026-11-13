use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;
use crate::debug::Debug;
use crate::environment::Environment;
use crate::execution::Execution;
use crate::mac_package::MacPackage;
use crate::policy::Policy;
use crate::signals::Signals;
use crate::storage::Storage;
use crate::target_lock::TargetLock;

pub struct Runner {
    root: PathBuf,
    target: PathBuf,
}

impl Runner {
    pub fn prepare(&self, profile: &str, args: &[OsString]) -> Result<()> {
        if !self.inherited() {
            let executable = std::env::current_exe()?;
            let mut command = Command::new(&executable);
            command
                .arg("--root")
                .arg(&self.root)
                .arg(profile)
                .arg(executable)
                .arg("--root")
                .arg(&self.root)
                .arg(format!("prepare-{profile}"))
                .args(args);
            if profile != "dev" {
                let mut environment = Environment::current();
                environment.load(&self.root)?;
                environment.apply(&mut command);
            }
            return Execution::new(command).run();
        }
        if profile == "dev" {
            Debug::new(self.root.clone(), self.target.clone()).prepare(args)
        } else {
            MacPackage::new(self.root.clone(), self.target.clone()).prepare(profile)
        }
    }

    pub fn inherited(&self) -> bool {
        let owner = std::env::var("VMUX_TARGET_LOCK_OWNER_PID")
            .ok()
            .and_then(|owner| owner.parse::<u32>().ok());
        std::env::var_os("VMUX_BUILD_STORAGE_ROOT").as_deref() == Some(self.root.as_os_str())
            && std::env::var_os("VMUX_BUILD_STORAGE_TARGET").as_deref()
                == Some(self.target.as_os_str())
            && owner.is_some_and(|pid| pid != std::process::id() && Signals::alive(pid))
    }

    pub fn new(root: PathBuf, target: Option<PathBuf>) -> Result<Self> {
        let root = root.canonicalize()?;
        let setting = match target {
            Some(path) => path,
            None => match std::env::var_os("CARGO_TARGET_DIR") {
                Some(path) if !path.is_empty() => PathBuf::from(path),
                _ => PathBuf::from("target"),
            },
        };
        let path = if setting.is_absolute() {
            setting
        } else {
            root.join(setting)
        };
        let path = std::path::absolute(path)?;
        let parent = path.parent().ok_or("Missing target parent")?;
        std::fs::create_dir_all(parent)?;
        let target = parent
            .canonicalize()?
            .join(path.file_name().ok_or("Missing target filename")?);
        Ok(Self { root, target })
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn build(&self, profile: &str, command: &[OsString]) -> Result<()> {
        let (program, args) = command.split_first().ok_or("Missing build command")?;
        let storage = Storage::new(
            self.root.clone(),
            self.target.clone(),
            Policy::environment()?,
        )?;
        if !self.target.exists() {
            storage.check()?;
            self.seed()?;
        }
        let _lock = TargetLock::acquire(&self.root, &self.target)?;
        storage.preflight()?;
        Signals::check()?;
        let mut child = Command::new(program);
        child
            .args(args)
            .current_dir(&self.root)
            .env("VMUX_TARGET_LOCK_TARGET", &self.target)
            .env("VMUX_TARGET_LOCK_OWNER_PID", std::process::id().to_string())
            .env("VMUX_BUILD_STORAGE_TARGET", &self.target)
            .env("VMUX_BUILD_STORAGE_ROOT", &self.root)
            .env("CARGO_TARGET_DIR", &self.target);
        Execution::new(child).run()?;
        Signals::check()?;
        if let Err(error) = storage.finish(profile) {
            Signals::check()?;
            eprintln!("Build storage: cleanup skipped after successful build: {error}");
        }
        Signals::check()
    }

    pub fn clean(&self) -> Result<()> {
        let _lock = TargetLock::acquire(&self.root, &self.target)?;
        let storage = Storage::new(
            self.root.clone(),
            self.target.clone(),
            Policy::environment()?,
        )?;
        let apply = std::env::var("VMUX_BUILD_STORAGE_APPLY").as_deref() == Ok("1");
        storage.prune_caches(&crate::processes::Processes::capture()?, apply)?;
        storage.report()
    }

    pub fn open(&self, profile: &str) -> Result<()> {
        let mut environment = Environment::current();
        environment.load(&self.root)?;
        let product = environment.product(&self.root)?;
        let product_name = product.value("VMUX_HOST_NAME");
        let mut release = self.target.clone();
        if let Some(target) = std::env::var_os("CARGO_BUILD_TARGET")
            && !target.is_empty()
        {
            release.push(target);
        }
        release.push("release");
        let name = if profile == "local" {
            let output = Command::new("git")
                .arg("-C")
                .arg(&self.root)
                .args(["rev-parse", "--short=7", "HEAD"])
                .output()?;
            if !output.status.success() {
                return Err("Cannot identify local bundle".into());
            }
            format!(
                "{product_name} ({}).app",
                String::from_utf8(output.stdout)?.trim()
            )
        } else {
            format!("{product_name}.app")
        };
        let mut command = Command::new("open");
        command.arg(release.join(name)).current_dir(&self.root);
        Execution::new(command).run()
    }

    fn seed(&self) -> Result<()> {
        if std::env::var("CI").as_deref() == Ok("true") {
            return Ok(());
        }
        let key = Command::new(self.root.join("scripts/target-seed-key.sh"))
            .current_dir(&self.root)
            .output()?;
        if !key.status.success() {
            return Err("Cannot identify a compatible target seed".into());
        }
        let key = String::from_utf8(key.stdout)?;
        let mut command = Command::new(self.root.join("scripts/seed-worktree-target.sh"));
        command
            .arg("--if-needed")
            .arg(&self.root)
            .current_dir(&self.root)
            .env("VMUX_TARGET_SEED_KEY", key.trim())
            .env("CARGO_TARGET_DIR", &self.target);
        Execution::new(command).run()
    }
}
