pub struct ProjectRoute;

impl ProjectRoute {
    #[cfg(feature = "filesystem")]
    pub fn attach(
        root: &std::path::Path,
        directory: &std::path::Path,
    ) -> Result<String, crate::Error> {
        std::fs::create_dir_all(root)
            .map_err(|error| crate::Error::Unavailable(error.to_string()))?;
        let root = root
            .canonicalize()
            .map_err(|error| crate::Error::Unavailable(error.to_string()))?;
        let selected = directory
            .canonicalize()
            .map_err(|error| crate::Error::Unavailable(error.to_string()))?;
        if !selected.is_dir() {
            return Err(crate::Error::InvalidName);
        }
        if let Some(url) = Self::from_path(&root, &selected) {
            return Ok(url);
        }
        let imports = root.join("imported");
        std::fs::create_dir_all(&imports)
            .map_err(|error| crate::Error::Unavailable(error.to_string()))?;
        let name = selected
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(crate::Error::InvalidName)?;
        for suffix in 1..=999 {
            let candidate = if suffix == 1 {
                imports.join(name)
            } else {
                imports.join(format!("{name}-{suffix}"))
            };
            if candidate.canonicalize().ok().as_deref() == Some(&selected) {
                return Self::from_path(&root, &candidate).ok_or(crate::Error::InvalidName);
            }
            if std::fs::symlink_metadata(&candidate).is_ok() {
                continue;
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(&selected, &candidate)
                .map_err(|error| crate::Error::Unavailable(error.to_string()))?;
            #[cfg(not(unix))]
            return Err(crate::Error::Unavailable(
                "directory attachments are unavailable on this platform".to_string(),
            ));
            return Self::from_path(&root, &candidate).ok_or(crate::Error::InvalidName);
        }
        Err(crate::Error::Duplicate)
    }

    pub const ROOT: &'static str = "vmux://projects/";

    pub fn is_page(value: &str) -> bool {
        value == "vmux://projects" || value.starts_with(Self::ROOT)
    }

    pub fn is_root(value: &str) -> bool {
        value.trim_end_matches('/') == Self::ROOT.trim_end_matches('/')
    }
}

#[cfg(all(test, feature = "filesystem", unix))]
mod tests {
    use super::*;

    #[test]
    fn directory_attachments_open_managed_external_and_worktree_paths() {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let managed = root.path().join("managed");
        std::fs::create_dir(&managed).unwrap();
        let worktree = external.path().join("worktree");
        std::fs::create_dir(&worktree).unwrap();
        std::fs::write(
            worktree.join(".git"),
            "gitdir: /tmp/repository/worktrees/test",
        )
        .unwrap();
        for path in [&managed, &worktree, &external.path().to_path_buf()] {
            let url = ProjectRoute::attach(root.path(), path).unwrap();
            assert_eq!(
                ProjectRoute::resolve(&url, root.path()).unwrap().unwrap(),
                path.canonicalize().unwrap()
            );
            assert_eq!(ProjectRoute::attach(root.path(), path).unwrap(), url);
        }
        assert_eq!(
            std::fs::read_dir(root.path().join("imported"))
                .unwrap()
                .count(),
            2
        );
    }

    #[test]
    fn route_round_trips_a_project_path() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("github.com/example/a project");
        std::fs::create_dir_all(&project).unwrap();

        let url = ProjectRoute::from_path(root.path(), &project).unwrap();
        let resolved = ProjectRoute::resolve(&url, root.path()).unwrap().unwrap();

        assert_eq!(url, "vmux://projects/github.com/example/a%20project");
        assert_eq!(resolved, project.canonicalize().unwrap());
    }

    #[test]
    fn route_rejects_parent_directory_segments() {
        let root = tempfile::tempdir().unwrap();

        let result = ProjectRoute::resolve("vmux://projects/%2E%2E", root.path())
            .unwrap()
            .unwrap_err();

        assert_eq!(result, "project URL contains an invalid path");
    }

    #[test]
    fn route_rejects_unmanaged_symlinks_outside_the_projects_root() {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(external.path(), root.path().join("external")).unwrap();

        let result = ProjectRoute::resolve("vmux://projects/external", root.path())
            .unwrap()
            .unwrap_err();

        assert_eq!(result, "project is unavailable");
    }
}

impl ProjectRoute {
    pub fn route_path(
        value: &str,
        root: &std::path::Path,
    ) -> Option<Result<std::path::PathBuf, String>> {
        let root = root.to_path_buf();
        if Self::is_root(value) {
            return Some(Ok(root));
        }
        let relative = value.strip_prefix(Self::ROOT)?;
        let relative = relative.split(['?', '#']).next().unwrap_or_default();
        if relative.is_empty() {
            return Some(Ok(root));
        }
        let mut path = root;
        for encoded in relative.split('/') {
            let Ok(decoded) = percent_encoding::percent_decode_str(encoded).decode_utf8() else {
                return Some(Err("project URL contains invalid UTF-8".to_string()));
            };
            let mut components = std::path::Path::new(decoded.as_ref()).components();
            let Some(std::path::Component::Normal(component)) = components.next() else {
                return Some(Err("project URL contains an invalid path".to_string()));
            };
            if components.next().is_some() {
                return Some(Err("project URL contains an invalid path".to_string()));
            }
            path.push(component);
        }
        Some(Ok(path))
    }

    #[cfg(feature = "filesystem")]
    pub fn resolve(
        value: &str,
        root: &std::path::Path,
    ) -> Option<Result<std::path::PathBuf, String>> {
        let root = match root.canonicalize() {
            Ok(root) => root,
            Err(error) => {
                return Some(Err(format!("projects directory is unavailable: {error}")));
            }
        };
        let path = match Self::route_path(value, &root)? {
            Ok(path) => path,
            Err(error) => return Some(Err(error)),
        };
        let canonical = match path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                return Some(Err(format!("project is unavailable: {error}")));
            }
        };
        let imported = path.parent() == Some(root.join("imported").as_path())
            && std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_symlink());
        if (!canonical.starts_with(&root) && !imported) || !path.is_dir() || !canonical.is_dir() {
            return Some(Err("project is unavailable".to_string()));
        }
        Some(Ok(canonical))
    }

    pub fn from_path(root: &std::path::Path, path: &std::path::Path) -> Option<String> {
        let relative = path.strip_prefix(root).ok()?;
        let mut url = url::Url::parse(Self::ROOT).ok()?;
        let mut segments = url.path_segments_mut().ok()?;
        segments.pop_if_empty();
        for component in relative.components() {
            let std::path::Component::Normal(component) = component else {
                return None;
            };
            segments.push(component.to_str()?);
        }
        drop(segments);
        Some(url.to_string())
    }
}
