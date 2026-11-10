use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Result;
use crate::execution::Execution;
use crate::target_lock::TargetLock;

const COMPILER: Option<&str> = option_env!("VMUX_BUILDER_COMPILER");
const SOURCES: &[(&str, &[u8])] = &[
    ("main.rs", include_bytes!("main.rs")),
    ("bootstrap.rs", include_bytes!("bootstrap.rs")),
    ("execution.rs", include_bytes!("execution.rs")),
    ("policy.rs", include_bytes!("policy.rs")),
    ("processes.rs", include_bytes!("processes.rs")),
    ("profile.rs", include_bytes!("profile.rs")),
    ("runner.rs", include_bytes!("runner.rs")),
    ("signals.rs", include_bytes!("signals.rs")),
    ("storage.rs", include_bytes!("storage.rs")),
    ("target_lock.rs", include_bytes!("target_lock.rs")),
];

pub struct Bootstrap {
    root: PathBuf,
    cache: PathBuf,
}

impl Bootstrap {
    pub fn new(root: PathBuf) -> Result<Self> {
        let root = root.canonicalize()?;
        let cache = root.join(".build-tools");
        if fs::symlink_metadata(&cache).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err("Unsafe builder cache".into());
        }
        fs::create_dir_all(&cache)?;
        Ok(Self { root, cache })
    }

    pub fn ensure(&self) -> Result<()> {
        let version = self.compiler_version()?;
        let mut matches = COMPILER == Some(version.as_str());
        for (name, compiled) in SOURCES {
            if fs::read(self.sources().join(name))? != *compiled {
                matches = false;
            }
        }
        if matches {
            return Ok(());
        }
        let lock = TargetLock::acquire(&self.root, &self.cache)?;
        let key = self.key(&version)?;
        let binary = self.cache.join("vmux_builder");
        if !binary.is_file()
            || fs::read_to_string(binary.with_extension("key"))
                .ok()
                .as_deref()
                != Some(&key)
        {
            self.compile(false, &version, &key)?;
        }
        drop(lock);
        let error = Command::new(binary)
            .args(std::env::args_os().skip(1))
            .exec();
        Err(error.into())
    }

    pub fn tests(&self) -> Result<()> {
        let version = self.compiler_version()?;
        let _lock = TargetLock::acquire(&self.root, &self.cache)?;
        let key = self.key(&version)?;
        let binary = self.cache.join("vmux_builder_tests");
        if !binary.is_file()
            || fs::read_to_string(binary.with_extension("key"))
                .ok()
                .as_deref()
                != Some(&key)
        {
            self.compile(true, &version, &key)?;
        }
        Execution::new(Command::new(binary)).run()
    }

    fn sources(&self) -> PathBuf {
        self.root.join("crates/app/vmux_builder/src")
    }

    fn compiler(&self) -> std::ffi::OsString {
        std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into())
    }

    fn compiler_version(&self) -> Result<String> {
        let output = Command::new(self.compiler())
            .arg("-vV")
            .current_dir(&self.root)
            .output()?;
        if !output.status.success() {
            return Err("Cannot identify the host Rust compiler".into());
        }
        Ok(String::from_utf8(output.stdout)?.trim_end().to_owned())
    }

    fn key(&self, version: &str) -> Result<String> {
        let mut hasher = DefaultHasher::new();
        version.hash(&mut hasher);
        for (name, _) in SOURCES {
            name.hash(&mut hasher);
            fs::read(self.sources().join(name))?.hash(&mut hasher);
        }
        Ok(format!("{:016x}", hasher.finish()))
    }

    fn compile(&self, test: bool, version: &str, key: &str) -> Result<()> {
        let temporary = Scratch::new(&self.cache)?;
        let output = temporary.path.join("output");
        let mut command = Command::new(self.compiler());
        command
            .args(["--edition=2024", "-C", "opt-level=1", "-C", "debuginfo=0"])
            .arg(self.sources().join("main.rs"))
            .arg("-o")
            .arg(&output)
            .env("VMUX_BUILDER_COMPILER", version)
            .current_dir(&self.root);
        if test {
            command.arg("--test");
        }
        Execution::new(command).run()?;
        let name = if test {
            "vmux_builder_tests"
        } else {
            "vmux_builder"
        };
        fs::rename(output, self.cache.join(name))?;
        let stamp = temporary.path.join("key");
        fs::write(&stamp, key)?;
        fs::rename(stamp, self.cache.join(name).with_extension("key"))?;
        Ok(())
    }
}

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(parent: &Path) -> Result<Self> {
        let time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = parent.join(format!(".compile-{}-{time}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
