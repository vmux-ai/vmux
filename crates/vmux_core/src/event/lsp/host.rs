use super::{
    CompletionItem, EditorAction, FileDiagnostic, HoverBlock, InstallPhase, LspPackage,
    LspPkgStatus, LspServerState, RefItem,
};

#[vmux_api::payload(Eq)]
pub struct FileDiagnosticsEvent {
    pub path: String,
    pub diagnostics: Vec<FileDiagnostic>,
}

#[vmux_api::payload(Eq)]
pub struct FileLspStatusEvent {
    pub path: String,
    pub server: String,
    pub package: Option<String>,
    pub state: LspServerState,
    pub actions: Vec<EditorAction>,
}

#[vmux_api::payload(Eq)]
#[vmux_api::host_event(target = "lsp")]
pub struct LspCatalogEvent {
    pub packages: Vec<LspPackage>,
}

#[vmux_api::payload(Eq)]
#[vmux_api::host_event(
    targets = ["files", "lsp"]
)]
pub struct LspInstallProgress {
    pub name: String,
    pub phase: InstallPhase,
    pub pct: Option<u8>,
    pub message: String,
}

#[vmux_api::payload(Eq)]
#[vmux_api::host_event(
    targets = ["files", "lsp"]
)]
pub struct LspPkgStatusEvent {
    pub name: String,
    pub status: LspPkgStatus,
    pub version: Option<String>,
}

#[vmux_api::payload]
pub struct FileHoverEvent {
    pub line: u32,
    pub col: u32,
    pub blocks: Vec<HoverBlock>,
}

#[vmux_api::payload(Eq)]
pub struct FileCodeActionsEvent {
    pub titles: Vec<String>,
}

#[vmux_api::payload(Eq)]
pub struct FileEditFailedEvent {
    pub reason: String,
}

#[vmux_api::payload(Eq)]
pub struct FileRenameBeginEvent {
    pub line: u32,
    pub col: u32,
    pub current: String,
}

#[vmux_api::payload(Eq)]
pub struct FileReferencesEvent {
    pub items: Vec<RefItem>,
}

#[vmux_api::payload(Eq)]
pub struct FileCompletionEvent {
    pub items: Vec<CompletionItem>,
    pub replace_from_col: u32,
    pub line: u32,
}
