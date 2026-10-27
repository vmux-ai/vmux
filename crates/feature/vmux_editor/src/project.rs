use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_api::Id;

#[vmux_api::contract(Copy, Default, Eq)]
#[derive(Component, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_editor"]
pub struct Project;

impl Project {
    pub fn bundle(
        id: Id<Self>,
        name: impl Into<String>,
        path: PathBuf,
        source: impl Into<String>,
        branch: impl Into<String>,
    ) -> Result<ProjectBundle, String> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("project name must not be empty".to_string());
        }
        Ok(ProjectBundle {
            project: Self,
            id,
            name: Name::new(name),
            path: ProjectPath::new(path)?,
            source: ProjectSource(source.into()),
            branch: ProjectBranch(branch.into()),
        })
    }
}

#[derive(Bundle)]
pub struct ProjectBundle {
    project: Project,
    id: Id<Project>,
    name: Name,
    path: ProjectPath,
    source: ProjectSource,
    branch: ProjectBranch,
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_editor"]
pub struct ProjectPath(PathBuf);

impl ProjectPath {
    pub fn new(path: PathBuf) -> Result<Self, String> {
        if !path.is_absolute() {
            return Err("project path must be absolute".to_string());
        }
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_editor"]
pub struct ProjectSource(String);

impl ProjectSource {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectBranch(String);

impl ProjectBranch {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = OwnedProjects)]
pub struct ProjectOwner(Entity);

impl ProjectOwner {
    pub const fn new(entity: Entity) -> Self {
        Self(entity)
    }

    pub const fn entity(&self) -> Entity {
        self.0
    }
}

#[derive(Component, Debug)]
#[relationship_target(relationship = ProjectOwner, linked_spawn)]
pub struct OwnedProjects(Vec<Entity>);

#[derive(Resource, Default)]
pub struct ProjectIndex {
    entities: HashMap<Id<Project>, Entity>,
    duplicates: HashSet<Id<Project>>,
}

impl ProjectIndex {
    pub fn entity(&self, id: &Id<Project>) -> Result<Option<Entity>, ProjectLookupError> {
        if self.duplicates.contains(id) {
            return Err(ProjectLookupError::Duplicate(id.clone()));
        }
        Ok(self.entities.get(id).copied())
    }

    pub(crate) fn include(&mut self, id: Id<Project>, entity: Entity) {
        if self.entities.insert(id.clone(), entity).is_some() {
            self.duplicates.insert(id);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectLookupError {
    Duplicate(Id<Project>),
}

impl std::fmt::Display for ProjectLookupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate(id) => write!(formatter, "duplicate project ID `{id}`"),
        }
    }
}

#[vmux_api::contract(Eq)]
pub struct ProjectEntry {
    pub id: Id<Project>,
    pub name: String,
    pub source: String,
    pub path: String,
    pub url: String,
    pub branch: String,
    pub base_ref: String,
    pub is_worktree: bool,
    pub uncommitted: u32,
    pub ahead: u32,
    pub ongoing_sessions: u32,
    pub stale_sessions: u32,
    pub archived: bool,
}

impl ProjectEntry {
    pub fn identity_seed(&self) -> &str {
        self.id.as_str()
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ProjectCatalogView {
    pub entries: Vec<ProjectEntry>,
    pub operation_pending: bool,
}

#[vmux_api::contract(Default, Eq)]
pub enum ProjectView {
    #[default]
    Loading,
    Catalog(ProjectCatalogView),
    Detail(ProjectDetailView),
    Error(String),
}

#[vmux_api::contract(Eq)]
pub struct ProjectDetailView {
    pub id: Id<Project>,
    pub name: String,
    pub path: String,
}

#[vmux_api::ui_state(Default, Eq, version = 3)]
pub struct UiProject {
    pub view: ProjectView,
    pub collection_view: vmux_ecs::CollectionView,
}

#[cfg(host)]
impl UiProject {
    pub(crate) fn pending(mut self) -> Self {
        if let ProjectView::Catalog(catalog) = &mut self.view {
            catalog.operation_pending = true;
        }
        self
    }
}

#[vmux_api::ui_event(Eq)]
pub struct ProjectCreateRequest {
    pub name: String,
}

#[vmux_api::ui_event]
pub struct ProjectImportRequest;

#[vmux_api::ui_event(Eq)]
pub struct ProjectCloneRequest {
    pub repository: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ProjectArchiveRequest {
    pub url: String,
    pub archived: bool,
}

#[vmux_api::ui_event(Eq)]
pub struct ProjectDeleteRequest {
    pub url: String,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct ProjectCollectionViewRequest {
    pub view: vmux_ecs::CollectionView,
}

pub struct ProjectRoute;

impl ProjectRoute {
    pub const ROOT: &'static str = "vmux://projects/";

    pub fn is_page(value: &str) -> bool {
        value == "vmux://projects" || value.starts_with(Self::ROOT)
    }

    pub fn is_root(value: &str) -> bool {
        value.trim_end_matches('/') == Self::ROOT.trim_end_matches('/')
    }
}

#[cfg(all(test, host))]
mod tests {
    use super::*;

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

#[cfg(host)]
impl ProjectRoute {
    pub(crate) fn route_path(
        value: &str,
        root: &std::path::Path,
    ) -> Option<Result<std::path::PathBuf, String>> {
        let root = match root.canonicalize() {
            Ok(root) => root,
            Err(error) => {
                return Some(Err(format!("projects directory is unavailable: {error}")));
            }
        };
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
