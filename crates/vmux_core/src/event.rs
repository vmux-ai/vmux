pub mod space;
pub mod team;

use serde::{Deserialize, Serialize};

pub const TERM_VIEWPORT_EVENT: &str = "term_viewport";
pub const TERM_KEY_EVENT: &str = "term_key";
pub const TERM_MOUSE_EVENT: &str = "term_mouse";
pub const TERM_RESIZE_EVENT: &str = "term_resize";

pub const TERM_THEME_EVENT: &str = "term_theme";
pub const TERM_TITLE_EVENT: &str = "term_title";
pub const TERM_LOADING_EVENT: &str = "term_loading";
pub const SERVICE_UNAVAILABLE_EVENT: &str = "service_unavailable";
pub const FILE_META_EVENT: &str = "file_meta";
pub const FILE_VIEWPORT_EVENT: &str = "file_viewport";
pub const FILE_ERROR_EVENT: &str = "file_error";
pub const FILE_RESIZE_EVENT: &str = "file_resize";
pub const FILE_SCROLL_EVENT: &str = "file_scroll";
pub const FILE_DIR_EVENT: &str = "file_dir";
pub const FILE_THEME_EVENT: &str = "file_theme";
pub const FILE_PREVIEW_REQUEST_EVENT: &str = "file_preview_request";
pub const FILE_PREVIEW_EVENT: &str = "file_preview";
pub const FILE_OPEN_EVENT: &str = "file_open";
pub const FILE_IMAGE_EVENT: &str = "file_image";
pub const FILE_DIAGNOSTICS_EVENT: &str = "file_diagnostics";
pub const LSP_CATALOG_REQUEST: &str = "lsp_catalog_request";
pub const LSP_CATALOG_EVENT: &str = "lsp_catalog";
pub const LSP_INSTALL_REQUEST: &str = "lsp_install_request";
pub const LSP_UNINSTALL_REQUEST: &str = "lsp_uninstall_request";
pub const LSP_UPDATE_REQUEST: &str = "lsp_update_request";
pub const LSP_INSTALL_PROGRESS_EVENT: &str = "lsp_install_progress";
pub const LSP_PKG_STATUS_EVENT: &str = "lsp_pkg_status";
pub const TERMINAL_PAGE_URL: &str = "vmux://terminal/";

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
pub struct StyledSpan {
    pub text: String,
    pub fg: [u8; 3],
    pub bold: bool,
    pub italic: bool,
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
pub struct FileLine {
    pub line_no: u32,
    pub spans: Vec<StyledSpan>,
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
pub struct FileMetaEvent {
    pub path: String,
    pub abs_path: String,
    pub language: String,
    pub total_lines: u32,
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
pub struct FileViewportPatch {
    pub first_line: u32,
    pub total_lines: u32,
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
pub struct FileErrorEvent {
    pub message: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Default,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FileResizeEvent {
    pub char_height: f32,
    pub viewport_height: f32,
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
pub struct FileScrollEvent {
    pub top_line: u32,
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
pub struct FileDirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
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
pub struct FileDirEvent {
    pub path: String,
    pub abs_path: String,
    pub entries: Vec<FileDirEntry>,
    pub parent_path: String,
    pub parent_entries: Vec<FileDirEntry>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Default,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FileThemeEvent {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
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
pub struct FilePreviewRequest {
    pub path: String,
    pub thumb: bool,
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
pub enum PreviewKind {
    Dir(Vec<FileDirEntry>),
    Text(Vec<FileLine>),
    Image {
        mime: String,
        bytes: Vec<u8>,
    },
    Info {
        size: u64,
        modified: String,
        kind: String,
    },
    Error(String),
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
pub struct FilePreviewEvent {
    pub path: String,
    pub thumb: bool,
    pub kind: PreviewKind,
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
pub struct FileOpenEvent {
    pub path: String,
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
pub struct FileImageEvent {
    pub mime: String,
    pub bytes: Vec<u8>,
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
    /// 0-based absolute line number.
    pub line: u32,
    /// Char index within the line (NOT UTF-16, NOT byte).
    pub start_col: u32,
    /// Char index within the line, exclusive.
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
    /// Toolchain required when not installable directly, e.g. "node".
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
pub struct LspCatalogRequest {
    pub query: String,
    pub language: String,
    pub category: String,
    pub installed_only: bool,
    #[serde(default)]
    pub refresh: bool,
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
pub struct LspPkgStatusEvent {
    pub name: String,
    pub status: LspPkgStatus,
    pub version: Option<String>,
}

#[cfg(test)]
mod file_event_tests {
    use super::*;

    #[test]
    fn file_viewport_patch_rkyv_roundtrip() {
        let patch = FileViewportPatch {
            first_line: 100,
            total_lines: 5000,
            lines: vec![FileLine {
                line_no: 100,
                spans: vec![StyledSpan {
                    text: "fn main() {".into(),
                    fg: [220, 220, 170],
                    bold: false,
                    italic: false,
                }],
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&patch).expect("ser");
        let decoded =
            rkyv::from_bytes::<FileViewportPatch, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded.first_line, 100);
        assert_eq!(decoded.total_lines, 5000);
        assert_eq!(decoded.lines[0].line_no, 100);
        assert_eq!(decoded.lines[0].spans[0].text, "fn main() {");
        assert_eq!(decoded.lines[0].spans[0].fg, [220, 220, 170]);
    }

    #[test]
    fn file_scroll_and_resize_roundtrip() {
        let s = FileScrollEvent { top_line: 42 };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&s).unwrap();
        assert_eq!(
            rkyv::from_bytes::<FileScrollEvent, rkyv::rancor::Error>(&b)
                .unwrap()
                .top_line,
            42
        );
        let r = FileResizeEvent {
            char_height: 16.0,
            viewport_height: 480.0,
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let d = rkyv::from_bytes::<FileResizeEvent, rkyv::rancor::Error>(&b).unwrap();
        assert_eq!(d.char_height, 16.0);
        assert_eq!(d.viewport_height, 480.0);
    }

    #[test]
    fn preview_kind_rkyv_roundtrip() {
        let k = PreviewKind::Image {
            mime: "image/png".into(),
            bytes: vec![1, 2, 3],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&k).unwrap();
        let back = rkyv::from_bytes::<PreviewKind, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(k, back);
    }

    #[test]
    fn file_dir_event_has_parent_fields() {
        let e = FileDirEvent {
            path: "/a/b".into(),
            abs_path: "/a/b".into(),
            entries: vec![],
            parent_path: "/a".into(),
            parent_entries: vec![],
        };
        assert_eq!(e.parent_path, "/a");
    }

    #[test]
    fn file_diagnostics_event_rkyv_roundtrip() {
        let ev = FileDiagnosticsEvent {
            path: "/src/main.rs".into(),
            diagnostics: vec![FileDiagnostic {
                line: 3,
                start_col: 4,
                end_col: 9,
                severity: DiagSeverity::Error,
                message: "cannot find value `x`".into(),
                source: Some("rustc".into()),
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&ev).expect("ser");
        let back =
            rkyv::from_bytes::<FileDiagnosticsEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(back.path, "/src/main.rs");
        assert_eq!(back.diagnostics.len(), 1);
        assert_eq!(back.diagnostics[0].line, 3);
        assert_eq!(back.diagnostics[0].end_col, 9);
        assert_eq!(back.diagnostics[0].severity, DiagSeverity::Error);
        assert_eq!(back.diagnostics[0].source.as_deref(), Some("rustc"));
    }

    #[test]
    fn lsp_catalog_event_rkyv_roundtrip() {
        let ev = LspCatalogEvent {
            packages: vec![LspPackage {
                name: "rust-analyzer".into(),
                description: "Rust LSP".into(),
                languages: vec!["rust".into()],
                categories: vec!["LSP".into()],
                status: LspPkgStatus::Available,
                version: None,
                installable: true,
                requires: None,
            }],
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&ev).unwrap();
        let d = rkyv::from_bytes::<LspCatalogEvent, rkyv::rancor::Error>(&b).unwrap();
        assert_eq!(d.packages[0].name, "rust-analyzer");
        assert_eq!(d.packages[0].status, LspPkgStatus::Available);
        assert!(d.packages[0].installable);
    }

    #[test]
    fn lsp_install_progress_rkyv_roundtrip() {
        let p = LspInstallProgress {
            name: "gopls".into(),
            phase: InstallPhase::Downloading,
            pct: Some(42),
            message: "downloading".into(),
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&p).unwrap();
        let d = rkyv::from_bytes::<LspInstallProgress, rkyv::rancor::Error>(&b).unwrap();
        assert_eq!(d.name, "gopls");
        assert_eq!(d.phase, InstallPhase::Downloading);
        assert_eq!(d.pct, Some(42));
    }
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
pub struct ServiceUnavailableEvent {
    pub message: String,
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum TermColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct TermThemeEvent {
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    pub cursor: [u8; 3],
    pub ansi: [[u8; 3]; 16],
    #[serde(default)]
    pub font_family: String,
    #[serde(default)]
    pub font_size: f32,
    #[serde(default)]
    pub line_height: f32,
    #[serde(default)]
    pub padding: f32,
    #[serde(default)]
    pub cursor_style: String,
    #[serde(default)]
    pub cursor_blink: bool,
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
pub struct TermLoadingEvent {
    pub loading: bool,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermViewportEvent {
    pub lines: Vec<TermLine>,
    pub cursor: TermCursor,
    pub cols: u16,
    pub rows: u16,
    pub title: Option<String>,
    #[serde(default)]
    pub copy_mode: bool,
    #[serde(default)]
    pub selection: Option<TermSelectionRange>,
}

/// Range of selected cells in viewport coordinates (0-based row/col).
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
pub struct TermSelectionRange {
    pub start_col: u16,
    pub start_row: u16,
    pub end_col: u16,
    pub end_row: u16,
    pub is_block: bool,
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TermLine {
    pub spans: Vec<TermSpan>,
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TermSpan {
    pub text: String,
    pub fg: TermColor,
    pub bg: TermColor,
    pub flags: u16,
    /// Starting column index of this span in the row (0-based).
    #[serde(default)]
    pub col: u16,
    /// Number of grid columns this span covers (accounts for wide characters
    /// taking 2 columns).  When 0 (legacy), falls back to `text.chars().count()`.
    #[serde(default)]
    pub grid_cols: u16,
}

pub const FLAG_BOLD: u16 = 1;
pub const FLAG_ITALIC: u16 = 2;
pub const FLAG_UNDERLINE: u16 = 4;
pub const FLAG_STRIKETHROUGH: u16 = 8;
pub const FLAG_DIM: u16 = 16;
pub const FLAG_INVERSE: u16 = 32;

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TermCursor {
    pub col: u16,
    pub row: u16,
    pub shape: CursorShape,
    pub visible: bool,
    /// The character under the cursor (for block-cursor rendering).
    #[serde(default)]
    pub ch: String,
}

impl Default for TermCursor {
    fn default() -> Self {
        Self {
            col: 0,
            row: 0,
            shape: CursorShape::Block,
            visible: true,
            ch: " ".into(),
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum CursorShape {
    Block,
    Beam,
    Underline,
}

/// Incremental viewport update. Contains only changed lines plus cursor/selection.
/// When `full` is true, `changed_lines` contains ALL lines (used on resize/spawn).
#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct TermViewportPatch {
    /// (row_index, line) pairs for rows that changed since last sync.
    pub changed_lines: Vec<(u16, TermLine)>,
    pub cursor: TermCursor,
    pub cols: u16,
    pub rows: u16,
    pub selection: Option<TermSelectionRange>,
    #[serde(default)]
    pub copy_mode: bool,
    /// When true, changed_lines contains every row (full viewport rebuild).
    pub full: bool,
}

impl TermViewportPatch {
    pub fn requires_row_rebuild(&self, current_cols: u16, current_rows: u16) -> bool {
        self.full || self.cols != current_cols || self.rows != current_rows
    }

    pub fn changed_row_indices(&self) -> impl Iterator<Item = u16> + '_ {
        self.changed_lines.iter().map(|(row_idx, _)| *row_idx)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CursorRowUpdate {
    pub clear: Option<u16>,
    pub set: Option<u16>,
}

pub fn cursor_row_update(previous: Option<&TermCursor>, next: &TermCursor) -> CursorRowUpdate {
    let clear = previous
        .filter(|cursor| cursor.visible && (!next.visible || cursor.row != next.row))
        .map(|cursor| cursor.row);
    let set = next.visible.then_some(next.row);

    CursorRowUpdate { clear, set }
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TermKeyEvent {
    pub key: String,
    #[serde(default)]
    pub code: String,
    pub modifiers: u8,
    pub text: Option<String>,
}

pub const MOD_CTRL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;
pub const MOD_SUPER: u8 = 8;

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TermMouseEvent {
    /// 0=left, 1=middle, 2=right, 3=none (release/motion), 64=scroll_up, 65=scroll_down
    pub button: u8,
    pub col: u16,
    pub row: u16,
    pub modifiers: u8,
    /// true for press, false for release
    pub pressed: bool,
    /// true when this is a motion event (drag if button<3, move if button==3)
    #[serde(default)]
    pub moving: bool,
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TermResizeEvent {
    pub char_width: f32,
    pub char_height: f32,
    #[serde(default)]
    pub viewport_width: f32,
    #[serde(default)]
    pub viewport_height: f32,
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
pub struct TermTitleEvent {
    pub title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(changed_rows: Vec<u16>, cols: u16, rows: u16, full: bool) -> TermViewportPatch {
        TermViewportPatch {
            changed_lines: changed_rows
                .into_iter()
                .map(|row| (row, TermLine::default()))
                .collect(),
            cursor: TermCursor::default(),
            cols,
            rows,
            selection: None,
            copy_mode: false,
            full,
        }
    }

    #[test]
    fn viewport_patch_rebuilds_only_for_full_or_dimension_change() {
        assert!(!patch(vec![3], 80, 24, false).requires_row_rebuild(80, 24));
        assert!(patch(vec![3], 80, 24, true).requires_row_rebuild(80, 24));
        assert!(patch(vec![3], 100, 24, false).requires_row_rebuild(80, 24));
        assert!(patch(vec![3], 80, 30, false).requires_row_rebuild(80, 24));
    }

    #[test]
    fn viewport_patch_changed_rows_come_only_from_changed_lines() {
        let rows = patch(vec![1, 9], 80, 24, false)
            .changed_row_indices()
            .collect::<Vec<_>>();
        assert_eq!(rows, vec![1, 9]);
    }

    #[test]
    fn cursor_row_update_targets_only_old_and_new_visible_rows() {
        let old = TermCursor {
            row: 2,
            visible: true,
            ..TermCursor::default()
        };
        let new = TermCursor {
            row: 5,
            visible: true,
            ..TermCursor::default()
        };

        assert_eq!(
            cursor_row_update(Some(&old), &new),
            CursorRowUpdate {
                clear: Some(2),
                set: Some(5)
            }
        );
        assert_eq!(
            cursor_row_update(Some(&new), &new),
            CursorRowUpdate {
                clear: None,
                set: Some(5)
            }
        );
        assert_eq!(
            cursor_row_update(
                Some(&old),
                &TermCursor {
                    visible: false,
                    ..new
                }
            ),
            CursorRowUpdate {
                clear: Some(2),
                set: None
            }
        );
    }

    #[test]
    fn term_title_event_rkyv_roundtrip() {
        let original = TermTitleEvent {
            title: "hello-osc".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<TermTitleEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original.title, recovered.title);
    }

    #[test]
    fn term_loading_event_rkyv_roundtrip() {
        let original = TermLoadingEvent {
            loading: true,
            label: "Vibe".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<TermLoadingEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }
}
