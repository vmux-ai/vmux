#[vmux_api::ui_event(Default, Eq, url = "vmux://lsp/")]
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

#[vmux_api::ui_event(Eq, urls = ["file://", "vmux://lsp/"])]
pub struct LspInstallRequest {
    pub name: String,
}

#[vmux_api::ui_event(Eq, url = "vmux://lsp/")]
pub struct LspUninstallRequest {
    pub name: String,
}

#[vmux_api::ui_event(Eq, url = "vmux://lsp/")]
pub struct LspUpdateRequest {
    pub name: String,
}

#[vmux_api::ui_event(Copy, Eq, url = "file://")]
pub struct FileHoverRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::ui_event(Copy, Eq, url = "file://")]
pub struct FileDefinitionRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::ui_event(Eq, url = "file://")]
pub struct FileRenameRequest {
    pub line: u32,
    pub col: u32,
    pub new_name: String,
}

#[vmux_api::ui_event(Copy, Eq, url = "file://")]
pub struct FileCodeActionPick {
    pub index: u32,
}

#[vmux_api::ui_event(Copy, Eq, url = "file://")]
pub struct FileReferencesRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::ui_event(Copy, Eq, url = "file://")]
pub struct FileCompletionRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::ui_event(Copy, Eq, url = "file://")]
pub struct FilePanelPick {
    pub index: u32,
}
