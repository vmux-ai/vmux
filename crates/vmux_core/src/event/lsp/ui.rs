use super::EditorAction;

#[vmux_api::payload(Default, Eq)]
#[vmux_api::ui_event(namespace = "lsp", name = "catalog_request", target = "lsp")]
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

#[vmux_api::payload(Eq)]
#[vmux_api::ui_event(
    namespace = "lsp",
    name = "install_request",
    targets = ["files", "lsp"]
)]
pub struct LspInstallRequest {
    pub name: String,
}

#[vmux_api::payload(Eq)]
#[vmux_api::ui_event(namespace = "lsp", name = "uninstall_request", target = "lsp")]
pub struct LspUninstallRequest {
    pub name: String,
}

#[vmux_api::payload(Eq)]
#[vmux_api::ui_event(namespace = "lsp", name = "update_request", target = "lsp")]
pub struct LspUpdateRequest {
    pub name: String,
}

#[vmux_api::payload(Copy, Eq)]
#[vmux_api::ui_event(namespace = "file", name = "hover_request", target = "files")]
pub struct FileHoverRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::payload(Copy, Eq)]
#[vmux_api::ui_event(namespace = "file", name = "definition_request", target = "files")]
pub struct FileDefinitionRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::payload(Eq)]
#[vmux_api::ui_event(namespace = "file", name = "rename_request", target = "files")]
pub struct FileRenameRequest {
    pub line: u32,
    pub col: u32,
    pub new_name: String,
}

#[vmux_api::payload(Copy, Eq)]
#[vmux_api::ui_event(namespace = "file", name = "code_action_pick", target = "files")]
pub struct FileCodeActionPick {
    pub index: u32,
}

#[vmux_api::payload(Copy, Eq)]
#[vmux_api::ui_event(namespace = "file", name = "editor_action", target = "files")]
pub struct FileEditorAction {
    pub action: EditorAction,
    pub line: u32,
    pub col: u32,
}

#[vmux_api::payload(Copy, Eq)]
#[vmux_api::ui_event(namespace = "file", name = "references_request", target = "files")]
pub struct FileReferencesRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::payload(Copy, Eq)]
#[vmux_api::ui_event(namespace = "file", name = "completion_request", target = "files")]
pub struct FileCompletionRequest {
    pub line: u32,
    pub col: u32,
}

#[vmux_api::payload(Eq)]
#[vmux_api::ui_event(namespace = "file", name = "goto_request", target = "files")]
pub struct FileGotoRequest {
    pub path: String,
    pub line: u32,
    pub col: u32,
}

#[vmux_api::payload(Eq)]
#[vmux_api::ui_event(namespace = "file", name = "completion_commit", target = "files")]
pub struct FileCompletionCommit {
    pub line: u32,
    pub replace_from_col: u32,
    pub text: String,
}
