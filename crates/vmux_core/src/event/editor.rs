use super::CommandBarPicker;

#[vmux_api::contract]
pub struct StyledSpan {
    pub text: String,
    pub fg: [u8; 3],
    pub bold: bool,
    pub italic: bool,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub enum FoldGutter {
    #[default]
    None,
    Open,
    Collapsed,
}

#[vmux_api::contract(Default)]
pub struct FileLine {
    pub line_no: u32,
    pub fold: FoldGutter,
    pub spans: Vec<StyledSpan>,
    #[serde(default)]
    pub indent_levels: u16,
}

#[vmux_api::contract]
pub struct FileMetaEvent {
    pub revision: u64,
    pub path: String,
    pub abs_path: String,
    pub kind: FileDocumentKind,
    pub language: String,
    pub total_lines: u32,
    #[serde(default)]
    pub indent: FileIndent,
    #[serde(default)]
    pub line_ending: FileLineEnding,
    #[serde(default)]
    pub encoding: FileEncoding,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub enum FileDocumentKind {
    #[default]
    Text,
    Markdown,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub struct FileIndent {
    pub spaces: bool,
    pub width: u16,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub enum FileLineEnding {
    #[default]
    Lf,
    Crlf,
}

#[vmux_api::contract(Copy, Eq, Hash, Default)]
pub enum FileEncoding {
    #[default]
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    ShiftJis,
    EucJp,
    Iso2022Jp,
    Gbk,
    Big5,
    EucKr,
    Windows1252,
    Iso8859_1,
}

impl FileEncoding {
    pub const ALL: [Self; 12] = [
        Self::Utf8,
        Self::Utf8Bom,
        Self::Utf16Le,
        Self::Utf16Be,
        Self::ShiftJis,
        Self::EucJp,
        Self::Iso2022Jp,
        Self::Gbk,
        Self::Big5,
        Self::EucKr,
        Self::Windows1252,
        Self::Iso8859_1,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 with BOM",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::ShiftJis => "Shift_JIS",
            Self::EucJp => "EUC-JP",
            Self::Iso2022Jp => "ISO-2022-JP",
            Self::Gbk => "GBK",
            Self::Big5 => "Big5",
            Self::EucKr => "EUC-KR",
            Self::Windows1252 => "Windows-1252",
            Self::Iso8859_1 => "ISO-8859-1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedEncodingLabel;

impl std::fmt::Display for UnsupportedEncodingLabel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("unsupported file encoding label")
    }
}

impl std::error::Error for UnsupportedEncodingLabel {}

impl TryFrom<&str> for FileEncoding {
    type Error = UnsupportedEncodingLabel;

    fn try_from(label: &str) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.label() == label)
            .ok_or(UnsupportedEncodingLabel)
    }
}

#[vmux_api::contract]
pub struct FileViewportPatch {
    pub first_row: u32,
    pub total_rows: u32,
    pub total_lines: u32,
    pub wrap_columns: u16,
    pub layouts: Vec<FileLineLayout>,
    pub lines: Vec<FileLine>,
    #[serde(default)]
    pub sticky: Vec<FileLine>,
}

#[vmux_api::contract(Copy, Eq)]
pub struct FileLineLayout {
    pub line_no: u32,
    pub row: u32,
    pub rows: u16,
}

#[vmux_api::contract(Copy)]
pub enum MdTableAlign {
    None,
    Left,
    Center,
    Right,
}

#[vmux_api::contract]
#[rkyv(serialize_bounds(__S: rkyv::ser::Writer + rkyv::ser::Allocator, __S::Error: rkyv::rancor::Source))]
#[rkyv(deserialize_bounds(__D::Error: rkyv::rancor::Source))]
#[rkyv(bytecheck(bounds(__C: rkyv::validation::ArchiveContext, __C::Error: rkyv::rancor::Source)))]
pub enum MdInline {
    Text(String),
    Code(String),
    Strong(#[rkyv(omit_bounds)] Vec<MdInline>),
    Emph(#[rkyv(omit_bounds)] Vec<MdInline>),
    Strike(#[rkyv(omit_bounds)] Vec<MdInline>),
    Link {
        href: String,
        #[rkyv(omit_bounds)]
        inlines: Vec<MdInline>,
    },
    Image {
        src: String,
        alt: String,
    },
    SoftBreak,
    HardBreak,
    WikiLink {
        target: String,
        label: String,
        path: String,
        line: Option<u32>,
        exists: bool,
        embed: bool,
    },
}

#[vmux_api::contract]
#[rkyv(serialize_bounds(__S: rkyv::ser::Writer + rkyv::ser::Allocator, __S::Error: rkyv::rancor::Source))]
#[rkyv(deserialize_bounds(__D::Error: rkyv::rancor::Source))]
#[rkyv(bytecheck(bounds(__C: rkyv::validation::ArchiveContext, __C::Error: rkyv::rancor::Source)))]
pub struct MdListItem {
    pub source_line: u32,
    pub task: Option<bool>,
    #[rkyv(omit_bounds)]
    pub blocks: Vec<MdBlock>,
}

#[vmux_api::contract]
#[rkyv(serialize_bounds(__S: rkyv::ser::Writer + rkyv::ser::Allocator, __S::Error: rkyv::rancor::Source))]
#[rkyv(deserialize_bounds(__D::Error: rkyv::rancor::Source))]
#[rkyv(bytecheck(bounds(__C: rkyv::validation::ArchiveContext, __C::Error: rkyv::rancor::Source)))]
pub enum MdBlock {
    Heading {
        level: u8,
        inlines: Vec<MdInline>,
    },
    Paragraph {
        inlines: Vec<MdInline>,
    },
    List {
        ordered: bool,
        start: u64,
        #[rkyv(omit_bounds)]
        items: Vec<MdListItem>,
    },
    CodeBlock {
        lang: String,
        lines: Vec<FileLine>,
    },
    BlockQuote {
        #[rkyv(omit_bounds)]
        blocks: Vec<MdBlock>,
    },
    Table {
        aligns: Vec<MdTableAlign>,
        header: Vec<Vec<MdInline>>,
        rows: Vec<Vec<Vec<MdInline>>>,
    },
    ThematicBreak,
    Html {
        raw: String,
    },
}

#[vmux_api::contract]
pub struct NoteBlock {
    pub start_line: u32,
    pub end_line: u32,
    pub source: String,
    pub block: MdBlock,
}

#[vmux_api::contract]
pub struct FileNoteEvent {
    pub title: String,
    pub properties: Vec<crate::knowledge::KnowledgeProperty>,
    pub blocks: Vec<NoteBlock>,
    pub active: Option<u32>,
    pub references: Vec<crate::knowledge::KnowledgeReference>,
    pub reveal_line: Option<u32>,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct FilePropertyEdit {
    pub original_key: String,
    pub key: String,
    pub kind: crate::knowledge::KnowledgePropertyKind,
    pub values: Vec<String>,
    pub remove: bool,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct KnowledgeLinkOpen {
    pub path: String,
    pub title: String,
    pub line: Option<u32>,
    pub create: bool,
}

#[vmux_api::contract]
pub struct FileErrorEvent {
    pub message: String,
    #[serde(default)]
    pub undecodable: bool,
}

#[vmux_api::ui_event(Default, target = "files")]
pub struct FileResizeEvent {
    pub char_height: f32,
    pub viewport_height: f32,
    pub wrap_columns: u16,
}

#[vmux_api::ui_event(target = "files")]
pub struct FileVideoRect {
    pub path: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[vmux_api::ui_event(Eq, Default, target = "files")]
pub struct FileScrollEvent {
    pub top_row: u32,
    pub needs_rows: bool,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub struct FileScrollByEvent {
    pub lines: i32,
}

#[vmux_api::ui_event(Copy, Eq, Default, target = "files")]
pub struct FileFoldToggle {
    pub line: u32,
}

pub use vmux_api::space::{ProjectBranch, ProjectRow, ProjectRowKind, ProjectTreeToggle};

#[vmux_api::contract(Eq)]
pub struct FileDirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[vmux_api::contract(Eq)]
pub struct FileDirEvent {
    pub path: String,
    pub abs_path: String,
    pub entries: Vec<FileDirEntry>,
    pub parent_path: String,
    pub parent_entries: Vec<FileDirEntry>,
}

#[vmux_api::contract(Default)]
pub struct FileThemeEvent {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct FilePreviewRequest {
    pub path: String,
    pub thumb: bool,
}

#[vmux_api::contract]
pub enum PreviewKind {
    Dir(Vec<FileDirEntry>),
    Text(Vec<FileLine>),
    Image {
        mime: String,
        bytes: Vec<u8>,
    },
    Video {
        url: String,
        path: String,
        native: bool,
    },
    Info {
        size: u64,
        modified: String,
        kind: String,
    },
    Error(String),
}

#[vmux_api::contract]
pub struct FilePreviewEvent {
    pub path: String,
    pub thumb: bool,
    pub kind: PreviewKind,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct FileOpenEvent {
    pub path: String,
}

#[vmux_api::contract(Eq)]
pub struct FileMediaEvent {
    pub kind: crate::media::MediaKind,
    pub mime: String,
    pub url: String,
    pub abs_path: String,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct FileOpenExternalRequest {
    pub path: String,
}

#[vmux_api::ui_event(Eq, target = "files")]
pub struct FileTextInput {
    pub text: String,
}

#[vmux_api::ui_event(Copy, Eq, target = "files")]
pub struct FilePointerEvent {
    pub line: u32,
    pub col: u32,
    pub extend: bool,
    pub add: bool,
}

#[vmux_api::contract(Eq)]
pub struct FileCursorEvent {
    pub mode: crate::editor::EditMode,
    pub mode_label: String,
    pub primary: crate::editor::CursorPos,
    pub carets: Vec<crate::editor::CursorPos>,
    pub selections: Vec<crate::editor::SelSpan>,
    pub source_primary: crate::editor::CursorPos,
    pub source_selections: Vec<crate::editor::SelSpan>,
    pub search: Vec<crate::editor::SelSpan>,
    pub word_highlights: Vec<crate::editor::SelSpan>,
    pub search_total: u32,
    pub search_index: u32,
}

#[vmux_api::contract(Copy, Eq)]
pub struct FileDirtyEvent {
    pub dirty: bool,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub enum FileViewMode {
    #[default]
    Editor,
    Note,
    Diff,
}

#[vmux_api::contract(Copy, Eq)]
pub struct FileViewModeEvent {
    pub mode: FileViewMode,
}

#[vmux_api::ui_event(Copy, Eq, target = "files")]
pub struct FileViewModeSet {
    pub mode: FileViewMode,
}

#[vmux_api::contract(Copy, Eq)]
pub struct FileKeymapEvent {
    pub keymap: crate::editor::KeymapKind,
}

#[vmux_api::ui_event(Copy, Eq, target = "files")]
pub struct FileKeymapSet {
    pub keymap: crate::editor::KeymapKind,
}

#[vmux_api::contract(Copy, Eq)]
pub struct FileShapeEvent {
    pub indent: FileIndent,
    pub line_ending: FileLineEnding,
}

#[vmux_api::ui_event(Copy, Eq, target = "files")]
pub struct FileShapeSet {
    pub indent: FileIndent,
    pub line_ending: FileLineEnding,
}

#[vmux_api::contract(Copy, Eq)]
pub struct FileEncodingEvent {
    pub encoding: FileEncoding,
}

#[vmux_api::contract(Copy, Eq)]
pub enum FileEncodingAction {
    Reopen,
    Save,
}

#[vmux_api::ui_event(Copy, Eq, target = "files")]
pub struct FileEncodingSet {
    pub encoding: FileEncoding,
    pub action: FileEncodingAction,
}

#[vmux_api::ui_event(Copy, Eq, target = "files")]
pub struct FileStatusPickerOpen {
    pub picker: CommandBarPicker,
}

impl From<CommandBarPicker> for FileStatusPickerOpen {
    fn from(picker: CommandBarPicker) -> Self {
        Self { picker }
    }
}

#[vmux_api::contract(Copy, Eq)]
pub enum FileKey {
    ToggleExplorer,
    RevealInExplorer,
    PanelNext,
    PanelPrevious,
    PanelChoose,
    PanelDismiss,
    Find { forward: bool },
    FindClose,
    FindInFiles,
}

#[vmux_api::contract(Copy, Eq)]
pub struct FileTidyPromptEvent {
    pub count: u32,
}

#[vmux_api::contract(Copy, Eq)]
pub enum TidyChoice {
    Tidy,
    Always,
    Dismiss,
}

#[vmux_api::ui_event(Copy, Eq, target = "files")]
pub struct FileTidyRequest {
    pub choice: TidyChoice,
}

#[vmux_api::ui_event(Default, Eq, target = "files")]
pub struct FileFindRequest {
    pub query: String,
    pub step: bool,
    pub reverse: bool,
    pub done: bool,
    pub regex: bool,
    pub forward: bool,
}
