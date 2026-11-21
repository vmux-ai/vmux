use std::io::{self, ErrorKind};
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
use std::os::unix::ffi::{OsStrExt, OsStringExt};
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use bevy_ecs::prelude::Component as EcsComponent;
use vmux_ecs::{BlockingCancellation, Job};

#[derive(EcsComponent, Clone, Default)]
pub struct PathReceipt(Arc<Mutex<Receipt>>);

#[derive(Default)]
struct Receipt {
    changed: bool,
    directories: Vec<OwnedDirectory>,
    displaced: Vec<DisplacedEntry>,
    unverified: Vec<UnverifiedDirectory>,
    private: Arc<Mutex<Vec<PrivateCleanup>>>,
}

impl Receipt {
    fn cleanup_private(&self) -> Result<(), String> {
        let mut pending = {
            let mut queue = self.private.lock().map_err(|error| error.to_string())?;
            std::mem::take(&mut *queue)
        };
        while let Some(directory) = pending.pop() {
            if let Err(error) = directory.cleanup() {
                pending.push(directory);
                self.private
                    .lock()
                    .map_err(|error| error.to_string())?
                    .extend(pending);
                return Err(error.to_string());
            }
        }
        Ok(())
    }
}

impl PathReceipt {
    pub fn changed(&self) -> bool {
        self.0.lock().unwrap().changed
    }

    pub fn pending(&self) -> bool {
        let receipt = self.0.lock().unwrap();
        !receipt.directories.is_empty()
            || !receipt.displaced.is_empty()
            || !receipt.unverified.is_empty()
            || !receipt.private.lock().unwrap().is_empty()
    }

    pub(crate) fn commit(&self) {
        self.0.lock().unwrap().directories.clear();
    }

    pub(super) fn record(&self) {
        self.0.lock().unwrap().changed = true;
    }

    fn directory(&self, directory: OwnedDirectory) {
        self.0.lock().unwrap().directories.push(directory);
    }

    fn displaced(&self, entry: DisplacedEntry) {
        self.0.lock().unwrap().displaced.push(entry);
    }

    fn private(&self, parent: Arc<NativeDirectory>) -> io::Result<OwnedDirectory> {
        let mut receipt = self
            .0
            .lock()
            .map_err(|error| io::Error::other(error.to_string()))?;
        let Receipt {
            unverified,
            private,
            ..
        } = &mut *receipt;
        NativeDirectory::private(parent, unverified, private)
    }

    fn reconcile(&self) -> Result<(), String> {
        let mut receipt = self.0.lock().map_err(|error| error.to_string())?;
        while let Some(directory) = receipt.unverified.pop() {
            if let Err(error) = directory.released() {
                receipt.unverified.push(directory);
                return Err(error.to_string());
            }
        }
        while let Some(entry) = receipt.displaced.pop() {
            if let Err(error) = entry.restore() {
                receipt.displaced.push(entry);
                return Err(error.to_string());
            }
        }
        receipt.cleanup_private()?;
        while let Some(directory) = receipt.directories.pop() {
            let Receipt {
                displaced,
                unverified,
                private,
                ..
            } = &mut *receipt;
            if let Err(error) = directory.cleanup(displaced, unverified, private) {
                receipt.directories.push(directory);
                return Err(error.to_string());
            }
            if let Err(error) = receipt.cleanup_private() {
                receipt.directories.push(directory);
                return Err(error);
            }
        }
        receipt.cleanup_private()
    }
}

#[derive(EcsComponent)]
pub struct PathChanged(pub bool);

#[derive(Clone)]
pub struct CleanupPath {
    pub receipt: PathReceipt,
}

impl Job for CleanupPath {
    type Output = Result<PathChanged, String>;

    fn run(self) -> Self::Output {
        self.receipt.reconcile().map(|()| PathChanged(false))
    }
}

#[derive(Clone)]
pub struct LinkPath {
    pub source: PathBuf,
    pub source_may_be_missing: bool,
    pub target: PathBuf,
    pub boundary: Option<PathBuf>,
    pub receipt: PathReceipt,
}

impl Job for LinkPath {
    type Output = Result<PathChanged, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("link creation canceled".into());
        }
        let source = if self.boundary.is_some() && !self.source_may_be_missing {
            self.source
                .canonicalize()
                .map_err(|error| error.to_string())?
        } else {
            self.source
        };
        let pair = LinkPaths::new(source, self.target, self.boundary)?;
        let parent = pair
            .parent(true, &self.receipt)
            .map_err(|error| error.to_string())?;
        let name = pair.name()?;
        match parent.entry(&name) {
            Ok(entry) => {
                if entry
                    .metadata()
                    .map_err(|error| error.to_string())?
                    .file_type()
                    .is_symlink()
                    && pair.matches(&parent, &name, &parent)?
                {
                    return Ok(PathChanged(false));
                }
                return Err(format!(
                    "link target is occupied: {}",
                    pair.target.display()
                ));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        if BlockingCancellation::canceled() {
            return Err("link creation canceled".into());
        }
        let relative = LinkPaths::relative(
            &parent.location().map_err(|error| error.to_string())?,
            &pair.source,
        );
        match parent.link(&relative, &name) {
            Ok(()) => {
                self.receipt.record();
                Ok(PathChanged(true))
            }
            Err(error)
                if error.kind() == ErrorKind::AlreadyExists
                    && pair.matches(&parent, &name, &parent)? =>
            {
                Ok(PathChanged(false))
            }
            Err(error) => Err(error.to_string()),
        }
    }
}

#[derive(Clone)]
pub struct UnlinkPath {
    pub source: PathBuf,
    pub target: PathBuf,
    pub boundary: Option<PathBuf>,
    pub receipt: PathReceipt,
}

impl Job for UnlinkPath {
    type Output = Result<PathChanged, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("link removal canceled".into());
        }
        let pair = LinkPaths::new(
            self.source.clone(),
            self.target.clone(),
            self.boundary.clone(),
        )?;
        let parent = match pair.parent(false, &self.receipt) {
            Ok(parent) => parent,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(PathChanged(false)),
            Err(error) => return Err(error.to_string()),
        };
        let name = pair.name()?;
        let original = match parent.entry(&name) {
            Ok(entry) => entry,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(PathChanged(false)),
            Err(error) => return Err(error.to_string()),
        };
        if !original
            .metadata()
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
            || !pair.matches(&parent, &name, &parent)?
        {
            return Ok(PathChanged(false));
        }
        self.remove(pair, parent, name, original)
    }
}

impl UnlinkPath {
    fn remove(
        self,
        pair: LinkPaths,
        parent: Arc<NativeDirectory>,
        name: std::ffi::OsString,
        original: std::fs::File,
    ) -> Result<PathChanged, String> {
        let slot = self
            .receipt
            .private(parent.clone())
            .map_err(|error| error.to_string())?;
        let holder = std::ffi::OsString::from("entry");
        match parent.move_exclusive(&name, &slot.directory, &holder) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(PathChanged(false)),
            Err(error) => return Err(error.to_string()),
        }
        let entry = DisplacedEntry {
            parent: parent.clone(),
            name,
            slot,
            holder,
        };
        let owned = (|| -> Result<bool, String> {
            let moved = entry
                .slot
                .directory
                .entry(&entry.holder)
                .map_err(|error| error.to_string())?;
            let same =
                NativeDirectory::same(&original, &moved).map_err(|error| error.to_string())?;
            Ok(same && pair.matches(&entry.slot.directory, &entry.holder, &parent)?)
        })();
        if matches!(owned, Ok(true)) && !BlockingCancellation::canceled() {
            match entry.slot.directory.remove(&entry.holder, false) {
                Ok(()) => {
                    self.receipt.record();
                    return Ok(PathChanged(true));
                }
                Err(error) => {
                    self.receipt.displaced(entry);
                    return Err(error.to_string());
                }
            }
        }
        if let Err(error) = entry.restore() {
            let retained = entry
                .slot
                .directory
                .location()
                .unwrap_or_else(|_| pair.target.clone());
            self.receipt.displaced(entry);
            return Err(format!(
                "link replacement retained at {}: {error}",
                retained.display()
            ));
        }
        owned?;
        if BlockingCancellation::canceled() {
            return Err("link removal canceled".into());
        }
        Ok(PathChanged(false))
    }
}

struct LinkPaths {
    source: PathBuf,
    target: PathBuf,
    boundary: Option<PathBuf>,
}

impl LinkPaths {
    fn new(source: PathBuf, target: PathBuf, boundary: Option<PathBuf>) -> Result<Self, String> {
        if !source.is_absolute() || !target.is_absolute() {
            return Err("link paths must be absolute".into());
        }
        let source = if boundary.is_some() {
            Self::scoped_source(&source)?
        } else {
            vmux_path::PathIdentity::resolve(source).into_path_buf()
        };
        if let Some(root) = &boundary {
            let root = root.canonicalize().map_err(|error| error.to_string())?;
            if !source.starts_with(&root) {
                return Err("link source resolves outside the isolated data root".into());
            }
        }
        Ok(Self {
            source,
            target: vmux_path::PathIdentity::lexical(target).into_path_buf(),
            boundary,
        })
    }

    fn scoped_source(source: &Path) -> Result<PathBuf, String> {
        for ancestor in source.ancestors() {
            match ancestor.canonicalize() {
                Ok(resolved) => {
                    let suffix = source
                        .strip_prefix(ancestor)
                        .map_err(|error| error.to_string())?;
                    return Ok(
                        vmux_path::PathIdentity::lexical(resolved.join(suffix)).into_path_buf()
                    );
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("link source has no resolvable ancestor".into())
    }

    fn name(&self) -> Result<std::ffi::OsString, String> {
        self.target
            .file_name()
            .map(std::ffi::OsStr::to_os_string)
            .ok_or_else(|| "link target has no filename".into())
    }

    fn parent(&self, create: bool, receipt: &PathReceipt) -> io::Result<Arc<NativeDirectory>> {
        let parent = self
            .target
            .parent()
            .ok_or_else(|| io::Error::other("link target has no parent"))?;
        let root = self.boundary.as_deref().unwrap_or(Path::new("/"));
        let relative = parent
            .strip_prefix(root)
            .map_err(|_| io::Error::other("link target is outside the copied data root"))?;
        let mut directory = NativeDirectory::open(root)?;
        let scoped = self.boundary.is_some();
        for (depth, part) in relative.components().enumerate() {
            if depth >= 128 {
                return Err(io::Error::other("link parent depth exceeds128"));
            }
            let Component::Normal(name) = part else {
                if part == Component::CurDir {
                    continue;
                }
                return Err(io::Error::other("link parent escapes its root"));
            };
            directory = match directory.child(name, scoped) {
                Ok(child) => child,
                Err(error) if error.kind() == ErrorKind::NotFound && create => {
                    let mut staged = receipt.private(directory.clone())?;
                    match staged.parent.move_exclusive(&staged.name, &directory, name) {
                        Ok(()) => {
                            staged.private = false;
                            let child = staged.directory.clone();
                            let owned = OwnedDirectory {
                                parent: directory.clone(),
                                name: name.to_os_string(),
                                directory: child.clone(),
                                private: false,
                                cleanup: Weak::new(),
                            };
                            receipt.directory(owned);
                            child
                        }
                        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                            directory.child(name, scoped)?
                        }
                        Err(error) => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            };
            if BlockingCancellation::canceled() {
                return Err(io::Error::other("link parent creation canceled"));
            }
        }
        Ok(directory)
    }

    fn matches(
        &self,
        directory: &NativeDirectory,
        name: &std::ffi::OsStr,
        original_parent: &NativeDirectory,
    ) -> Result<bool, String> {
        let link = match directory.read_link(name) {
            Ok(link) => link,
            Err(error) if error.kind() == ErrorKind::InvalidInput => return Ok(false),
            Err(error) => return Err(error.to_string()),
        };
        let resolved = if link.is_absolute() {
            link
        } else {
            original_parent
                .location()
                .map_err(|error| error.to_string())?
                .join(link)
        };
        Ok(vmux_path::PathIdentity::resolve(resolved)
            == vmux_path::PathIdentity::resolve(&self.source))
    }

    fn relative(from: &Path, to: &Path) -> PathBuf {
        let left: Vec<_> = from.components().collect();
        let right: Vec<_> = to.components().collect();
        let mut shared = 0;
        while shared < left.len() && shared < right.len() && left[shared] == right[shared] {
            shared += 1;
        }
        let mut result = PathBuf::new();
        for _ in shared..left.len() {
            result.push("..");
        }
        for part in &right[shared..] {
            result.push(part.as_os_str());
        }
        result
    }
}

struct NativeDirectory {
    file: std::fs::File,
}

struct OwnedDirectory {
    parent: Arc<NativeDirectory>,
    name: std::ffi::OsString,
    directory: Arc<NativeDirectory>,
    private: bool,
    cleanup: Weak<Mutex<Vec<PrivateCleanup>>>,
}

struct PrivateCleanup {
    parent: Arc<NativeDirectory>,
    name: std::ffi::OsString,
    directory: Arc<NativeDirectory>,
}

impl PrivateCleanup {
    fn cleanup(&self) -> io::Result<()> {
        let current = match self.parent.entry(&self.name) {
            Ok(current) => current,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        if !NativeDirectory::same(&current, &self.directory.file)? {
            return Err(io::Error::other(
                "private native directory ownership changed",
            ));
        }
        self.parent.remove(&self.name, true)
    }
}

struct UnverifiedDirectory {
    parent: Arc<NativeDirectory>,
    name: std::ffi::OsString,
}

impl UnverifiedDirectory {
    fn released(&self) -> io::Result<()> {
        match self.parent.entry(&self.name) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
            Ok(_) => {
                let path = self.parent.location()?.join(&self.name);
                Err(io::Error::other(format!(
                    "unverified native directory retained at {}; explicit artifact disposition is required",
                    path.display()
                )))
            }
        }
    }
}

impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        if self.private
            && let Some(cleanup) = self.cleanup.upgrade()
        {
            cleanup.lock().unwrap().push(PrivateCleanup {
                parent: self.parent.clone(),
                name: self.name.clone(),
                directory: self.directory.clone(),
            });
        }
    }
}

impl OwnedDirectory {
    fn cleanup(
        &self,
        displaced: &mut Vec<DisplacedEntry>,
        unverified: &mut Vec<UnverifiedDirectory>,
        private: &Arc<Mutex<Vec<PrivateCleanup>>>,
    ) -> io::Result<()> {
        let current = match self.parent.entry(&self.name) {
            Ok(current) => current,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        if !NativeDirectory::same(&current, &self.directory.file)? {
            return Ok(());
        }
        let slot = NativeDirectory::private(self.parent.clone(), unverified, private)?;
        let holder = std::ffi::OsString::from("entry");
        self.parent
            .move_exclusive(&self.name, &slot.directory, &holder)?;
        let entry = DisplacedEntry {
            parent: self.parent.clone(),
            name: self.name.clone(),
            slot,
            holder,
        };
        let moved = match entry.slot.directory.entry(&entry.holder) {
            Ok(moved) => moved,
            Err(error) => {
                displaced.push(entry);
                return Err(error);
            }
        };
        let same = NativeDirectory::same(&moved, &self.directory.file);
        if matches!(same, Ok(true)) {
            match entry.slot.directory.remove(&entry.holder, true) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == ErrorKind::DirectoryNotEmpty => {}
                Err(error) => {
                    if entry.restore().is_err() {
                        displaced.push(entry);
                    }
                    return Err(error);
                }
            }
        }
        if let Err(error) = entry.restore() {
            displaced.push(entry);
            return Err(error);
        }
        same.map(|_| ())
    }
}

struct DisplacedEntry {
    parent: Arc<NativeDirectory>,
    name: std::ffi::OsString,
    slot: OwnedDirectory,
    holder: std::ffi::OsString,
}

impl DisplacedEntry {
    fn restore(&self) -> io::Result<()> {
        self.slot
            .directory
            .move_exclusive(&self.holder, &self.parent, &self.name)
    }
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
impl NativeDirectory {
    fn fd(&self) -> libc::c_int {
        self.file.as_raw_fd()
    }

    fn name(name: &std::ffi::OsStr) -> io::Result<std::ffi::CString> {
        std::ffi::CString::new(name.as_bytes()).map_err(io::Error::other)
    }

    fn from_fd(fd: libc::c_int) -> io::Result<Arc<Self>> {
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Arc::new(Self {
            file: unsafe { std::fs::File::from_raw_fd(fd) },
        }))
    }

    fn open(path: &Path) -> io::Result<Arc<Self>> {
        let path = Self::name(path.as_os_str())?;
        Self::from_fd(unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        })
    }

    fn child(&self, name: &std::ffi::OsStr, scoped: bool) -> io::Result<Arc<Self>> {
        let name = Self::name(name)?;
        let flags = libc::O_RDONLY
            | libc::O_DIRECTORY
            | libc::O_CLOEXEC
            | if scoped { libc::O_NOFOLLOW } else { 0 };
        Self::from_fd(unsafe { libc::openat(self.fd(), name.as_ptr(), flags) })
    }

    fn entry(&self, name: &std::ffi::OsStr) -> io::Result<std::fs::File> {
        let name = Self::name(name)?;
        #[cfg(target_os = "linux")]
        let flags = libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        #[cfg(not(target_os = "linux"))]
        let flags = libc::O_RDONLY | libc::O_SYMLINK | libc::O_NONBLOCK | libc::O_CLOEXEC;
        let fd = unsafe { libc::openat(self.fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { std::fs::File::from_raw_fd(fd) })
    }

    fn private(
        parent: Arc<Self>,
        unverified: &mut Vec<UnverifiedDirectory>,
        private: &Arc<Mutex<Vec<PrivateCleanup>>>,
    ) -> io::Result<OwnedDirectory> {
        for _ in 0..8 {
            let name =
                std::ffi::OsString::from(format!(".vmux-link-{}", uuid::Uuid::new_v4().simple()));
            let encoded = Self::name(&name)?;
            if unsafe { libc::mkdirat(parent.fd(), encoded.as_ptr(), 0o700) } != 0 {
                let error = io::Error::last_os_error();
                if error.kind() == ErrorKind::AlreadyExists {
                    continue;
                }
                return Err(error);
            }
            unverified.push(UnverifiedDirectory {
                parent: parent.clone(),
                name: name.clone(),
            });
            let directory = parent.child(&name, true)?;
            unverified.pop();
            return Ok(OwnedDirectory {
                parent,
                name,
                directory,
                private: true,
                cleanup: Arc::downgrade(private),
            });
        }
        Err(io::Error::other(
            "native link staging collisions exceeded limit",
        ))
    }

    fn move_exclusive(
        &self,
        name: &std::ffi::OsStr,
        target: &Self,
        destination: &std::ffi::OsStr,
    ) -> io::Result<()> {
        let name = Self::name(name)?;
        let destination = Self::name(destination)?;
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                self.fd(),
                name.as_ptr(),
                target.fd(),
                destination.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(not(target_os = "linux"))]
        let result = unsafe {
            libc::renameatx_np(
                self.fd(),
                name.as_ptr(),
                target.fd(),
                destination.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn remove(&self, name: &std::ffi::OsStr, directory: bool) -> io::Result<()> {
        let name = Self::name(name)?;
        if unsafe {
            libc::unlinkat(
                self.fd(),
                name.as_ptr(),
                if directory { libc::AT_REMOVEDIR } else { 0 },
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn read_link(&self, name: &std::ffi::OsStr) -> io::Result<PathBuf> {
        let name = Self::name(name)?;
        let mut bytes = vec![0u8; 8192];
        let length = unsafe {
            libc::readlinkat(
                self.fd(),
                name.as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        if length < 0 {
            return Err(io::Error::last_os_error());
        }
        if length as usize == bytes.len() {
            return Err(io::Error::other("native link exceeds8192 bytes"));
        }
        bytes.truncate(length as usize);
        Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
    }

    fn location(&self) -> io::Result<PathBuf> {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_link(format!("/proc/self/fd/{}", self.fd()))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let mut bytes = vec![0u8; libc::PATH_MAX as usize];
            if unsafe { libc::fcntl(self.fd(), libc::F_GETPATH, bytes.as_mut_ptr()) } != 0 {
                return Err(io::Error::last_os_error());
            }
            let length = bytes
                .iter()
                .position(|byte| *byte == 0)
                .ok_or_else(|| io::Error::other("native directory path exceeds limit"))?;
            bytes.truncate(length);
            Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
        }
    }

    fn same(left: &std::fs::File, right: &std::fs::File) -> io::Result<bool> {
        let left = left.metadata()?;
        let right = right.metadata()?;
        Ok(left.dev() == right.dev() && left.ino() == right.ino())
    }

    fn link(&self, source: &Path, name: &std::ffi::OsStr) -> io::Result<()> {
        let source = Self::name(source.as_os_str())?;
        let name = Self::name(name)?;
        if unsafe { libc::symlinkat(source.as_ptr(), self.fd(), name.as_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux")))]
impl NativeDirectory {
    fn unsupported() -> io::Error {
        io::Error::new(
            ErrorKind::Unsupported,
            "descriptor-scoped link operations are unsupported",
        )
    }
    fn open(_: &Path) -> io::Result<Arc<Self>> {
        Err(Self::unsupported())
    }
    fn child(&self, _: &std::ffi::OsStr, _: bool) -> io::Result<Arc<Self>> {
        Err(Self::unsupported())
    }
    fn entry(&self, _: &std::ffi::OsStr) -> io::Result<std::fs::File> {
        Err(Self::unsupported())
    }
    fn private(
        _: Arc<Self>,
        _: &mut Vec<UnverifiedDirectory>,
        _: &Arc<Mutex<Vec<PrivateCleanup>>>,
    ) -> io::Result<OwnedDirectory> {
        Err(Self::unsupported())
    }
    fn move_exclusive(&self, _: &std::ffi::OsStr, _: &Self, _: &std::ffi::OsStr) -> io::Result<()> {
        Err(Self::unsupported())
    }
    fn remove(&self, _: &std::ffi::OsStr, _: bool) -> io::Result<()> {
        Err(Self::unsupported())
    }
    fn read_link(&self, _: &std::ffi::OsStr) -> io::Result<PathBuf> {
        Err(Self::unsupported())
    }
    fn location(&self) -> io::Result<PathBuf> {
        Err(Self::unsupported())
    }
    fn same(_: &std::fs::File, _: &std::fs::File) -> io::Result<bool> {
        Err(Self::unsupported())
    }
    fn link(&self, _: &Path, _: &std::ffi::OsStr) -> io::Result<()> {
        Err(Self::unsupported())
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn isolated_link_sources_refuse_package_and_ancestor_symlinks_and_late_replacement() {
        for kind in ["package", "ancestor", "late", "missing"] {
            let directory = tempfile::tempdir().unwrap();
            let copied = directory.path().join("copied");
            let packages = copied.join("config/tools/dotfiles");
            let package = packages.join("shell");
            let live = directory.path().join("live/shell");
            std::fs::create_dir_all(&live).unwrap();
            std::fs::create_dir_all(copied.join("home")).unwrap();
            std::fs::write(live.join(".shell"), b"live content").unwrap();
            if kind == "ancestor" {
                std::fs::create_dir_all(packages.parent().unwrap()).unwrap();
                std::os::unix::fs::symlink(live.parent().unwrap(), &packages).unwrap();
            } else if kind == "package" {
                std::fs::create_dir_all(&packages).unwrap();
                std::os::unix::fs::symlink(&live, &package).unwrap();
            } else {
                std::fs::create_dir_all(&package).unwrap();
                std::fs::write(package.join(".shell"), b"copied content").unwrap();
            }
            let source = package.join(".shell");
            let target = copied.join("home/.shell");
            let request = LinkPath {
                source: source.clone(),
                source_may_be_missing: false,
                target: target.clone(),
                boundary: Some(copied),
                receipt: PathReceipt::default(),
            };
            if kind == "late" {
                std::fs::rename(&package, packages.join("retained-shell")).unwrap();
                std::os::unix::fs::symlink(&live, &package).unwrap();
            } else if kind == "missing" {
                let retained = directory.path().join("retained-packages");
                std::fs::rename(&packages, retained).unwrap();
                let empty = directory.path().join("empty-live");
                std::fs::create_dir(&empty).unwrap();
                std::os::unix::fs::symlink(empty, &packages).unwrap();
            }
            assert!(request.run().is_err(), "{kind}");
            assert!(target.symlink_metadata().is_err());
            assert_eq!(std::fs::read(live.join(".shell")).unwrap(), b"live content");
            let ordinary = directory.path().join("ordinary/.shell");
            LinkPath {
                source,
                source_may_be_missing: false,
                target: ordinary.clone(),
                boundary: None,
                receipt: PathReceipt::default(),
            }
            .run()
            .unwrap();
            if kind == "missing" {
                assert!(
                    ordinary
                        .symlink_metadata()
                        .unwrap()
                        .file_type()
                        .is_symlink()
                );
            } else {
                assert_eq!(std::fs::read(ordinary).unwrap(), b"live content");
            }
        }
    }

    #[test]
    fn isolated_unlink_and_restoration_keep_the_owned_link_when_its_source_vanished() {
        let directory = tempfile::tempdir().unwrap();
        let copied = directory.path().join("copied");
        let package = copied.join("packages/shell");
        let source = package.join("file");
        let target = copied.join("home/file");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(&source, b"copied").unwrap();
        let receipt = PathReceipt::default();
        LinkPath {
            source: source.clone(),
            source_may_be_missing: false,
            target: target.clone(),
            boundary: Some(copied.clone()),
            receipt: receipt.clone(),
        }
        .run()
        .unwrap();
        std::fs::remove_file(&source).unwrap();
        std::fs::remove_dir(&package).unwrap();
        let removal = PathReceipt::default();
        assert!(
            UnlinkPath {
                source: source.clone(),
                target: target.clone(),
                boundary: Some(copied.clone()),
                receipt: removal.clone()
            }
            .run()
            .unwrap()
            .0
        );
        CleanupPath { receipt: removal }.run().unwrap();
        let restore = PathReceipt::default();
        LinkPath {
            source: source.clone(),
            source_may_be_missing: true,
            target: target.clone(),
            boundary: Some(copied.clone()),
            receipt: restore.clone(),
        }
        .run()
        .unwrap();
        let relative = std::fs::read_link(&target).unwrap();
        assert_eq!(
            vmux_path::PathIdentity::lexical(target.parent().unwrap().join(relative)),
            vmux_path::PathIdentity::lexical(&source)
        );
        let empty = directory.path().join("foreign-empty");
        std::fs::create_dir(&empty).unwrap();
        std::fs::remove_file(&target).unwrap();
        std::fs::remove_dir(copied.join("packages")).unwrap();
        std::os::unix::fs::symlink(empty, copied.join("packages")).unwrap();
        assert!(
            LinkPath {
                source,
                source_may_be_missing: true,
                target: target.clone(),
                boundary: Some(copied),
                receipt: PathReceipt::default()
            }
            .run()
            .is_err()
        );
        assert!(target.symlink_metadata().is_err());
    }

    #[test]
    fn private_directory_drop_defers_native_removal_until_cleanup_acknowledges_it() {
        let directory = tempfile::tempdir().unwrap();
        let parent = NativeDirectory::open(directory.path()).unwrap();
        let receipt = PathReceipt::default();
        let slot = receipt.private(parent).unwrap();
        let path = slot.directory.location().unwrap();
        drop(slot);
        assert!(path.is_dir());
        assert!(receipt.pending());
        CleanupPath {
            receipt: receipt.clone(),
        }
        .run()
        .unwrap();
        assert!(!path.exists());
        assert!(!receipt.pending());
    }

    #[test]
    fn failed_private_cleanup_retains_foreign_content_and_recovers_without_losing_ownership() {
        let directory = tempfile::tempdir().unwrap();
        let parent = NativeDirectory::open(directory.path()).unwrap();
        let receipt = PathReceipt::default();
        let slot = receipt.private(parent).unwrap();
        let path = slot.directory.location().unwrap();
        std::fs::write(path.join("sentinel"), b"foreign content").unwrap();
        drop(slot);
        assert!(
            CleanupPath {
                receipt: receipt.clone()
            }
            .run()
            .is_err()
        );
        assert!(receipt.pending());
        assert_eq!(
            std::fs::read(path.join("sentinel")).unwrap(),
            b"foreign content"
        );
        std::fs::remove_file(path.join("sentinel")).unwrap();
        CleanupPath {
            receipt: receipt.clone(),
        }
        .run()
        .unwrap();
        assert!(!path.exists());
        assert!(!receipt.pending());
    }

    #[test]
    fn failed_private_directory_open_retains_an_unverified_remnant_and_preserves_replacement() {
        if std::env::var_os("VMUX_LINK_DESCRIPTOR_PRESSURE_CHILD").is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "io::links::tests::failed_private_directory_open_retains_an_unverified_remnant_and_preserves_replacement",
                    "--test-threads=1",
                ])
                .env("VMUX_LINK_DESCRIPTOR_PRESSURE_CHILD", "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let parent = NativeDirectory::open(directory.path()).unwrap();
        let receipt = PathReceipt::default();
        let mut previous = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut previous) },
            0
        );
        let limited = libc::rlimit {
            rlim_cur: previous.rlim_cur.min(64),
            rlim_max: previous.rlim_max,
        };
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limited) }, 0);
        let mut files = Vec::new();
        let mut exhausted = false;
        for _ in 0..256 {
            match std::fs::File::open("/dev/null") {
                Ok(file) => files.push(file),
                Err(error) => {
                    exhausted = error.raw_os_error() == Some(libc::EMFILE);
                    break;
                }
            }
        }
        let result = receipt.private(parent);
        drop(files);
        assert_eq!(
            unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &previous) },
            0
        );
        assert!(exhausted);
        assert!(result.is_err());
        assert!(receipt.pending());
        let entries: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(entries.len(), 1);
        let retained = directory.path().join("explicit-disposition");
        std::fs::rename(&entries[0], &retained).unwrap();
        std::fs::create_dir(&entries[0]).unwrap();
        std::fs::write(entries[0].join("sentinel"), b"foreign replacement").unwrap();
        assert!(
            CleanupPath {
                receipt: receipt.clone()
            }
            .run()
            .is_err()
        );
        assert_eq!(
            std::fs::read(entries[0].join("sentinel")).unwrap(),
            b"foreign replacement"
        );
        assert!(receipt.pending());
        std::fs::remove_file(entries[0].join("sentinel")).unwrap();
        std::fs::remove_dir(&entries[0]).unwrap();
        assert!(
            CleanupPath {
                receipt: receipt.clone()
            }
            .run()
            .is_ok()
        );
        assert!(!receipt.pending());
        assert!(retained.is_dir());
    }

    struct Fixture {
        directory: tempfile::TempDir,
        source: PathBuf,
        target: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("source");
            std::fs::write(&source, b"managed input").unwrap();
            let target = directory.path().join("home/.config/editor");
            Self {
                directory,
                source,
                target,
            }
        }

        fn link(&self) -> LinkPath {
            LinkPath {
                source_may_be_missing: false,
                source: self.source.clone(),
                target: self.target.clone(),
                boundary: Some(self.directory.path().to_path_buf()),
                receipt: PathReceipt::default(),
            }
        }

        fn unlink(&self) -> UnlinkPath {
            UnlinkPath {
                source: self.source.clone(),
                target: self.target.clone(),
                boundary: Some(self.directory.path().to_path_buf()),
                receipt: PathReceipt::default(),
            }
        }
    }

    #[test]
    fn rooted_link_is_idempotent_and_unlink_removes_only_the_owned_link() {
        let fixture = Fixture::new();
        let link = fixture.link();
        let receipt = link.receipt.clone();
        assert!(link.run().unwrap().0);
        assert_eq!(std::fs::read(&fixture.target).unwrap(), b"managed input");
        assert!(!fixture.link().run().unwrap().0);
        let unlink = fixture.unlink();
        let removal = unlink.receipt.clone();
        assert!(unlink.run().unwrap().0);
        CleanupPath { receipt: removal }.run().unwrap();
        CleanupPath { receipt }.run().unwrap();
        assert!(!fixture.target.parent().unwrap().exists());
    }

    #[test]
    fn copied_parent_symlink_cannot_redirect_link_or_unlink_to_live_files() {
        let fixture = Fixture::new();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(fixture.directory.path().join("home")).unwrap();
        std::os::unix::fs::symlink(
            outside.path(),
            fixture.directory.path().join("home/.config"),
        )
        .unwrap();
        assert!(fixture.link().run().is_err());
        assert!(!outside.path().join("editor").exists());
        std::os::unix::fs::symlink(&fixture.source, outside.path().join("editor")).unwrap();
        assert!(fixture.unlink().run().is_err());
        assert_eq!(
            std::fs::read(outside.path().join("editor")).unwrap(),
            b"managed input"
        );
    }

    #[test]
    fn replacement_between_identity_capture_and_native_take_is_restored_without_deletion() {
        let fixture = Fixture::new();
        let link = fixture.link();
        let receipt = link.receipt.clone();
        link.run().unwrap();
        receipt.commit();
        let unlink = fixture.unlink();
        let pair = LinkPaths::new(
            unlink.source.clone(),
            unlink.target.clone(),
            unlink.boundary.clone(),
        )
        .unwrap();
        let parent = pair.parent(false, &unlink.receipt).unwrap();
        let name = pair.name().unwrap();
        let original = parent.entry(&name).unwrap();
        std::fs::remove_file(&fixture.target).unwrap();
        std::fs::write(&fixture.target, b"foreign replacement").unwrap();
        assert!(!unlink.remove(pair, parent, name, original).unwrap().0);
        assert_eq!(
            std::fs::read(&fixture.target).unwrap(),
            b"foreign replacement"
        );
    }

    #[test]
    fn interrupted_parent_creation_cleans_owned_empty_directories_and_keeps_foreign_content() {
        for populated in [false, true] {
            let fixture = Fixture::new();
            let receipt = PathReceipt::default();
            let pair = LinkPaths::new(
                fixture.source.clone(),
                fixture.target.clone(),
                Some(fixture.directory.path().to_path_buf()),
            )
            .unwrap();
            let parent = pair.parent(true, &receipt).unwrap();
            if populated {
                std::fs::write(
                    fixture.target.parent().unwrap().join("foreign"),
                    b"preserve",
                )
                .unwrap();
            }
            drop(parent);
            CleanupPath { receipt }.run().unwrap();
            if populated {
                assert_eq!(
                    std::fs::read(fixture.target.parent().unwrap().join("foreign")).unwrap(),
                    b"preserve"
                );
            } else {
                assert!(!fixture.directory.path().join("home").exists());
            }
        }
    }

    #[test]
    fn blocked_foreign_restore_retains_both_entries_and_recovers_after_the_obstruction_is_removed()
    {
        let fixture = Fixture::new();
        let receipt = PathReceipt::default();
        let pair = LinkPaths::new(
            fixture.source.clone(),
            fixture.target.clone(),
            Some(fixture.directory.path().to_path_buf()),
        )
        .unwrap();
        let parent = pair.parent(true, &receipt).unwrap();
        std::fs::write(&fixture.target, b"first foreign").unwrap();
        let slot = receipt.private(parent.clone()).unwrap();
        let holder = std::ffi::OsString::from("entry");
        parent
            .move_exclusive(&pair.name().unwrap(), &slot.directory, &holder)
            .unwrap();
        std::fs::write(&fixture.target, b"second foreign").unwrap();
        let entry = DisplacedEntry {
            parent,
            name: pair.name().unwrap(),
            slot,
            holder,
        };
        let retained = entry.slot.directory.location().unwrap().join("entry");
        assert!(entry.restore().is_err());
        receipt.displaced(entry);
        assert!(
            CleanupPath {
                receipt: receipt.clone()
            }
            .run()
            .is_err()
        );
        assert_eq!(std::fs::read(&retained).unwrap(), b"first foreign");
        assert_eq!(std::fs::read(&fixture.target).unwrap(), b"second foreign");
        std::fs::remove_file(&fixture.target).unwrap();
        receipt.commit();
        CleanupPath { receipt }.run().unwrap();
        assert_eq!(std::fs::read(&fixture.target).unwrap(), b"first foreign");
        assert!(!retained.exists());
    }
}
