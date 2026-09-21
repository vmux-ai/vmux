use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathIdentity(PathBuf);

impl PathIdentity {
    pub fn resolve(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        if let Ok(canonical) = path.canonicalize() {
            return Self(canonical);
        }

        if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
            && let Ok(canonical_parent) = parent.canonicalize()
        {
            return Self(canonical_parent.join(name));
        }
        Self(Self::normalize(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    fn normalize(path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => match normalized.components().next_back() {
                    Some(Component::Normal(_)) => {
                        normalized.pop();
                    }
                    Some(Component::ParentDir) | None if !path.is_absolute() => {
                        normalized.push("..");
                    }
                    _ => {}
                },
                _ => normalized.push(component.as_os_str()),
            }
        }
        normalized
    }

    fn through_existing_ancestor(path: &Path) -> Self {
        if let Ok(canonical) = path.canonicalize() {
            return Self(canonical);
        }

        let mut ancestor = path;
        let mut suffix = Vec::<OsString>::new();
        while let Some(parent) = ancestor.parent() {
            if let Some(name) = ancestor.file_name() {
                suffix.push(name.to_os_string());
            }
            if let Ok(mut canonical) = parent.canonicalize() {
                for component in suffix.iter().rev() {
                    canonical.push(component);
                }
                return Self(Self::normalize(&canonical));
            }
            ancestor = parent;
        }
        Self(Self::normalize(path))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopedPath(PathIdentity);

impl ScopedPath {
    pub fn resolve(
        root: impl AsRef<Path>,
        path: impl AsRef<Path>,
    ) -> Result<Self, ScopedPathError> {
        let path = path.as_ref();
        if path
            .components()
            .any(|component| component == Component::ParentDir)
        {
            return Err(ScopedPathError::ParentTraversal);
        }

        let root = PathIdentity::resolve(root);
        let candidate = if path.is_absolute() {
            PathIdentity::through_existing_ancestor(path)
        } else {
            PathIdentity::through_existing_ancestor(&root.as_path().join(path))
        };
        if !candidate.as_path().starts_with(root.as_path()) {
            return Err(ScopedPathError::OutsideRoot);
        }
        Ok(Self(candidate))
    }

    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0.into_path_buf()
    }
}

impl AsRef<Path> for ScopedPath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedPathError {
    ParentTraversal,
    OutsideRoot,
}

impl std::fmt::Display for ScopedPathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParentTraversal => formatter.write_str("path contains parent traversal"),
            Self::OutsideRoot => formatter.write_str("path is outside root"),
        }
    }
}

impl std::error::Error for ScopedPathError {}

impl AsRef<Path> for PathIdentity {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl From<&Path> for PathIdentity {
    fn from(path: &Path) -> Self {
        Self::resolve(path)
    }
}

impl From<PathBuf> for PathIdentity {
    fn from(path: PathBuf) -> Self {
        Self::resolve(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_leaf_uses_canonical_parent() {
        let directory = std::env::current_dir().unwrap();
        let parent = directory.canonicalize().unwrap();

        assert_eq!(
            PathIdentity::resolve(directory.join("missing")).as_path(),
            parent.join("missing")
        );
    }

    #[test]
    fn relative_parent_components_are_normalized() {
        assert_eq!(
            PathIdentity::normalize(Path::new("a/b/../c")),
            Path::new("a/c")
        );
        assert_eq!(
            PathIdentity::normalize(Path::new("../../a")),
            Path::new("../../a")
        );
    }

    #[test]
    fn scoped_path_accepts_relative_and_absolute_children() {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("child");
        let expected = directory.path().canonicalize().unwrap().join("child");

        assert_eq!(
            ScopedPath::resolve(directory.path(), "child")
                .unwrap()
                .as_path(),
            expected
        );
        assert_eq!(
            ScopedPath::resolve(directory.path(), &child)
                .unwrap()
                .as_path(),
            expected
        );
    }

    #[test]
    fn scoped_path_rejects_parent_traversal() {
        let directory = tempfile::tempdir().unwrap();

        assert_eq!(
            ScopedPath::resolve(directory.path(), "../outside"),
            Err(ScopedPathError::ParentTraversal)
        );
    }

    #[cfg(unix)]
    #[test]
    fn scoped_path_rejects_symlink_escape_for_missing_leaf() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();

        assert_eq!(
            ScopedPath::resolve(root.path(), "escape/missing"),
            Err(ScopedPathError::OutsideRoot)
        );
    }
}
