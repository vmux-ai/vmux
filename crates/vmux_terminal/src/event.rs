use serde::{Deserialize, Serialize};

pub const TERM_VIEWPORT_EVENT: &str = "term_viewport";
pub const TERM_KEY_EVENT: &str = "term_key";
pub const TERM_MOUSE_EVENT: &str = "term_mouse";
pub const TERM_RESIZE_EVENT: &str = "term_resize";

pub const TERMINAL_WEBVIEW_URL: &str = "vmux://terminal/";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermViewportEvent {
    pub lines: Vec<TermLine>,
    pub cursor: TermCursor,
    pub cols: u16,
    pub rows: u16,
    pub title: Option<String>,
    #[serde(default)]
    pub font_family: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermLine {
    pub spans: Vec<TermSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermSpan {
    pub text: String,
    pub fg: Option<[u8; 3]>,
    pub bg: Option<[u8; 3]>,
    pub flags: u16,
}

pub const FLAG_BOLD: u16 = 1;
pub const FLAG_ITALIC: u16 = 2;
pub const FLAG_UNDERLINE: u16 = 4;
pub const FLAG_STRIKETHROUGH: u16 = 8;
pub const FLAG_DIM: u16 = 16;
pub const FLAG_INVERSE: u16 = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermCursor {
    pub col: u16,
    pub row: u16,
    pub shape: CursorShape,
    pub visible: bool,
}

impl Default for TermCursor {
    fn default() -> Self {
        Self {
            col: 0,
            row: 0,
            shape: CursorShape::Block,
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Beam,
    Underline,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermKeyEvent {
    pub key: String,
    pub modifiers: u8,
    pub text: Option<String>,
}

pub const MOD_CTRL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;
pub const MOD_SUPER: u8 = 8;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermMouseEvent {
    pub button: u8,
    pub col: u16,
    pub row: u16,
    pub modifiers: u8,
    pub pressed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermResizeEvent {
    pub char_width: f32,
    pub char_height: f32,
}
