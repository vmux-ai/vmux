#[vmux_api::ui_event(Default, Eq)]
pub struct LspCatalogRequest {
    pub query: String,
    pub language: String,
    pub category: String,
    pub installed_only: bool,
    #[serde(default)]
    pub refresh: bool,
}

impl LspCatalogRequest {
    pub fn for_query(query: impl Into<String>, refresh: bool) -> Self {
        Self {
            query: query.into(),
            language: String::new(),
            category: String::new(),
            installed_only: false,
            refresh,
        }
    }
}

#[vmux_api::ui_event(Eq)]
pub struct LspInstallRequest {
    pub name: String,
}

#[vmux_api::ui_event(Eq)]
pub struct LspUninstallRequest {
    pub name: String,
}

#[vmux_api::ui_event(Eq)]
pub struct LspUpdateRequest {
    pub name: String,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct FileHoverRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct FileHoverDismissRequest;

#[vmux_api::ui_event(Copy, Eq)]
pub struct FileDefinitionRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::ui_event(Eq)]
pub struct FileRenameRequest {
    pub line: u32,
    pub col: u32,
    pub new_name: String,
}

#[vmux_api::ui_event(Eq)]
pub struct FileRenameDraftRequest {
    pub draft: String,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct FileRenameDismissRequest;

#[vmux_api::ui_event(Copy, Eq)]
pub struct FileCodeActionPick {
    pub index: u32,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct FileCodeActionMoveRequest {
    pub next: bool,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct FileCodeActionDismissRequest;

#[vmux_api::ui_event(Copy, Eq)]
pub struct FileReferencesRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct FileCompletionRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct FilePanelPick {
    pub index: u32,
}
