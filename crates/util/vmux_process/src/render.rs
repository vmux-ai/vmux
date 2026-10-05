use alacritty_terminal::{
    event::EventListener as TermEventListener,
    grid::Dimensions,
    index::{Column, Line},
    term::{Term, cell::Flags as CellFlags},
    vte::ansi::{Color, NamedColor},
};
use vmux_ecs::event::{
    FLAG_BOLD, FLAG_DIM, FLAG_INVERSE, FLAG_ITALIC, FLAG_STRIKETHROUGH, FLAG_UNDERLINE, TermColor,
    TermLine, TermSpan,
};

fn mix_row_hash(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(0x100000001b3);
}

fn mix_color_hash(hash: &mut u64, color: &Color) {
    match color {
        Color::Named(color) => {
            mix_row_hash(hash, 0);
            mix_row_hash(hash, *color as u8 as u64);
        }
        Color::Spec(rgb) => {
            mix_row_hash(hash, 1);
            mix_row_hash(hash, rgb.r as u64);
            mix_row_hash(hash, rgb.g as u64);
            mix_row_hash(hash, rgb.b as u64);
        }
        Color::Indexed(index) => {
            mix_row_hash(hash, 2);
            mix_row_hash(hash, *index as u64);
        }
    }
}

pub(crate) struct TermRow;

impl TermRow {
    pub(crate) fn hash<T: TermEventListener>(term: &Term<T>, row_idx: usize, offset: i32) -> u64 {
        let mut hash = 0xcbf29ce484222325;
        let grid = term.grid();
        let num_cols = grid.columns();
        let row = &grid[Line(row_idx as i32 - offset)];
        for col_idx in 0..num_cols {
            let cell = &row[Column(col_idx)];
            mix_row_hash(&mut hash, cell.c as u32 as u64);
            mix_color_hash(&mut hash, &cell.fg);
            mix_color_hash(&mut hash, &cell.bg);
            mix_row_hash(&mut hash, cell.flags.bits() as u64);
        }
        hash
    }

    pub(crate) fn line<T: TermEventListener>(
        term: &Term<T>,
        row_idx: usize,
        offset: i32,
    ) -> TermLine {
        let grid = term.grid();
        let num_cols = grid.columns();
        let row = &grid[Line(row_idx as i32 - offset)];
        let mut spans = Vec::new();
        let mut text = String::new();
        let mut cur_fg = TermColor::Default;
        let mut cur_bg = TermColor::Default;
        let mut cur_flags: u16 = 0;
        let mut span_col_start: u16 = 0;
        let mut span_grid_cols: u16 = 0;

        for col_idx in 0..num_cols {
            let cell = &row[Column(col_idx)];
            if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
                span_grid_cols += 1;
                continue;
            }
            let fg = color_to_term_color(&cell.fg);
            let bg = color_to_term_color(&cell.bg);
            let flags = cell_flags_to_u16(cell.flags);
            if fg != cur_fg || bg != cur_bg || flags != cur_flags {
                if !text.is_empty() {
                    spans.push(TermSpan {
                        text: std::mem::take(&mut text),
                        fg: cur_fg,
                        bg: cur_bg,
                        flags: cur_flags,
                        col: span_col_start,
                        grid_cols: span_grid_cols,
                    });
                    span_col_start = col_idx as u16;
                    span_grid_cols = 0;
                }
                cur_fg = fg;
                cur_bg = bg;
                cur_flags = flags;
            }
            text.push(cell.c);
            span_grid_cols += 1;
        }
        if !text.is_empty() {
            spans.push(TermSpan {
                text,
                fg: cur_fg,
                bg: cur_bg,
                flags: cur_flags,
                col: span_col_start,
                grid_cols: span_grid_cols,
            });
        }
        TermLine {
            spans,
            links: Vec::new(),
        }
    }
}

fn color_to_term_color(color: &Color) -> TermColor {
    match color {
        Color::Named(named) => match named {
            NamedColor::Foreground | NamedColor::DimForeground | NamedColor::BrightForeground => {
                TermColor::Default
            }
            NamedColor::Background | NamedColor::Cursor => TermColor::Default,
            other => TermColor::Indexed(named_to_ansi_index(other)),
        },
        Color::Indexed(idx) if *idx < 16 => TermColor::Indexed(*idx),
        Color::Indexed(idx) => {
            let [r, g, b] = ansi_256_to_rgb(*idx);
            TermColor::Rgb(r, g, b)
        }
        Color::Spec(rgb) => TermColor::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

fn named_to_ansi_index(named: &NamedColor) -> u8 {
    match named {
        NamedColor::Black | NamedColor::DimBlack => 0,
        NamedColor::Red | NamedColor::DimRed => 1,
        NamedColor::Green | NamedColor::DimGreen => 2,
        NamedColor::Yellow | NamedColor::DimYellow => 3,
        NamedColor::Blue | NamedColor::DimBlue => 4,
        NamedColor::Magenta | NamedColor::DimMagenta => 5,
        NamedColor::Cyan | NamedColor::DimCyan => 6,
        NamedColor::White | NamedColor::DimWhite => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
        _ => 7,
    }
}

fn cell_flags_to_u16(flags: CellFlags) -> u16 {
    let mut f = 0u16;
    if flags.contains(CellFlags::BOLD) {
        f |= FLAG_BOLD;
    }
    if flags.contains(CellFlags::ITALIC) {
        f |= FLAG_ITALIC;
    }
    if flags.contains(CellFlags::UNDERLINE) {
        f |= FLAG_UNDERLINE;
    }
    if flags.contains(CellFlags::STRIKEOUT) {
        f |= FLAG_STRIKETHROUGH;
    }
    if flags.contains(CellFlags::DIM) {
        f |= FLAG_DIM;
    }
    if flags.contains(CellFlags::INVERSE) {
        f |= FLAG_INVERSE;
    }
    f
}

fn ansi_256_to_rgb(idx: u8) -> [u8; 3] {
    if idx < 16 {
        return [0, 0, 0];
    }
    if idx < 232 {
        let i = idx - 16;
        let r = (i / 36) * 51;
        let g = ((i % 36) / 6) * 51;
        let b = (i % 6) * 51;
        [r, g, b]
    } else {
        let v = 8 + (idx - 232) * 10;
        [v, v, v]
    }
}
