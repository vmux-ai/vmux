use crate::ProjectId;

#[vmux_api::contract(Default, Eq)]
pub struct ProjectEntry {
    pub id: ProjectId,
    #[serde(default)]
    pub identity: String,
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
    pub repository: Option<String>,
    pub revision: u64,
}

impl ProjectEntry {
    pub fn identity_seed(&self) -> &str {
        if !self.identity.is_empty() {
            return &self.identity;
        }
        if !self.path.trim().is_empty() {
            return &self.path;
        }
        if !self.id.0.trim().is_empty() {
            return &self.id.0;
        }
        if !self.url.trim().is_empty() {
            return &self.url;
        }
        &self.name
    }
}

impl ProjectEntry {
    pub fn matches(&self, needle: &str) -> bool {
        needle.is_empty()
            || self.name.to_ascii_lowercase().contains(needle)
            || self.source.to_ascii_lowercase().contains(needle)
            || self.path.to_ascii_lowercase().contains(needle)
            || self.branch.to_ascii_lowercase().contains(needle)
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

#[vmux_api::contract(Default, Eq)]
pub struct ProjectDetailView {
    pub id: ProjectId,
    #[serde(default)]
    pub identity: String,
    pub name: String,
    pub path: String,
    pub choices: Vec<ProjectEntry>,
    pub choices_pending: bool,
    pub repository: Option<String>,
    pub revision: u64,
}

#[vmux_api::ui_state(Default, Eq, version = 5)]
pub struct ProjectUiState {
    pub view: ProjectView,
    pub collection_view: vmux_ecs::CollectionView,
}

#[cfg(host)]
impl ProjectUiState {
    pub fn choices(&self) -> Vec<ProjectEntry> {
        match &self.view {
            ProjectView::Catalog(catalog) => catalog.entries.clone(),
            ProjectView::Detail(detail) => detail.choices.clone(),
            ProjectView::Loading | ProjectView::Error(_) => Vec::new(),
        }
    }

    pub fn pending(mut self) -> Self {
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

#[vmux_api::contract(Eq)]
pub enum ProjectSource {
    New,
    Local(String),
    Github(String),
}

#[vmux_api::ui_event(Eq)]
pub struct ProjectSaveRequest {
    pub request_id: String,
    pub project: Option<ProjectId>,
    pub revision: u64,
    pub name: String,
    pub source: ProjectSource,
}

#[vmux_api::ui_event(Eq)]
pub struct ProjectFolderPickRequest {
    pub request_id: String,
    pub directory: String,
}

#[vmux_api::ui_state(Default, Eq)]
pub struct ProjectFormUiState {
    pub request_id: String,
    pub folder: Option<String>,
    pub pending: bool,
    pub error: Option<String>,
    pub saved: Option<ProjectId>,
}
