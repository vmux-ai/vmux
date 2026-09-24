use super::{
    CompletionItem, EditorAction, FileDiagnostic, HoverBlock, InstallPhase, LspPackage,
    LspPkgStatus, LspServerState, RefItem,
};

#[vmux_api::contract(Eq)]
pub struct FileDiagnosticsEvent {
    pub path: String,
    pub diagnostics: Vec<FileDiagnostic>,
}

#[vmux_api::contract(Eq)]
pub struct FileLspStatusEvent {
    pub path: String,
    pub server: String,
    pub package: Option<String>,
    pub state: LspServerState,
    pub actions: Vec<EditorAction>,
}

#[vmux_api::contract(Eq)]
pub struct LspCatalogEvent {
    pub packages: Vec<LspPackage>,
}

#[vmux_api::contract(Eq)]
pub struct LspInstallProgress {
    pub name: String,
    pub phase: InstallPhase,
    pub pct: Option<u8>,
    pub message: String,
}

#[vmux_api::contract(Eq)]
pub struct LspPkgStatusEvent {
    pub name: String,
    pub status: LspPkgStatus,
    pub version: Option<String>,
}

#[vmux_api::ui_state(Eq, target = "lsp")]
pub struct LspManagerStateEvent {
    pub packages: Vec<LspPackage>,
    pub progress: Vec<LspInstallProgress>,
    pub loading: bool,
}

impl Default for LspManagerStateEvent {
    fn default() -> Self {
        Self {
            packages: Vec::new(),
            progress: Vec::new(),
            loading: true,
        }
    }
}

#[vmux_api::contract]
pub struct FileHoverEvent {
    pub line: u32,
    pub col: u32,
    pub blocks: Vec<HoverBlock>,
}

#[vmux_api::contract(Eq)]
pub struct FileCodeActionsEvent {
    pub titles: Vec<String>,
}

#[vmux_api::contract(Eq)]
pub struct FileEditFailedEvent {
    pub reason: String,
}

#[vmux_api::contract(Eq)]
pub struct FileRenameBeginEvent {
    pub line: u32,
    pub col: u32,
    pub current: String,
}

#[vmux_api::contract(Eq)]
pub struct FileReferencesEvent {
    pub items: Vec<RefItem>,
}

#[vmux_api::contract(Eq)]
pub struct FileCompletionEvent {
    pub items: Vec<CompletionItem>,
    pub replace_from_col: u32,
    pub line: u32,
}
