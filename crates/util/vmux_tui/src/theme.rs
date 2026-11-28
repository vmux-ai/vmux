use std::env;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::io::TuiFs;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn lerp(self, other: Self, num: usize, den: usize) -> Self {
        if den == 0 {
            return self;
        }
        let mix = |a: u8, b: u8| -> u8 {
            let a = a as i32;
            let b = b as i32;
            (a + ((b - a) * num as i32) / den as i32).clamp(0, 255) as u8
        };
        Self(
            mix(self.0, other.0),
            mix(self.1, other.1),
            mix(self.2, other.2),
        )
    }

    pub fn lift(self, fg: Self, amount: usize) -> Self {
        self.lerp(fg, amount, 100)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bg: Rgb,
    pub fg: Rgb,
    pub muted: Rgb,
    pub header: Rgb,
    pub surface: Rgb,
    pub primary: Rgb,
    pub accent: Rgb,
    pub gold: Rgb,
    pub green: Rgb,
    pub red: Rgb,
    pub cyan: Rgb,
    ansi16: Option<[Rgb; 16]>,
}

impl Palette {
    pub fn from_ansi(bg: Rgb, fg: Rgb, ansi: [Rgb; 16]) -> Self {
        Self {
            bg,
            fg,
            muted: bg.lift(fg, 58),
            header: bg.lift(fg, 7),
            surface: bg.lift(fg, 18),
            primary: ansi[4],
            accent: ansi[5],
            gold: ansi[3],
            green: ansi[2],
            red: ansi[1],
            cyan: ansi[6],
            ansi16: Some(ansi),
        }
    }

    fn from_desktop(colors: [[u8; 3]; 8]) -> Self {
        let rgb = |c: [u8; 3]| Rgb(c[0], c[1], c[2]);
        let bg = rgb(colors[0]);
        let fg = rgb(colors[1]);
        Self {
            bg,
            fg,
            muted: bg.lift(fg, 58),
            header: bg.lift(fg, 7),
            surface: bg.lift(fg, 18),
            primary: rgb(colors[2]),
            accent: rgb(colors[3]),
            gold: rgb(colors[4]),
            green: rgb(colors[5]),
            red: rgb(colors[6]),
            cyan: rgb(colors[7]),
            ansi16: None,
        }
    }

    pub fn ansi(self, index: u8) -> Rgb {
        if let Some(table) = self.ansi16
            && usize::from(index) < table.len()
        {
            return table[usize::from(index)];
        }
        match index {
            0 => self.bg,
            1 => self.red,
            2 => self.green,
            3 => self.gold,
            4 => self.primary,
            5 => self.accent,
            6 => self.cyan,
            7 => self.fg,
            8 => self.muted,
            9 => self.red.lift(self.fg, 20),
            10 => self.green.lift(self.fg, 20),
            11 => self.gold.lift(self.fg, 20),
            12 => self.primary.lift(self.fg, 20),
            13 => self.accent.lift(self.fg, 20),
            14 => self.cyan.lift(self.fg, 20),
            15 => self.fg,
            16..=231 => {
                let n = index - 16;
                let r = n / 36;
                let g = (n % 36) / 6;
                let b = n % 6;
                let map = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Rgb(map(r), map(g), map(b))
            }
            232..=255 => {
                let gray = 8 + (index - 232) * 10;
                Rgb(gray, gray, gray)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemePreset {
    #[default]
    CatppuccinMocha,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    TokyoNight,
    Dracula,
    RosePine,
    Kanagawa,
    Nord,
    GruvboxDark,
    OneDark,
    GithubDark,
    GithubLight,
    SolarizedDark,
    SolarizedLight,
}

impl ThemePreset {
    pub const ALL: [Self; 15] = [
        Self::CatppuccinMocha,
        Self::CatppuccinLatte,
        Self::CatppuccinFrappe,
        Self::CatppuccinMacchiato,
        Self::TokyoNight,
        Self::Dracula,
        Self::RosePine,
        Self::Kanagawa,
        Self::Nord,
        Self::GruvboxDark,
        Self::OneDark,
        Self::GithubDark,
        Self::GithubLight,
        Self::SolarizedDark,
        Self::SolarizedLight,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::CatppuccinMocha => "catppuccin-mocha",
            Self::CatppuccinLatte => "catppuccin-latte",
            Self::CatppuccinFrappe => "catppuccin-frappe",
            Self::CatppuccinMacchiato => "catppuccin-macchiato",
            Self::TokyoNight => "tokyo-night",
            Self::Dracula => "dracula",
            Self::RosePine => "rose-pine",
            Self::Kanagawa => "kanagawa",
            Self::Nord => "nord",
            Self::GruvboxDark => "gruvbox-dark",
            Self::OneDark => "one-dark",
            Self::GithubDark => "github-dark",
            Self::GithubLight => "github-light",
            Self::SolarizedDark => "solarized-dark",
            Self::SolarizedLight => "solarized-light",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        let normalized = name.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|preset| preset.id() == normalized)
    }

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|preset| *preset == self)
            .unwrap_or(0)
    }

    pub fn is_light(self) -> bool {
        matches!(
            self,
            Self::CatppuccinLatte | Self::GithubLight | Self::SolarizedLight
        )
    }

    pub fn for_scheme(self, light: bool) -> Self {
        if light == self.is_light() {
            return self;
        }
        if light {
            match self {
                Self::CatppuccinMocha | Self::CatppuccinFrappe | Self::CatppuccinMacchiato => {
                    Self::CatppuccinLatte
                }
                Self::SolarizedDark => Self::SolarizedLight,
                _ => Self::GithubLight,
            }
        } else {
            match self {
                Self::CatppuccinLatte => Self::CatppuccinMocha,
                Self::SolarizedLight => Self::SolarizedDark,
                _ => Self::GithubDark,
            }
        }
    }

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    pub fn palette(self) -> Palette {
        match self {
            Self::CatppuccinMocha
            | Self::CatppuccinLatte
            | Self::CatppuccinFrappe
            | Self::CatppuccinMacchiato => self.catppuccin_palette(),
            Self::TokyoNight
            | Self::Dracula
            | Self::RosePine
            | Self::Kanagawa
            | Self::Nord
            | Self::GruvboxDark => self.vivid_palette(),
            Self::OneDark
            | Self::GithubDark
            | Self::GithubLight
            | Self::SolarizedDark
            | Self::SolarizedLight => self.classic_palette(),
        }
    }

    fn catppuccin_palette(self) -> Palette {
        match self {
            Self::CatppuccinMocha => Palette::from_desktop([
                [30, 30, 46],
                [205, 214, 244],
                [137, 180, 250],
                [245, 194, 231],
                [249, 226, 175],
                [166, 227, 161],
                [243, 139, 168],
                [148, 226, 213],
            ]),
            Self::CatppuccinLatte => Palette::from_desktop([
                [239, 241, 245],
                [76, 79, 105],
                [30, 102, 245],
                [234, 118, 203],
                [223, 142, 29],
                [64, 160, 43],
                [210, 15, 57],
                [4, 165, 229],
            ]),
            Self::CatppuccinFrappe => Palette::from_desktop([
                [48, 52, 70],
                [198, 208, 245],
                [140, 170, 238],
                [244, 184, 228],
                [229, 200, 144],
                [166, 209, 137],
                [231, 130, 132],
                [129, 200, 190],
            ]),
            _ => Palette::from_desktop([
                [36, 39, 58],
                [202, 211, 245],
                [138, 173, 244],
                [245, 189, 230],
                [238, 212, 159],
                [166, 218, 149],
                [237, 135, 150],
                [139, 213, 202],
            ]),
        }
    }

    fn vivid_palette(self) -> Palette {
        match self {
            Self::TokyoNight => Palette::from_desktop([
                [26, 27, 38],
                [192, 202, 245],
                [122, 162, 247],
                [187, 154, 247],
                [224, 175, 104],
                [158, 206, 106],
                [247, 118, 142],
                [125, 207, 255],
            ]),
            Self::Dracula => Palette::from_desktop([
                [40, 42, 54],
                [248, 248, 242],
                [189, 147, 249],
                [255, 121, 198],
                [241, 250, 140],
                [80, 250, 123],
                [255, 85, 85],
                [139, 233, 253],
            ]),
            Self::RosePine => Palette::from_desktop([
                [25, 23, 36],
                [224, 222, 244],
                [196, 167, 231],
                [235, 111, 146],
                [246, 193, 119],
                [156, 207, 216],
                [235, 111, 146],
                [156, 207, 216],
            ]),
            Self::Kanagawa => Palette::from_desktop([
                [31, 31, 40],
                [220, 215, 186],
                [126, 156, 216],
                [149, 127, 184],
                [224, 195, 117],
                [135, 169, 117],
                [231, 115, 118],
                [127, 180, 202],
            ]),
            Self::Nord => Palette::from_desktop([
                [46, 52, 64],
                [216, 222, 233],
                [136, 192, 208],
                [180, 142, 173],
                [235, 203, 139],
                [163, 190, 140],
                [191, 97, 106],
                [129, 161, 193],
            ]),
            _ => Palette::from_desktop([
                [40, 40, 40],
                [235, 219, 178],
                [142, 192, 124],
                [211, 134, 155],
                [250, 189, 47],
                [184, 187, 38],
                [251, 73, 52],
                [131, 165, 152],
            ]),
        }
    }

    fn classic_palette(self) -> Palette {
        match self {
            Self::OneDark => Palette::from_desktop([
                [40, 44, 52],
                [171, 178, 191],
                [97, 175, 239],
                [198, 120, 221],
                [229, 192, 123],
                [152, 195, 121],
                [224, 108, 117],
                [86, 182, 194],
            ]),
            Self::GithubDark => Palette::from_desktop([
                [13, 17, 23],
                [230, 237, 243],
                [88, 166, 255],
                [210, 168, 255],
                [227, 179, 65],
                [86, 211, 100],
                [255, 123, 114],
                [86, 212, 221],
            ]),
            Self::GithubLight => Palette::from_desktop([
                [255, 255, 255],
                [31, 35, 40],
                [9, 105, 218],
                [130, 80, 223],
                [154, 103, 0],
                [26, 127, 55],
                [207, 34, 46],
                [27, 124, 131],
            ]),
            Self::SolarizedDark => Palette::from_desktop([
                [0, 43, 54],
                [147, 161, 161],
                [38, 139, 210],
                [211, 54, 130],
                [181, 137, 0],
                [133, 153, 0],
                [220, 50, 47],
                [42, 161, 152],
            ]),
            _ => Palette::from_desktop([
                [253, 246, 227],
                [101, 123, 131],
                [38, 139, 210],
                [211, 54, 130],
                [181, 137, 0],
                [133, 153, 0],
                [220, 50, 47],
                [42, 161, 152],
            ]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub preset: ThemePreset,
    palette: Palette,
    pub color: bool,
}

impl Theme {
    pub fn new(preset: ThemePreset, color: bool) -> Self {
        Self::with_palette(preset, preset.palette(), color)
    }

    pub fn with_palette(preset: ThemePreset, palette: Palette, color: bool) -> Self {
        Self {
            preset,
            palette,
            color,
        }
    }

    pub fn palette(self) -> Palette {
        self.palette
    }

    pub fn fg(self, rgb: Rgb, text: &str) -> String {
        if self.color {
            format!("\x1b[38;2;{};{};{}m{text}\x1b[0m", rgb.0, rgb.1, rgb.2)
        } else {
            text.to_owned()
        }
    }

    pub fn fg_bold(self, rgb: Rgb, text: &str) -> String {
        if self.color {
            format!("\x1b[1;38;2;{};{};{}m{text}\x1b[0m", rgb.0, rgb.1, rgb.2)
        } else {
            text.to_owned()
        }
    }

    pub fn bold(self, text: &str) -> String {
        self.fg_bold(self.palette().fg, text)
    }

    pub fn text(self, text: &str) -> String {
        self.fg(self.palette().fg, text)
    }

    pub fn muted(self, text: &str) -> String {
        self.fg(self.palette().muted, text)
    }

    pub fn primary(self, text: &str) -> String {
        self.fg(self.palette().primary, text)
    }

    pub fn accent(self, text: &str) -> String {
        self.fg(self.palette().accent, text)
    }

    pub fn gold(self, text: &str) -> String {
        self.fg(self.palette().gold, text)
    }

    pub fn green(self, text: &str) -> String {
        self.fg(self.palette().green, text)
    }

    pub fn fg_bg_bold(self, fg: Rgb, bg: Rgb, text: &str) -> String {
        if self.color {
            format!(
                "\x1b[1;38;2;{};{};{};48;2;{};{};{}m{text}\x1b[0m",
                fg.0, fg.1, fg.2, bg.0, bg.1, bg.2
            )
        } else {
            text.to_owned()
        }
    }

    pub fn fg_bg(self, fg: Rgb, bg: Rgb, text: &str) -> String {
        if self.color {
            format!(
                "\x1b[38;2;{};{};{};48;2;{};{};{}m{text}\x1b[0m",
                fg.0, fg.1, fg.2, bg.0, bg.1, bg.2
            )
        } else {
            text.to_owned()
        }
    }

    pub fn surface_row(self, text: &str, width: usize) -> String {
        self.bg_row(self.palette().surface, text, width)
    }

    pub fn header_row(self, text: &str, width: usize) -> String {
        self.bg_row(self.palette().header, text, width)
    }

    pub fn panel_row(self, text: &str, width: usize) -> String {
        self.bg_row(self.palette().header, text, width)
    }

    fn bg_row(self, color: Rgb, text: &str, width: usize) -> String {
        let trimmed = Self::truncate_ellipsis(text, width);
        let pad = width.saturating_sub(Self::display_width(&trimmed));
        if self.color {
            let bg = format!("\x1b[48;2;{};{};{}m", color.0, color.1, color.2);
            let body = trimmed.replace("\x1b[0m", &format!("\x1b[0m{bg}"));
            format!("{bg}{body}{}\x1b[0m", " ".repeat(pad))
        } else {
            format!("{trimmed}{}", " ".repeat(pad))
        }
    }

    pub fn swatches(self, preset: ThemePreset) -> String {
        let p = preset.palette();
        [p.primary, p.accent, p.green]
            .into_iter()
            .map(|rgb| self.fg(rgb, "●"))
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn theme_list(self, active: ThemePreset) -> String {
        let mut out = String::new();
        for preset in ThemePreset::ALL {
            let marker = if preset == active {
                self.green("★")
            } else {
                " ".into()
            };
            let swatches = self.swatches(preset);
            let name = if preset == active {
                self.bold(preset.id())
            } else {
                preset.id().to_owned()
            };
            out.push_str(&format!("  {marker} {swatches}  {name}\n"));
        }
        out
    }

    pub fn display_width(text: &str) -> usize {
        let mut width = 0usize;
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                for esc in chars.by_ref() {
                    if esc.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            width += Self::char_width(ch);
        }
        width
    }

    pub fn char_width(ch: char) -> usize {
        let code = ch as u32;
        if matches!(code, 0x200D | 0xFE0E | 0xFE0F | 0x0300..=0x036F) {
            return 0;
        }
        if matches!(
            code,
            0x1100..=0x115F
                | 0x2E80..=0x303E
                | 0x3040..=0xA4CF
                | 0xAC00..=0xD7A3
                | 0xF900..=0xFAFF
                | 0xFE10..=0xFE19
                | 0xFE30..=0xFE6F
                | 0xFF00..=0xFF60
                | 0xFFE0..=0xFFE6
                | 0x1F300..=0x1FAFF
        ) {
            return 2;
        }
        1
    }

    pub fn fit_title(text: &str, max_width: usize) -> String {
        if max_width == 0 {
            return String::new();
        }
        if text.contains('/') {
            Self::truncate_start_ellipsis(text, max_width)
        } else {
            Self::truncate_ellipsis(text, max_width)
        }
    }

    pub fn truncate_start_ellipsis(text: &str, max_width: usize) -> String {
        if Self::display_width(text) <= max_width {
            return text.to_owned();
        }
        if max_width == 0 {
            return String::new();
        }
        if max_width == 1 {
            return "…".to_owned();
        }
        let plain = Self::strip_ansi(text);
        let limit = max_width - 1;
        let mut kept = Vec::new();
        let mut width = 0usize;
        for ch in plain.chars().rev() {
            let w = Self::char_width(ch);
            if width + w > limit {
                break;
            }
            kept.push(ch);
            width += w;
        }
        kept.reverse();
        let mut out = String::with_capacity(kept.len() + 3);
        out.push('…');
        for ch in kept {
            out.push(ch);
        }
        out
    }

    fn strip_ansi(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                for esc in chars.by_ref() {
                    if esc.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            out.push(ch);
        }
        out
    }

    pub fn truncate_ellipsis(text: &str, max_width: usize) -> String {
        if max_width == 0 {
            return String::new();
        }
        if Self::display_width(text) <= max_width {
            return text.to_owned();
        }
        let limit = max_width.saturating_sub(1);
        let mut out = String::new();
        let mut width = 0usize;
        let mut in_ansi = false;
        let mut saw_ansi = false;
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                in_ansi = true;
                saw_ansi = true;
                out.push(ch);
                continue;
            }
            if in_ansi {
                out.push(ch);
                if ch.is_ascii_alphabetic() {
                    in_ansi = false;
                }
                continue;
            }
            let w = Self::char_width(ch);
            if width + w > limit {
                out.push('…');
                if saw_ansi {
                    out.push_str("\x1b[0m");
                }
                break;
            }
            out.push(ch);
            width += w;
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarPosition {
    #[default]
    Top,
    Bottom,
}

impl BarPosition {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "bottom" | "footer" => Some(Self::Bottom),
            "top" | "header" => Some(Self::Top),
            _ => None,
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Bottom => Self::Top,
            Self::Top => Self::Bottom,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoredPreferences {
    #[serde(default)]
    theme: String,
    #[serde(default)]
    bar_position: BarPosition,
}

pub struct ThemeStore;

impl ThemeStore {
    pub fn load() -> ThemePreset {
        Self::load_explicit().unwrap_or_default()
    }

    pub fn load_explicit() -> Option<ThemePreset> {
        if let Some(name) = env::var_os("VMUX_TUI_THEME").and_then(|value| value.into_string().ok())
            && let Some(preset) = ThemePreset::parse(&name)
        {
            return Some(preset);
        }
        Self::read_stored().and_then(|stored| ThemePreset::parse(&stored.theme))
    }

    pub fn load_bar_position() -> BarPosition {
        if let Some(value) = env::var_os("VMUX_TUI_BAR").and_then(|value| value.into_string().ok())
            && let Some(position) = BarPosition::parse(&value)
        {
            return position;
        }
        Self::read_stored()
            .map(|stored| stored.bar_position)
            .unwrap_or_default()
    }

    pub fn save(preset: ThemePreset) -> io::Result<()> {
        let bar_position = Self::read_stored()
            .map(|stored| stored.bar_position)
            .unwrap_or_default();
        Self::write_stored(&StoredPreferences {
            theme: preset.id().to_owned(),
            bar_position,
        })
    }

    pub fn save_bar_position(bar_position: BarPosition) -> io::Result<()> {
        let theme = Self::read_stored()
            .map(|stored| stored.theme)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| ThemePreset::default().id().to_owned());
        Self::write_stored(&StoredPreferences {
            theme,
            bar_position,
        })
    }

    fn read_stored() -> Option<StoredPreferences> {
        let path = Self::path()?;
        let bytes = TuiFs::read_bytes(&path)?;
        serde_json::from_slice::<StoredPreferences>(&bytes).ok()
    }

    fn write_stored(prefs: &StoredPreferences) -> io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        let payload = serde_json::to_vec_pretty(prefs).map_err(io::Error::other)?;
        TuiFs::write_atomic_no_symlink(&path, &payload)
    }

    fn path() -> Option<PathBuf> {
        if let Some(path) = env::var_os("VMUX_CLI_TUI_CONFIG")
            && !path.is_empty()
        {
            return Some(PathBuf::from(path));
        }
        let home = env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".vmux/cli/tui.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_title_keeps_trailing_path_segments() {
        let path = "~/.vmux/projects/github.com/vmux-ai/vmux-cloud";
        let fitted = Theme::fit_title(path, 20);
        assert!(fitted.starts_with('…'));
        assert!(fitted.ends_with("vmux-cloud"));
        assert!(Theme::display_width(&fitted) <= 20);
        assert_eq!(Theme::fit_title("short-name", 20), "short-name");
        assert_eq!(Theme::fit_title("terminal", 0), "");
        assert_eq!(Theme::fit_title("a/b", 0), "");
    }
}
