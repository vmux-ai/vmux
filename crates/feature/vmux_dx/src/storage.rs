use std::collections::HashSet;
use std::fs::{self, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;
use crate::policy::{GIB, Policy};
use crate::processes::Processes;
use crate::profile::{Directory, Profile};
use crate::signals::Signals;

#[derive(bevy_ecs::prelude::Component, Clone)]
pub struct Storage {
    root: PathBuf,
    target: PathBuf,
    policy: Policy,
}

impl Storage {
    pub fn new(root: PathBuf, target: PathBuf, policy: Policy) -> Result<Self> {
        let root = root.canonicalize()?;
        let target = std::path::absolute(target)?;
        let canonical = match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_symlink() || !metadata.is_dir() => {
                return Err(
                    "Build storage requires a dedicated, nonsymlink Cargo target directory".into(),
                );
            }
            Ok(_) => target.canonicalize()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => target
                .parent()
                .ok_or("Missing target parent")?
                .canonicalize()?
                .join(target.file_name().ok_or("Missing target filename")?),
            Err(error) => return Err(error.into()),
        };
        if canonical != target || root.starts_with(&target) {
            return Err(
                "Build storage requires a dedicated, nonsymlink Cargo target directory".into(),
            );
        }
        Ok(Self {
            root,
            target,
            policy,
        })
    }

    fn available(&self) -> Result<u64> {
        let mut path = self.target.as_path();
        while !path.exists() {
            path = path.parent().ok_or("Cannot find target filesystem")?;
        }
        let output = Command::new("df").arg("-Pk").arg(path).output()?;
        if !output.status.success() {
            return Err("Cannot measure available build space".into());
        }
        let text = String::from_utf8(output.stdout)?;
        let line = text.lines().nth(1).ok_or("Missing filesystem capacity")?;
        let available: u64 = line
            .split_whitespace()
            .nth(3)
            .ok_or("Missing available space")?
            .parse()?;
        available
            .checked_mul(1024)
            .ok_or_else(|| "Filesystem capacity overflow".into())
    }

    fn profiles(&self) -> Result<Vec<Profile>> {
        let mut roots = vec![self.target.clone()];
        if self.target.is_dir() {
            for entry in fs::read_dir(&self.target)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    roots.push(entry.path());
                }
            }
        }
        let mut profiles = Vec::new();
        for root in roots {
            for name in ["debug", "release", "cef-helper"] {
                if let Some(profile) = Profile::open(root.join(name))? {
                    profiles.push(profile);
                }
            }
        }
        Ok(profiles)
    }

    fn cache_bytes(&self) -> Result<u64> {
        let mut paths = Vec::new();
        for profile in self.profiles()? {
            paths.extend(profile.cache_paths()?);
        }
        if paths.is_empty() {
            return Ok(0);
        }
        let output = Command::new("du").arg("-sk").args(paths).output()?;
        if !output.status.success() {
            return Err("Cannot measure compile cache size".into());
        }
        let mut total = 0u64;
        for line in String::from_utf8(output.stdout)?.lines() {
            let blocks: u64 = line
                .split_whitespace()
                .next()
                .ok_or("Missing cache size")?
                .parse()?;
            total = total
                .checked_add(blocks.checked_mul(1024).ok_or("Cache size overflow")?)
                .ok_or("Cache size overflow")?;
        }
        Ok(total)
    }

    fn cargo_target(&self) -> Result<bool> {
        match fs::read_to_string(self.target.join("CACHEDIR.TAG")) {
            Ok(tag) => Ok(tag.contains("Signature: 8a477f597d28d172789f06886806bc55")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn prune_caches(&self, processes: &Processes, apply: bool) -> Result<()> {
        if !self.cargo_target()? {
            println!("Build storage: no Cargo cache tag; cache deletion skipped");
            return Ok(());
        }
        let mut removed = false;
        for profile in self.profiles()? {
            Signals::check()?;
            if processes.uses(profile.path()) {
                println!(
                    "Build storage: keeping active profile {}",
                    profile.path().display()
                );
                continue;
            }
            let lock = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(profile.path().join(".cargo-lock"))?;
            match lock.try_lock() {
                Ok(()) => {}
                Err(TryLockError::WouldBlock) => {
                    println!(
                        "Build storage: keeping locked profile {}",
                        profile.path().display()
                    );
                    continue;
                }
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
            for path in profile.cache_paths()? {
                Signals::check()?;
                let operation = if apply { "removing" } else { "would remove" };
                println!("Build storage: {operation} {}", path.display());
                if !apply {
                    continue;
                }
                if !removed {
                    if let Some(directory) = Directory::open(self.target.join(".vmux-seed"))? {
                        self.remove(directory.path())?;
                    }
                    removed = true;
                }
                if path.file_name().is_some_and(|name| name == "deps") {
                    for entry in fs::read_dir(&path)? {
                        Signals::check()?;
                        let entry = entry?;
                        let path = entry.path();
                        if path
                            .extension()
                            .is_some_and(|ext| ext == "dylib" || ext == "so")
                        {
                            continue;
                        }
                        self.remove(&path)?;
                    }
                } else {
                    self.remove(&path)?;
                }
            }
        }
        Ok(())
    }

    fn remove(&self, path: &Path) -> Result<()> {
        if !path.starts_with(&self.target) {
            return Err(format!(
                "Refusing to delete outside Cargo target: {}",
                path.display()
            )
            .into());
        }
        if fs::symlink_metadata(path)?.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn prune_apps(&self, processes: &Processes, current: &str) -> Result<()> {
        if !self.cargo_target()? {
            println!("Build storage: no Cargo cache tag; app deletion skipped");
            return Ok(());
        }
        for directory in self.profiles()? {
            if directory
                .path()
                .file_name()
                .is_none_or(|name| name != "release")
            {
                continue;
            }
            let mut apps = Vec::new();
            for entry in fs::read_dir(directory.path())? {
                let entry = entry?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let Some(revision) = name
                    .strip_prefix("Vmux (")
                    .and_then(|name| name.strip_suffix(").app"))
                else {
                    continue;
                };
                if !(7..=40).contains(&revision.len())
                    || !revision
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    || !entry.file_type()?.is_dir()
                {
                    continue;
                }
                apps.push((entry.metadata()?.modified()?, entry.path()));
            }
            apps.sort_by_key(|app| std::cmp::Reverse(app.0));
            let mut kept = HashSet::new();
            for (_, path) in &apps {
                if path.file_name().is_some_and(|name| name == current) {
                    kept.insert(path.clone());
                }
            }
            for (_, path) in &apps {
                if kept.len() >= self.policy.local_keep {
                    break;
                }
                kept.insert(path.clone());
            }
            for (_, path) in apps {
                if kept.contains(&path) || processes.uses(&path) {
                    continue;
                }
                println!("Build storage: removing old local app {}", path.display());
                self.remove(&path)?;
            }
        }
        Ok(())
    }

    pub fn check(&self) -> Result<()> {
        let free = self.available()?;
        println!("Build storage: {:.1} GiB free", free as f64 / GIB as f64);
        self.policy.check(free)?;
        if free < self.policy.warning {
            eprintln!(
                "Build storage: warning, less than {} GiB free",
                self.policy.warning as f64 / GIB as f64
            );
        }
        Ok(())
    }

    pub fn guard(&self) -> Result<()> {
        self.policy.check(self.available()?)
    }

    pub fn preflight(&self) -> Result<()> {
        if self.available()? < self.policy.minimum {
            self.prune_caches(&Processes::capture()?, true)?;
        }
        self.check()
    }

    pub fn finish(&self, profile: &str) -> Result<()> {
        let processes = Processes::capture()?;
        if profile == "local" {
            let output = Command::new("git")
                .arg("-C")
                .arg(&self.root)
                .args(["rev-parse", "--short=7", "HEAD"])
                .output()?;
            if !output.status.success() {
                return Err("Cannot identify the current local build".into());
            }
            let revision = String::from_utf8(output.stdout)?;
            self.prune_apps(&processes, &format!("Vmux ({}).app", revision.trim()))?;
        }
        self.finish_caches(&processes, profile)
    }

    fn finish_caches(&self, processes: &Processes, profile: &str) -> Result<()> {
        let size = self.cache_bytes()?;
        if size > self.policy.cache_limit || self.available()? < self.policy.minimum {
            println!(
                "Build storage: cache uses {:.1} GiB; pruning unused profiles",
                size as f64 / GIB as f64
            );
            self.prune_caches(processes, true)?;
        }
        let free = self.available()?;
        println!(
            "Build storage: {:.1} GiB free after {profile} build",
            free as f64 / GIB as f64
        );
        if free < self.policy.warning {
            eprintln!(
                "Build storage: free space remains low; active profiles and distribution artifacts were kept"
            );
        }
        Ok(())
    }

    pub fn report(&self) -> Result<()> {
        println!(
            "Build storage: {:.1} GiB free, {:.1} GiB of compile caches",
            self.available()? as f64 / GIB as f64,
            self.cache_bytes()? as f64 / GIB as f64
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File, FileTimes};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use crate::policy::Policy;
    use crate::processes::Processes;

    use super::Storage;

    struct Fixture {
        base: PathBuf,
        storage: Storage,
    }

    impl Fixture {
        fn new() -> Self {
            let base = tempfile::tempdir().unwrap().keep().canonicalize().unwrap();
            let root = base.join("checkout with spaces");
            let target = base.join("target with spaces");
            fs::create_dir(&root).unwrap();
            fs::create_dir(&target).unwrap();
            let policy = Policy {
                minimum: 0,
                warning: 0,
                cache_limit: u64::MAX,
                local_keep: 3,
            };
            let storage = Storage::new(root, target, policy).unwrap();
            let fixture = Self { base, storage };
            fixture.write(
                "CACHEDIR.TAG",
                "Signature: 8a477f597d28d172789f06886806bc55",
            );
            fixture
        }

        fn write(&self, relative: &str, contents: &str) -> PathBuf {
            let path = self.storage.target.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, contents).unwrap();
            path
        }

        fn caches(&self, profile: &str) {
            for relative in [
                ".fingerprint/current",
                "deps/object.o",
                "deps/library.rlib",
                "deps/current.dylib",
                "deps/current.so",
                "build/out/generated",
                "incremental/state",
            ] {
                self.write(&format!("{profile}/{relative}"), "compiled artifact");
            }
            self.write(&format!("{profile}/vmux_desktop"), "ready executable");
            self.write(
                &format!("{profile}/Vmux Dev.app/Contents/MacOS/vmux_desktop"),
                "signed app",
            );
            self.write(".vmux-seed/compatible", "compatible");
        }

        fn app(&self, revision: usize) -> PathBuf {
            let path = self
                .write(
                    &format!("release/Vmux ({revision:07x}).app/Contents/MacOS/vmux_desktop"),
                    "signed app",
                )
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            let time = SystemTime::UNIX_EPOCH + Duration::from_secs(revision as u64);
            File::open(&path)
                .unwrap()
                .set_times(FileTimes::new().set_modified(time))
                .unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.base).unwrap();
        }
    }

    #[test]
    fn cache_pruning_preserves_prepared_apps_and_runtime_libraries() {
        let fixture = Fixture::new();
        let profiles = ["debug", "release", "cef-helper", "aarch64-apple-ios/debug"];
        for profile in profiles {
            fixture.caches(profile);
        }
        fixture
            .storage
            .prune_caches(&Processes::idle(), true)
            .unwrap();
        for profile in profiles {
            let path = fixture.storage.target.join(profile);
            assert_eq!(
                fs::read_to_string(path.join("vmux_desktop")).unwrap(),
                "ready executable"
            );
            assert_eq!(
                fs::read_to_string(path.join("Vmux Dev.app/Contents/MacOS/vmux_desktop")).unwrap(),
                "signed app"
            );
            assert!(path.join("deps/current.dylib").is_file());
            assert!(path.join("deps/current.so").is_file());
            assert!(!path.join("deps/object.o").exists());
            assert!(!path.join("deps/library.rlib").exists());
            assert!(!path.join(".fingerprint").exists());
        }
        assert!(!fixture.storage.target.join(".vmux-seed").exists());
    }

    #[test]
    fn active_profile_is_preserved() {
        let fixture = Fixture::new();
        fixture.caches("debug");
        fixture.caches("release");
        let opened = format!(
            "p12\nn{}\n",
            fixture
                .storage
                .target
                .join("debug/Vmux Dev.app/Contents/MacOS/vmux_desktop")
                .display()
        );
        let processes = Processes::parse("12 vmux_desktop", &opened, 99).unwrap();
        fixture.storage.prune_caches(&processes, true).unwrap();
        assert!(fixture.storage.target.join("debug/deps/object.o").is_file());
        assert!(
            !fixture
                .storage
                .target
                .join("release/deps/object.o")
                .exists()
        );
    }

    #[test]
    fn raw_cargo_lock_prevents_cache_pruning() {
        let fixture = Fixture::new();
        fixture.caches("debug");
        let lock = File::create(fixture.storage.target.join("debug/.cargo-lock")).unwrap();
        lock.lock().unwrap();
        fixture
            .storage
            .prune_caches(&Processes::idle(), true)
            .unwrap();
        assert!(fixture.storage.target.join("debug/deps/object.o").exists());
        assert!(
            fixture
                .storage
                .target
                .join(".vmux-seed/compatible")
                .exists()
        );
    }

    #[test]
    fn untagged_target_is_not_pruned() {
        let fixture = Fixture::new();
        fixture.caches("debug");
        fs::remove_file(fixture.storage.target.join("CACHEDIR.TAG")).unwrap();
        fixture
            .storage
            .prune_caches(&Processes::idle(), true)
            .unwrap();
        assert!(fixture.storage.target.join("debug/deps/object.o").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_do_not_expose_external_directories_to_cleanup() {
        let fixture = Fixture::new();
        fixture.caches("debug");
        let outside = fixture.storage.root.join("user-data");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("important"), "user data").unwrap();
        std::os::unix::fs::symlink(&outside, fixture.storage.target.join("debug/examples"))
            .unwrap();
        fixture
            .storage
            .prune_caches(&Processes::idle(), true)
            .unwrap();
        assert_eq!(
            fs::read_to_string(outside.join("important")).unwrap(),
            "user data"
        );
        let link = fixture.storage.root.join("target-link");
        std::os::unix::fs::symlink(&fixture.storage.target, &link).unwrap();
        let policy = Policy {
            minimum: 0,
            warning: 0,
            cache_limit: 1,
            local_keep: 3,
        };
        assert!(Storage::new(fixture.storage.root.clone(), link, policy).is_err());
        let dangling = fixture.storage.root.join("dangling-target");
        std::os::unix::fs::symlink(fixture.base.join("missing"), &dangling).unwrap();
        let policy = Policy {
            minimum: 0,
            warning: 0,
            cache_limit: 1,
            local_keep: 3,
        };
        assert!(Storage::new(fixture.storage.root.clone(), dangling, policy).is_err());
    }

    #[test]
    fn cleanup_preview_keeps_cache_and_seed_marker() {
        let fixture = Fixture::new();
        fixture.caches("debug");
        fixture
            .storage
            .prune_caches(&Processes::idle(), false)
            .unwrap();
        assert!(fixture.storage.target.join("debug/deps/object.o").exists());
        assert!(
            fixture
                .storage
                .target
                .join("debug/.fingerprint/current")
                .exists()
        );
        assert!(
            fixture
                .storage
                .target
                .join(".vmux-seed/compatible")
                .exists()
        );
    }

    #[test]
    fn app_retention_keeps_current_and_running_bundles() {
        let fixture = Fixture::new();
        let mut apps = Vec::new();
        for index in 1..=7 {
            apps.push(fixture.app(index));
        }
        fixture.write(
            "release/Vmux.app/Contents/MacOS/vmux_desktop",
            "shipping app",
        );
        fixture.write(
            "release/Vmux Dev.app/Contents/MacOS/vmux_desktop",
            "debug app",
        );
        fixture.write("release/Vmux.dmg", "shipping image");
        let opened = format!(
            "p12\nn{}\n",
            apps[1].join("Contents/MacOS/vmux_desktop").display()
        );
        let processes = Processes::parse("12 vmux_desktop", &opened, 99).unwrap();
        fixture
            .storage
            .prune_apps(&processes, "Vmux (0000001).app")
            .unwrap();
        for (index, app) in apps.iter().enumerate() {
            assert_eq!(app.exists(), matches!(index, 0 | 1 | 5 | 6));
        }
        assert!(fixture.storage.target.join("release/Vmux.app").exists());
        assert!(fixture.storage.target.join("release/Vmux Dev.app").exists());
        assert_eq!(
            fs::read_to_string(fixture.storage.target.join("release/Vmux.dmg")).unwrap(),
            "shipping image"
        );
    }

    #[test]
    fn cache_budget_cleanup_keeps_distribution_files() {
        let mut fixture = Fixture::new();
        fixture.caches("release");
        fixture.storage.policy.cache_limit = 1;
        fixture.write(
            "release/Vmux.app/Contents/MacOS/vmux_desktop",
            "shipping app",
        );
        fixture.write("release/Vmux.dmg", "shipping image");
        fixture
            .storage
            .finish_caches(&Processes::idle(), "release")
            .unwrap();
        assert!(
            !fixture
                .storage
                .target
                .join("release/deps/object.o")
                .exists()
        );
        assert!(fixture.storage.target.join("release/Vmux.app").exists());
        assert_eq!(
            fs::read_to_string(fixture.storage.target.join("release/Vmux.dmg")).unwrap(),
            "shipping image"
        );
    }

    #[test]
    fn warning_only_capacity_preserves_warm_caches() {
        let mut fixture = Fixture::new();
        fixture.caches("debug");
        fixture.storage.policy.warning = u64::MAX;
        fixture
            .storage
            .finish_caches(&Processes::idle(), "dev")
            .unwrap();
        assert!(fixture.storage.target.join("debug/deps/object.o").exists());
        assert!(
            fixture
                .storage
                .target
                .join("debug/.fingerprint/current")
                .exists()
        );
    }

    #[test]
    fn workspace_and_outside_deletion_are_refused() {
        let fixture = Fixture::new();
        let policy = Policy {
            minimum: 0,
            warning: 0,
            cache_limit: 1,
            local_keep: 3,
        };
        assert!(
            Storage::new(
                fixture.storage.root.clone(),
                fixture.storage.root.clone(),
                policy
            )
            .is_err()
        );
        let file = fixture.storage.root.join("source");
        fs::write(&file, "source code").unwrap();
        assert!(fixture.storage.remove(&file).is_err());
        assert_eq!(fs::read_to_string(file).unwrap(), "source code");
    }
}
