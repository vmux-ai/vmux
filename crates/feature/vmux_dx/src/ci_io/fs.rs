use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component as PathComponent, PathBuf};

use bevy_ecs::prelude::Component;
use sha2::{Digest, Sha256};
use vmux_job::Job;

use super::IoJob;

#[derive(Clone)]
pub(crate) struct Location {
    pub(crate) root: PathBuf,
    pub(crate) relative: PathBuf,
}

impl Location {
    pub(crate) fn resolve(&self) -> Result<PathBuf, String> {
        let root = self
            .root
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if self.relative.is_absolute() || self.relative.as_os_str().is_empty() {
            return Err("CI path must be relative to its checkout".into());
        }
        let mut path = root.clone();
        for part in self.relative.components() {
            match part {
                PathComponent::Normal(part) => path.push(part),
                PathComponent::CurDir => continue,
                _ => return Err("CI path escapes its checkout".into()),
            }
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err("CI paths must not contain symbolic links".into());
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        if path == root {
            return Err("CI path must be inside its checkout".into());
        }
        Ok(path)
    }
}

#[derive(Clone)]
pub(crate) struct ReadFile {
    pub(crate) location: Location,
    pub(crate) limit: u64,
}

impl IoJob for ReadFile {
    type Data = Bytes;
}

impl Job for ReadFile {
    type Output = Result<Bytes, String>;

    fn run(self) -> Self::Output {
        let path = self.location.resolve()?;
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err(format!(
                "CI input {:?} is not a regular file",
                self.location.relative
            ));
        }
        if metadata.len() > self.limit {
            return Err(format!(
                "CI input {:?} exceeds its bound: {} bytes > {} bytes",
                self.location.relative,
                metadata.len(),
                self.limit
            ));
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(self.limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > self.limit {
            return Err("CI input exceeded its bound while reading".into());
        }
        Ok(Bytes(bytes))
    }
}

#[derive(Component, Clone)]
pub(crate) struct Bytes(pub Vec<u8>);

#[derive(Clone)]
pub(crate) struct Publish {
    pub(crate) location: Location,
    pub(crate) bytes: Vec<u8>,
    pub(crate) executable: bool,
}

impl IoJob for Publish {
    type Data = Written;
}

impl Job for Publish {
    type Output = Result<Written, String>;

    fn run(self) -> Self::Output {
        let path = self.location.resolve()?;
        let parent = path.parent().ok_or("Missing CI publication directory")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let fragment = format!("{:x}", Sha256::digest(&self.bytes))[..16].to_owned();
        let staging = parent.join(format!(".vmux-ci-{}-{}", std::process::id(), fragment));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&staging)
            .map_err(|error| error.to_string())?;
        let mut owned = Staging(staging.clone());
        file.write_all(&self.bytes)
            .map_err(|error| error.to_string())?;
        let mode = if self.executable { 0o755 } else { 0o600 };
        file.set_permissions(std::fs::Permissions::from_mode(mode))
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        self.location.resolve()?;
        std::fs::rename(&staging, &path).map_err(|error| error.to_string())?;
        owned.0 = PathBuf::new();
        Ok(Written)
    }
}

struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

#[derive(Component, Clone)]
pub(crate) struct Written;

#[derive(Clone)]
pub(crate) struct OptionalRead {
    pub(crate) location: Location,
    pub(crate) limit: u64,
}

impl IoJob for OptionalRead {
    type Data = OptionalBytes;
}

impl Job for OptionalRead {
    type Output = Result<OptionalBytes, String>;

    fn run(self) -> Self::Output {
        let path = self.location.resolve()?;
        if !path.exists() {
            return Ok(OptionalBytes(None));
        }
        let bytes = ReadFile {
            location: self.location,
            limit: self.limit,
        }
        .run()?;
        Ok(OptionalBytes(Some(bytes.0)))
    }
}

#[derive(Component, Clone)]
pub(crate) struct OptionalBytes(pub Option<Vec<u8>>);

#[derive(Clone)]
pub(crate) struct ProbeRead(pub ReadFile);

impl IoJob for ProbeRead {
    type Data = ProbedBytes;
}

impl Job for ProbeRead {
    type Output = Result<ProbedBytes, String>;

    fn run(self) -> Self::Output {
        Ok(ProbedBytes(self.0.run().map(|bytes| bytes.0)))
    }
}

#[derive(Component, Clone)]
pub(crate) struct ProbedBytes(pub Result<Vec<u8>, String>);

#[derive(Clone)]
pub(crate) struct SetTimes {
    pub(crate) location: Location,
    pub(crate) accessed: i128,
    pub(crate) modified: i128,
}

impl IoJob for SetTimes {
    type Data = Written;
}

impl Job for SetTimes {
    type Output = Result<Written, String>;

    fn run(self) -> Self::Output {
        let path = self.location.resolve()?;
        let atime = filetime::FileTime::from_unix_time(
            (self.accessed / 1_000_000_000) as i64,
            (self.accessed % 1_000_000_000) as u32,
        );
        let mtime = filetime::FileTime::from_unix_time(
            (self.modified / 1_000_000_000) as i64,
            (self.modified % 1_000_000_000) as u32,
        );
        filetime::set_file_times(path, atime, mtime).map_err(|error| error.to_string())?;
        Ok(Written)
    }
}

#[derive(Clone)]
pub(crate) struct RemoveFile(pub Location);

impl IoJob for RemoveFile {
    type Data = Written;
}

impl Job for RemoveFile {
    type Output = Result<Written, String>;

    fn run(self) -> Self::Output {
        let path = self.0.resolve()?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(Written),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Written),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RemoveLink(pub Location);

impl IoJob for RemoveLink {
    type Data = Written;
}

impl Job for RemoveLink {
    type Output = Result<Written, String>;

    fn run(self) -> Self::Output {
        let parent = self.0.relative.parent().ok_or("Missing symlink parent")?;
        let parent = Location {
            root: self.0.root,
            relative: parent.into(),
        }
        .resolve()?;
        let name = self
            .0
            .relative
            .file_name()
            .ok_or("Missing symlink filename")?;
        let path = parent.join(name);
        match path.symlink_metadata() {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                std::fs::remove_file(path).map_err(|error| error.to_string())?;
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        Ok(Written)
    }
}

#[derive(Clone)]
pub(crate) struct SourceFacts {
    pub(crate) root: PathBuf,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) target: Option<PathBuf>,
}

impl IoJob for SourceFacts {
    type Data = CapturedSources;
}

impl Job for SourceFacts {
    type Output = Result<CapturedSources, String>;

    fn run(self) -> Self::Output {
        let root = self
            .root
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let mut facts = CapturedSources {
            files: Vec::new(),
            directories: BTreeMap::new(),
            entries: Vec::new(),
            links: Vec::new(),
            git_references: Vec::new(),
        };
        let mut directories = BTreeSet::new();
        for relative in &self.paths {
            Self::cancellation()?;
            match Self::file(&root, relative) {
                Ok(Some(file)) => facts.files.push(file),
                Ok(None) => {
                    if root
                        .join(relative)
                        .symlink_metadata()
                        .is_ok_and(|metadata| metadata.file_type().is_symlink())
                    {
                        facts.links.push(relative.to_string_lossy().into_owned());
                    }
                }
                Err(error) => return Err(error),
            }
            let mut parent = relative.parent();
            while let Some(path) = parent {
                if path.as_os_str().is_empty() {
                    break;
                }
                directories.insert(path.to_owned());
                parent = path.parent();
            }
        }
        facts
            .files
            .sort_by(|left, right| left.name.cmp(&right.name));
        for relative in directories {
            Self::cancellation()?;
            if !root.join(&relative).exists() {
                continue;
            }
            facts.directories.insert(
                relative.to_string_lossy().into_owned(),
                Self::directory(&root, &relative)?,
            );
        }
        if let Some(target) = &self.target {
            facts.entries = Self::entries(&root, target)?;
        }
        facts.git_references = Self::references(&root)?;
        Ok(facts)
    }
}

impl SourceFacts {
    fn cancellation() -> Result<(), String> {
        if vmux_job::blocking::BlockingCancellation::canceled() {
            return Err("CI source capture canceled".into());
        }
        Ok(())
    }

    fn file(
        root: &std::path::Path,
        relative: &std::path::Path,
    ) -> Result<Option<SourceFile>, String> {
        let parent = relative.parent().unwrap_or(std::path::Path::new(""));
        let parent = if parent.as_os_str().is_empty() {
            root.into()
        } else {
            Location {
                root: root.into(),
                relative: parent.into(),
            }
            .resolve()?
        };
        let path = parent.join(relative.file_name().ok_or("Missing CI source filename")?);
        let metadata = match path.symlink_metadata() {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        if metadata.file_type().is_symlink() {
            return Ok(None);
        }
        if !metadata.is_file() {
            return Err("CI tracked input is not a regular file".into());
        }
        if metadata.len() > 256 * 1024 * 1024 {
            return Err("CI tracked file exceeds 256 MiB".into());
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .map_err(|error| error.to_string())?;
        if !file
            .metadata()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("CI tracked input changed file type".into());
        }
        let mut hash = Sha256::new();
        let mut buffer = [0; 8192];
        let mut read = 0usize;
        loop {
            Self::cancellation()?;
            let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            read += count;
            if read > 256 * 1024 * 1024 {
                return Err("CI file exceeded capture bound".into());
            }
            hash.update(&buffer[..count]);
        }
        Ok(Some(SourceFile {
            name: relative
                .to_str()
                .ok_or("CI source path must be UTF-8")?
                .into(),
            hash: format!("{:x}", hash.finalize()),
            mode: metadata.mode(),
            accessed: metadata.atime() as i128 * 1_000_000_000 + metadata.atime_nsec() as i128,
            modified: metadata.mtime() as i128 * 1_000_000_000 + metadata.mtime_nsec() as i128,
        }))
    }

    fn directory(
        root: &std::path::Path,
        relative: &std::path::Path,
    ) -> Result<SourceDirectory, String> {
        let path = Location {
            root: root.into(),
            relative: relative.into(),
        }
        .resolve()?;
        let metadata = path.metadata().map_err(|error| error.to_string())?;
        let mut children = Vec::new();
        for entry in std::fs::read_dir(&path).map_err(|error| error.to_string())? {
            Self::cancellation()?;
            if children.len() >= 100_000 {
                return Err("CI directory exceeds capture bound".into());
            }
            let entry = entry.map_err(|error| error.to_string())?;
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            children.push(DirectoryChild {
                name: relative
                    .join(entry.file_name())
                    .to_str()
                    .ok_or("CI directory path must be UTF-8")?
                    .into(),
                file: kind.is_file(),
                directory: kind.is_dir(),
            });
        }
        Ok(SourceDirectory {
            accessed: metadata.atime() as i128 * 1_000_000_000 + metadata.atime_nsec() as i128,
            modified: metadata.mtime() as i128 * 1_000_000_000 + metadata.mtime_nsec() as i128,
            children,
        })
    }

    fn entries(
        root: &std::path::Path,
        relative: &std::path::Path,
    ) -> Result<Vec<SourceEntry>, String> {
        let target = Location {
            root: root.into(),
            relative: relative.into(),
        }
        .resolve()?;
        let mut queue = vec![target];
        let mut entries = Vec::new();
        while let Some(path) = queue.pop() {
            Self::cancellation()?;
            if !path.exists() {
                continue;
            }
            Location {
                root: root.into(),
                relative: path
                    .strip_prefix(root)
                    .map_err(|error| error.to_string())?
                    .into(),
            }
            .resolve()?;
            for entry in std::fs::read_dir(&path).map_err(|error| error.to_string())? {
                Self::cancellation()?;
                if entries.len() >= 200_000 {
                    return Err("CI compiler inventory exceeds capture bound".into());
                }
                let entry = entry.map_err(|error| error.to_string())?;
                let kind = entry.file_type().map_err(|error| error.to_string())?;
                let metadata = entry
                    .path()
                    .symlink_metadata()
                    .map_err(|error| error.to_string())?;
                entries.push(SourceEntry {
                    name: entry
                        .path()
                        .strip_prefix(root)
                        .map_err(|error| error.to_string())?
                        .into(),
                    directory: kind.is_dir(),
                    modified: metadata.mtime() as i128 * 1_000_000_000
                        + metadata.mtime_nsec() as i128,
                });
                if kind.is_dir() {
                    queue.push(entry.path());
                }
            }
        }
        Ok(entries)
    }

    fn references(root: &std::path::Path) -> Result<Vec<SourceFile>, String> {
        let git_directory = root.join(".git");
        let mut references = Vec::new();
        if !git_directory.is_dir()
            || git_directory
                .symlink_metadata()
                .map_err(|error| error.to_string())?
                .file_type()
                .is_symlink()
        {
            return Ok(references);
        }
        let mut names = vec![PathBuf::from(".git/HEAD"), PathBuf::from(".git/logs/HEAD")];
        if let Ok(head) = std::fs::read_to_string(git_directory.join("HEAD"))
            && let Some(reference) = head.trim().strip_prefix("ref: ")
            && reference.starts_with("refs/")
            && !reference.contains("..")
        {
            names.push(PathBuf::from(".git").join(reference));
        }
        for relative in names {
            if let Some(file) = Self::file(root, &relative)? {
                references.push(file);
            }
        }
        Ok(references)
    }
}

#[derive(Component, Clone)]
pub(crate) struct CapturedSources {
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub(crate) files: Vec<SourceFile>,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub(crate) directories: BTreeMap<String, SourceDirectory>,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub(crate) entries: Vec<SourceEntry>,
    pub(crate) links: Vec<String>,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub(crate) git_references: Vec<SourceFile>,
}

#[derive(Clone)]
pub(crate) struct SourceEntry {
    pub(crate) name: PathBuf,
    pub(crate) directory: bool,
    pub(crate) modified: i128,
}

#[derive(Clone)]
pub(crate) struct SourceFile {
    pub(crate) name: String,
    pub(crate) hash: String,
    pub(crate) mode: u32,
    pub(crate) accessed: i128,
    pub(crate) modified: i128,
}

#[derive(Clone)]
pub(crate) struct SourceDirectory {
    pub(crate) accessed: i128,
    pub(crate) modified: i128,
    pub(crate) children: Vec<DirectoryChild>,
}

#[derive(Clone)]
pub(crate) struct DirectoryChild {
    pub(crate) name: String,
    pub(crate) file: bool,
    pub(crate) directory: bool,
}

#[derive(Clone)]
pub(crate) struct RemoveDirectory(pub Location);

impl IoJob for RemoveDirectory {
    type Data = Written;
}

impl Job for RemoveDirectory {
    type Output = Result<Written, String>;

    fn run(self) -> Self::Output {
        let path = self.0.resolve()?;
        match std::fs::remove_dir_all(path) {
            Ok(()) => Ok(Written),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Written),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[derive(Clone)]
pub(crate) struct DirectoryProbe(pub Location);

impl IoJob for DirectoryProbe {
    type Data = DirectoryFact;
}

impl Job for DirectoryProbe {
    type Output = Result<DirectoryFact, String>;

    fn run(self) -> Self::Output {
        let path = self.0.resolve()?;
        match path.symlink_metadata() {
            Ok(metadata) => Ok(DirectoryFact(Some(metadata.is_dir()))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DirectoryFact(None)),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[derive(Component, Clone)]
pub(crate) struct DirectoryFact(pub Option<bool>);

#[derive(Clone)]
pub(crate) struct Print(pub Vec<u8>);

impl IoJob for Print {
    type Data = Written;
}

impl Job for Print {
    type Output = Result<Written, String>;

    fn run(self) -> Self::Output {
        std::io::stdout()
            .write_all(&self.0)
            .map_err(|error| error.to_string())?;
        Ok(Written)
    }
}
