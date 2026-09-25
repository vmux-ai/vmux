use super::{
    CompletionItem, EditorCapability, FileDiagnostic, HoverBlock, InstallPhase, LspPackage,
    LspPkgStatus, LspServerState, RefItem,
};

#[vmux_api::contract(Eq)]
pub struct FileDiagnostics {
    pub path: String,
    pub diagnostics: Vec<FileDiagnostic>,
}

#[vmux_api::contract(Eq)]
pub struct FileLspStatus {
    pub path: String,
    pub server: String,
    pub package: Option<String>,
    pub state: LspServerState,
    pub capabilities: Vec<EditorCapability>,
}

#[vmux_api::contract(Eq)]
pub struct LspCatalog {
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
pub struct LspPackageStatus {
    pub name: String,
    pub status: LspPkgStatus,
    pub version: Option<String>,
}

#[vmux_api::ui_state(Eq, target = "lsp")]
pub struct LspManagerUiState {
    pub packages: Vec<LspPackage>,
    pub progress: Vec<LspInstallProgress>,
    pub loading: bool,
}

impl Default for LspManagerUiState {
    fn default() -> Self {
        Self {
            packages: Vec::new(),
            progress: Vec::new(),
            loading: true,
        }
    }
}

#[vmux_api::contract]
pub struct FileHover {
    pub line: u32,
    pub col: u32,
    pub blocks: Vec<HoverBlock>,
}

#[vmux_api::contract(Eq)]
pub struct FileCodeActions {
    pub titles: Vec<String>,
}

#[vmux_api::contract(Eq)]
pub struct FileEditFailure {
    pub reason: String,
}

#[vmux_api::contract(Eq)]
pub struct FileRenamePrompt {
    pub line: u32,
    pub col: u32,
    pub current: String,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub enum FilePanelFocusTarget {
    #[default]
    None,
    References,
    Editor,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub struct FilePanelFocus {
    pub revision: u64,
    pub target: FilePanelFocusTarget,
}

#[vmux_api::contract(Eq)]
pub enum FilePanelContent {
    References {
        items: Vec<RefItem>,
    },
    Completion {
        items: Vec<CompletionItem>,
        replace_from_col: u32,
        line: u32,
    },
}

#[vmux_api::contract(Eq, Default)]
pub struct FilePanelState {
    pub content: Option<FilePanelContent>,
    pub selected: u32,
    pub focus: FilePanelFocus,
}
