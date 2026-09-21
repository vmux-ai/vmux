use serde::{Deserialize, Serialize};

use super::FileLine;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum DiagSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FileDiagnostic {
    pub line: u32,
    pub start_col: u32,
    pub end_col: u32,
    pub severity: DiagSeverity,
    pub message: String,
    pub source: Option<String>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "file", name = "diagnostics", target = "files")]
pub struct FileDiagnosticsEvent {
    pub path: String,
    pub diagnostics: Vec<FileDiagnostic>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum LspServerState {
    Missing,
    Starting,
    Ready,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "file", name = "lsp_status", target = "files")]
pub struct FileLspStatusEvent {
    pub path: String,
    pub server: String,
    pub package: Option<String>,
    pub state: LspServerState,
    pub actions: Vec<EditorAction>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum LspPkgStatus {
    Available,
    OnPath,
    Installing,
    Installed,
    Outdated,
    Running,
    Failed,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct LspPackage {
    pub name: String,
    pub description: String,
    pub languages: Vec<String>,
    pub categories: Vec<String>,
    pub status: LspPkgStatus,
    pub version: Option<String>,
    pub installable: bool,
    pub requires: Option<String>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "lsp", name = "catalog", target = "lsp")]
pub struct LspCatalogEvent {
    pub packages: Vec<LspPackage>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(
    namespace = "lsp",
    name = "install_request",
    targets = ["files", "lsp"]
)]
pub struct LspInstallRequest {
    pub name: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "lsp", name = "uninstall_request", target = "lsp")]
pub struct LspUninstallRequest {
    pub name: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "lsp", name = "update_request", target = "lsp")]
pub struct LspUpdateRequest {
    pub name: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum InstallPhase {
    Resolving,
    Downloading,
    Extracting,
    Linking,
    Done,
    Failed,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(
    namespace = "lsp",
    name = "install_progress",
    targets = ["files", "lsp"]
)]
pub struct LspInstallProgress {
    pub name: String,
    pub phase: InstallPhase,
    pub pct: Option<u8>,
    pub message: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(
    namespace = "lsp",
    name = "pkg_status",
    targets = ["files", "lsp"]
)]
pub struct LspPkgStatusEvent {
    pub name: String,
    pub status: LspPkgStatus,
    pub version: Option<String>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "file", name = "hover_request", target = "files")]
pub struct FileHoverRequest {
    pub line: u32,
    pub col: u32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HoverBlock {
    pub code: bool,
    pub text: String,
    pub lines: Vec<FileLine>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "file", name = "hover", target = "files")]
pub struct FileHoverEvent {
    pub line: u32,
    pub col: u32,
    pub blocks: Vec<HoverBlock>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "file", name = "definition_request", target = "files")]
pub struct FileDefinitionRequest {
    pub line: u32,
    pub col: u32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "file", name = "rename_request", target = "files")]
pub struct FileRenameRequest {
    pub line: u32,
    pub col: u32,
    pub new_name: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "file", name = "code_actions", target = "files")]
pub struct FileCodeActionsEvent {
    pub titles: Vec<String>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "file", name = "code_action_pick", target = "files")]
pub struct FileCodeActionPick {
    pub index: u32,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum EditorAction {
    GotoDeclaration,
    GotoTypeDefinition,
    GotoImplementation,
    Rename,
    FormatDocument,
    FormatSelection,
    Cut,
    Copy,
    Paste,
    ChangeAllOccurrences,
    CodeAction,
    CommandPalette,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "file", name = "editor_action", target = "files")]
pub struct FileEditorAction {
    pub action: EditorAction,
    pub line: u32,
    pub col: u32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "file", name = "edit_failed", target = "files")]
pub struct FileEditFailedEvent {
    pub reason: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "file", name = "rename_begin", target = "files")]
pub struct FileRenameBeginEvent {
    pub line: u32,
    pub col: u32,
    pub current: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "file", name = "references_request", target = "files")]
pub struct FileReferencesRequest {
    pub line: u32,
    pub col: u32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RefItem {
    pub path: String,
    pub display: String,
    pub line: u32,
    pub col: u32,
    pub preview: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "file", name = "references", target = "files")]
pub struct FileReferencesEvent {
    pub items: Vec<RefItem>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "file", name = "completion_request", target = "files")]
pub struct FileCompletionRequest {
    pub line: u32,
    pub col: u32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CompletionItem {
    pub label: String,
    pub insert_text: String,
    pub detail: String,
    pub kind: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "file", name = "completion", target = "files")]
pub struct FileCompletionEvent {
    pub items: Vec<CompletionItem>,
    pub replace_from_col: u32,
    pub line: u32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "file", name = "goto_request", target = "files")]
pub struct FileGotoRequest {
    pub path: String,
    pub line: u32,
    pub col: u32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "file", name = "completion_commit", target = "files")]
pub struct FileCompletionCommit {
    pub line: u32,
    pub replace_from_col: u32,
    pub text: String,
}
