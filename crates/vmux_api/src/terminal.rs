#[vmux_api::contract(Copy, Default, Eq, Hash)]
pub struct RgbColor([u8; 3]);

impl RgbColor {
    pub const fn components(self) -> [u8; 3] {
        self.0
    }
}

impl From<[u8; 3]> for RgbColor {
    fn from(value: [u8; 3]) -> Self {
        Self(value)
    }
}

#[vmux_api::contract(Copy, Eq, Hash)]
pub struct AnsiPalette([RgbColor; 16]);

impl AnsiPalette {
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &RgbColor> {
        self.0.iter()
    }
}

impl From<[[u8; 3]; 16]> for AnsiPalette {
    fn from(value: [[u8; 3]; 16]) -> Self {
        Self(value.map(RgbColor::from))
    }
}

#[vmux_api::contract(Default)]
pub enum TermColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[vmux_api::contract(Copy, Eq)]
pub struct TermSelectionRange {
    pub start_col: u16,
    pub start_row: u16,
    pub end_col: u16,
    pub end_row: u16,
    pub is_block: bool,
}

#[vmux_api::contract(Default)]
pub struct TermLine {
    pub spans: Vec<TermSpan>,
    #[serde(default)]
    pub links: Vec<LinkRange>,
}

#[vmux_api::contract(Default)]
pub struct LinkRange {
    pub start_col: u16,
    pub end_col: u16,
    pub url: String,
}

#[vmux_api::contract(Default)]
pub struct TermSpan {
    pub text: String,
    pub fg: TermColor,
    pub bg: TermColor,
    pub flags: u16,
    #[serde(default)]
    pub col: u16,
    #[serde(default)]
    pub grid_cols: u16,
}

pub const FLAG_BOLD: u16 = 1;
pub const FLAG_ITALIC: u16 = 2;
pub const FLAG_UNDERLINE: u16 = 4;
pub const FLAG_STRIKETHROUGH: u16 = 8;
pub const FLAG_DIM: u16 = 16;
pub const FLAG_INVERSE: u16 = 32;

#[vmux_api::contract]
pub struct TermCursor {
    pub col: u16,
    pub row: u32,
    pub shape: CursorShape,
    pub visible: bool,
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

#[vmux_api::contract(Copy, Eq)]
pub enum CursorShape {
    Block,
    Beam,
    Underline,
}
