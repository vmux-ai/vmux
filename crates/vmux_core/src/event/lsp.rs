mod host;
mod ui;

use super::FileLine;

pub use host::*;
pub use ui::*;

#[vmux_api::contract(Copy, Eq)]
pub enum DiagSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[vmux_api::contract(Eq)]
pub struct FileDiagnostic {
    pub line: u32,
    pub start_col: u32,
    pub end_col: u32,
    pub severity: DiagSeverity,
    pub message: String,
    pub source: Option<String>,
}

#[vmux_api::contract(Copy, Eq)]
pub enum LspServerState {
    Missing,
    Starting,
    Ready,
}

#[vmux_api::contract(Copy, Eq)]
pub enum LspPkgStatus {
    Available,
    OnPath,
    Installing,
    Installed,
    Outdated,
    Running,
    Failed,
}

#[vmux_api::contract(Eq)]
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

#[vmux_api::contract(Copy, Eq)]
pub enum InstallPhase {
    Resolving,
    Downloading,
    Extracting,
    Linking,
    Done,
    Failed,
}

#[vmux_api::contract]
pub struct HoverBlock {
    pub code: bool,
    pub text: String,
    pub lines: Vec<FileLine>,
}

#[vmux_api::contract(Copy, Eq)]
pub enum EditorCapability {
    GotoDeclaration,
    GotoTypeDefinition,
    GotoImplementation,
    Rename,
    FormatDocument,
    FormatSelection,
    CodeAction,
}

#[vmux_api::contract(Eq)]
pub struct RefItem {
    pub path: String,
    pub display: String,
    pub line: u32,
    pub col: u32,
    pub preview: String,
}

#[vmux_api::contract(Eq)]
pub struct CompletionItem {
    pub label: String,
    pub insert_text: String,
    pub detail: String,
    pub kind: String,
}
