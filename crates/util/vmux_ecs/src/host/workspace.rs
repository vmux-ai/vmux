use std::path::PathBuf;

use bevy::prelude::SystemSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceLocation {
    pub name: String,
    pub revision: String,
    pub working_directory: PathBuf,
    pub project_directory: PathBuf,
}

impl WorkspaceLocation {
    pub fn new(
        name: String,
        revision: String,
        working_directory: impl Into<PathBuf>,
        project_directory: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        if name.trim().is_empty() {
            return Err("workspace name is empty".to_string());
        }
        if revision.trim().is_empty() {
            return Err("workspace revision is empty".to_string());
        }
        Ok(Self {
            name,
            revision,
            working_directory: Self::directory(working_directory.into(), "working")?,
            project_directory: Self::directory(project_directory.into(), "project")?,
        })
    }

    fn directory(path: PathBuf, role: &str) -> Result<PathBuf, String> {
        if !path.is_absolute() {
            return Err(format!("{role} path is not absolute"));
        }
        let path = path
            .canonicalize()
            .map_err(|error| format!("invalid {role} directory: {error}"))?;
        if !path.is_dir() {
            return Err(format!("{role} path is not a directory"));
        }
        Ok(path)
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TabCommandSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StackCommandSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComputeFocusSet;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_location_resolves_existing_directories() {
        let project = tempfile::tempdir().unwrap();
        let working = project.path().join("working");
        std::fs::create_dir(&working).unwrap();

        let location = WorkspaceLocation::new(
            "workspace".to_string(),
            "feature/workspace".to_string(),
            &working,
            project.path(),
        )
        .unwrap();

        assert_eq!(location.working_directory, working.canonicalize().unwrap());
        assert_eq!(
            location.project_directory,
            project.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn workspace_location_rejects_missing_directories() {
        let project = tempfile::tempdir().unwrap();

        assert!(
            WorkspaceLocation::new(
                "workspace".to_string(),
                "feature/workspace".to_string(),
                project.path().join("missing"),
                project.path(),
            )
            .is_err()
        );
    }
}
