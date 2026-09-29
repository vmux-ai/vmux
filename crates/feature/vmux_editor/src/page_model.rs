use unicode_width::UnicodeWidthChar;
use vmux_core::event::{DiagSeverity, FileDiagnostic, MdTableAlign, StyledSpan};

pub fn editor_drag_started(origin: (i32, i32), current: (i32, i32)) -> bool {
    let dx = f64::from(current.0) - f64::from(origin.0);
    let dy = f64::from(current.1) - f64::from(origin.1);
    dx * dx + dy * dy >= 16.0
}

pub fn note_list_marker_prefix_len(line: &str) -> Option<(usize, usize)> {
    let chars = line.chars().collect::<Vec<_>>();
    let indent = chars.iter().take_while(|ch| ch.is_whitespace()).count();
    let rest = &chars[indent..];
    let marker_end =
        if rest.len() >= 2 && matches!(rest[0], '-' | '*' | '+') && rest[1].is_whitespace() {
            2
        } else {
            let digits = rest.iter().take_while(|ch| ch.is_ascii_digit()).count();
            if digits > 0
                && rest.len() > digits + 1
                && matches!(rest[digits], '.' | ')')
                && rest[digits + 1].is_whitespace()
            {
                digits + 2
            } else {
                return None;
            }
        };
    let task = &rest[marker_end..];
    let task_prefix = usize::from(
        task.len() >= 4
            && task[0] == '['
            && matches!(task[1], ' ' | 'x' | 'X')
            && task[2] == ']'
            && task[3].is_whitespace(),
    ) * 4;
    Some((indent, indent + marker_end + task_prefix))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteInlineKind {
    BlockMarker,
    Code,
    Strong,
    Emph,
    Strike,
    Link,
    WikiLink,
    Escape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteInlineNode {
    Text {
        start: u32,
        end: u32,
    },
    Syntax {
        kind: NoteInlineKind,
        start: u32,
        prefix_end: u32,
        suffix_start: u32,
        end: u32,
        children: Vec<NoteInlineNode>,
    },
}

impl NoteInlineNode {
    pub fn start(&self) -> u32 {
        match self {
            Self::Text { start, .. } | Self::Syntax { start, .. } => *start,
        }
    }

    pub fn end(&self) -> u32 {
        match self {
            Self::Text { end, .. } | Self::Syntax { end, .. } => *end,
        }
    }
}

fn find_chars(chars: &[char], from: usize, end: usize, needle: &[char]) -> Option<usize> {
    if needle.is_empty() || from >= end || needle.len() > end.saturating_sub(from) {
        return None;
    }
    (from..=end - needle.len()).find(|index| chars[*index..].starts_with(needle))
}

fn inline_syntax_at(chars: &[char], index: usize, end: usize) -> Option<NoteInlineNode> {
    let syntax = |kind, start, prefix_end, suffix_start, end, children| NoteInlineNode::Syntax {
        kind,
        start: start as u32,
        prefix_end: prefix_end as u32,
        suffix_start: suffix_start as u32,
        end: end as u32,
        children,
    };

    if chars[index] == '\\' && index + 1 < end {
        return Some(syntax(
            NoteInlineKind::Escape,
            index,
            index + 1,
            index + 2,
            index + 2,
            vec![NoteInlineNode::Text {
                start: (index + 1) as u32,
                end: (index + 2) as u32,
            }],
        ));
    }

    let wiki_open = if chars[index..end].starts_with(&['!', '[', '[']) {
        Some(3)
    } else if chars[index..end].starts_with(&['[', '[']) {
        Some(2)
    } else {
        None
    };
    if let Some(open_len) = wiki_open
        && let Some(close) = find_chars(chars, index + open_len, end, &[']', ']'])
    {
        let label = chars[index + open_len..close]
            .iter()
            .rposition(|character| *character == '|')
            .map_or(index + open_len, |offset| index + open_len + offset + 1);
        return Some(syntax(
            NoteInlineKind::WikiLink,
            index,
            label,
            close,
            close + 2,
            parse_inline_range(chars, label, close),
        ));
    }

    let link_open = if chars[index..end].starts_with(&['!', '[']) {
        Some(2)
    } else if chars[index] == '[' {
        Some(1)
    } else {
        None
    };
    if let Some(open_len) = link_open
        && let Some(label_end) = find_chars(chars, index + open_len, end, &[']', '('])
        && let Some(close) = find_chars(chars, label_end + 2, end, &[')'])
    {
        return Some(syntax(
            NoteInlineKind::Link,
            index,
            index + open_len,
            label_end,
            close + 1,
            parse_inline_range(chars, index + open_len, label_end),
        ));
    }

    if chars[index] == '`' {
        let run = chars[index..end]
            .iter()
            .take_while(|character| **character == '`')
            .count();
        if let Some(close) = find_chars(chars, index + run, end, &vec!['`'; run])
            && close > index + run
        {
            return Some(syntax(
                NoteInlineKind::Code,
                index,
                index + run,
                close,
                close + run,
                vec![NoteInlineNode::Text {
                    start: (index + run) as u32,
                    end: close as u32,
                }],
            ));
        }
    }

    let paired = [
        (&['*', '*'][..], NoteInlineKind::Strong),
        (&['_', '_'][..], NoteInlineKind::Strong),
        (&['~', '~'][..], NoteInlineKind::Strike),
    ];
    for (delimiter, kind) in paired {
        if chars[index..end].starts_with(delimiter)
            && let Some(close) = find_chars(chars, index + delimiter.len(), end, delimiter)
            && close > index + delimiter.len()
        {
            return Some(syntax(
                kind,
                index,
                index + delimiter.len(),
                close,
                close + delimiter.len(),
                parse_inline_range(chars, index + delimiter.len(), close),
            ));
        }
    }

    if matches!(chars[index], '*' | '_') {
        let delimiter = chars[index];
        if let Some(close) = chars[index + 1..end]
            .iter()
            .position(|character| *character == delimiter)
            .map(|offset| index + 1 + offset)
            && close > index + 1
        {
            return Some(syntax(
                NoteInlineKind::Emph,
                index,
                index + 1,
                close,
                close + 1,
                parse_inline_range(chars, index + 1, close),
            ));
        }
    }

    None
}

fn parse_inline_range(chars: &[char], start: usize, end: usize) -> Vec<NoteInlineNode> {
    let mut nodes = Vec::new();
    let mut text_start = start;
    let mut index = start;
    while index < end {
        let Some(node) = inline_syntax_at(chars, index, end) else {
            index += 1;
            continue;
        };
        if text_start < index {
            nodes.push(NoteInlineNode::Text {
                start: text_start as u32,
                end: index as u32,
            });
        }
        index = node.end() as usize;
        text_start = index;
        nodes.push(node);
    }
    if text_start < end {
        nodes.push(NoteInlineNode::Text {
            start: text_start as u32,
            end: end as u32,
        });
    }
    nodes
}

pub fn note_inline_nodes(source: &str, heading_level: Option<u8>) -> Vec<NoteInlineNode> {
    let chars = source.chars().collect::<Vec<_>>();
    let prefix = heading_level
        .map(|level| level as usize)
        .filter(|level| {
            chars.len() > *level
                && chars[..*level].iter().all(|character| *character == '#')
                && chars[*level].is_whitespace()
        })
        .map_or(0, |level| level + 1);
    let children = parse_inline_range(&chars, prefix, chars.len());
    if prefix == 0 {
        children
    } else {
        vec![NoteInlineNode::Syntax {
            kind: NoteInlineKind::BlockMarker,
            start: 0,
            prefix_end: prefix as u32,
            suffix_start: chars.len() as u32,
            end: chars.len() as u32,
            children,
        }]
    }
}

pub fn note_source_offset(source: &str, start_line: u32, line: u32, col: u32) -> u32 {
    let target = line.saturating_sub(start_line) as usize;
    let lines = source.split('\n').collect::<Vec<_>>();
    let before = lines
        .iter()
        .take(target.min(lines.len()))
        .map(|line| line.chars().count() as u32 + 1)
        .sum::<u32>();
    let length = lines
        .get(target)
        .map_or(0, |line| line.chars().count() as u32);
    before + col.min(length)
}

pub fn note_source_position(source: &str, start_line: u32, offset: u32) -> (u32, u32) {
    let mut line = start_line;
    let mut col = 0;
    for character in source.chars().take(offset as usize) {
        if character == '\n' {
            line = line.saturating_add(1);
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub fn centered_scroll_top(target_center: f64, viewport_height: f64) -> f64 {
    (target_center - viewport_height * 0.5).max(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteCursorActivation {
    Center(u32),
    PreserveViewport(u32),
}

pub fn note_cursor_activation(
    reveal_line: Option<u32>,
    restore_vim_cursor: bool,
    cursor_line: u32,
) -> Option<NoteCursorActivation> {
    reveal_line.map(NoteCursorActivation::Center).or_else(|| {
        restore_vim_cursor.then_some(NoteCursorActivation::PreserveViewport(cursor_line))
    })
}

pub fn gutter_width(total_lines: u32) -> usize {
    let digits = total_lines.max(1).to_string().len();
    digits.max(3)
}

#[derive(Clone, Copy)]
struct DisplayCellWidth(u32);

impl From<char> for DisplayCellWidth {
    fn from(character: char) -> Self {
        Self(UnicodeWidthChar::width(character).unwrap_or(0) as u32)
    }
}

impl DisplayCellWidth {
    fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy)]
pub struct DisplayCells<'a> {
    text: &'a str,
}

impl<'a> From<&'a str> for DisplayCells<'a> {
    fn from(text: &'a str) -> Self {
        Self { text }
    }
}

impl DisplayCells<'_> {
    pub fn width(self) -> u32 {
        let mut cells = 0;
        for character in self.text.chars() {
            cells += DisplayCellWidth::from(character).get();
        }
        cells
    }

    pub fn char_at(self, cell: u32) -> usize {
        let mut cells = 0;
        for (index, character) in self.text.chars().enumerate() {
            if cells >= cell {
                return index;
            }
            let width = DisplayCellWidth::from(character).get();
            if cells + width > cell {
                return index;
            }
            cells += width;
        }
        self.text.chars().count()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CellMetrics {
    pub narrow: f64,
    pub wide: f64,
    pub height: f64,
}

impl CellMetrics {
    pub fn measured(self) -> bool {
        self.narrow > 0.0 && self.height > 0.0
    }

    pub fn vars(self) -> String {
        if !self.measured() {
            return String::new();
        }
        format!("--cw:{}px;--ch:{}px;", self.narrow, self.height)
    }

    pub fn wide_advance(self) -> f64 {
        match self.wide > 0.0 {
            true => self.wide,
            false => self.narrow * 2.0,
        }
    }

    fn advance_of(self, ch: char) -> f64 {
        match DisplayCellWidth::from(ch).get() {
            0 => 0.0,
            2 => self.wide_advance(),
            cells => self.narrow * f64::from(cells),
        }
    }
}

pub struct ColumnRuler<'a> {
    text: &'a str,
    metrics: CellMetrics,
}

impl<'a> ColumnRuler<'a> {
    pub fn new(text: &'a str, metrics: CellMetrics) -> Self {
        Self { text, metrics }
    }

    pub fn wrapped_row(text: &'a str, metrics: CellMetrics, wrap_columns: u16, index: u32) -> Self {
        if wrap_columns == 0
            || index == 0 && u32::from(wrap_columns) >= DisplayCells::from(text).width()
        {
            return Self::new(text, metrics);
        }
        let columns = u32::from(wrap_columns);
        let skip = index.saturating_mul(columns);
        let mut start = None;
        let mut end = text.len();
        let mut cells = 0;
        for (at, ch) in text.char_indices() {
            if start.is_none() && cells >= skip {
                start = Some(at);
            }
            if cells >= skip.saturating_add(columns) {
                end = at;
                break;
            }
            cells += DisplayCellWidth::from(ch).get();
        }
        let start = start.unwrap_or(text.len());
        Self::new(&text[start..end.max(start)], metrics)
    }

    pub fn x_of(&self, col: u32) -> f64 {
        let mut cells = 0;
        let mut x = 0.0;
        for ch in self.text.chars() {
            if cells >= col {
                return x;
            }
            let width = DisplayCellWidth::from(ch).get();
            let advance = self.metrics.advance_of(ch);
            if cells + width > col {
                return x + advance * f64::from(col - cells) / f64::from(width);
            }
            cells += width;
            x += advance;
        }
        x + f64::from(col.saturating_sub(cells)) * self.metrics.narrow
    }

    pub fn width_between(&self, start: u32, end: u32) -> f64 {
        (self.x_of(end) - self.x_of(start)).max(0.0)
    }

    pub fn x_of_char(&self, char_col: u32) -> f64 {
        let mut seen = 0;
        let mut x = 0.0;
        for ch in self.text.chars() {
            if seen >= char_col {
                return x;
            }
            seen += 1;
            x += self.metrics.advance_of(ch);
        }
        x + f64::from(char_col - seen) * self.metrics.narrow
    }

    pub fn advance_at(&self, col: u32) -> f64 {
        let mut cells = 0;
        for ch in self.text.chars() {
            let width = DisplayCellWidth::from(ch).get();
            if width == 0 {
                continue;
            }
            if cells >= col {
                return self.metrics.advance_of(ch);
            }
            cells += width;
        }
        self.metrics.narrow
    }

    pub fn col_at(&self, x: f64, snap: bool) -> u32 {
        if x <= 0.0 || !self.metrics.measured() {
            return 0;
        }
        let mut cells = 0;
        let mut at = 0.0;
        for ch in self.text.chars() {
            let width = DisplayCellWidth::from(ch).get();
            if width == 0 {
                continue;
            }
            let advance = self.metrics.advance_of(ch);
            if x < at + advance {
                if !snap {
                    return cells;
                }
                let into = (x - at) / advance;
                return match into < 0.5 {
                    true => cells,
                    false => cells + width,
                };
            }
            cells += width;
            at += advance;
        }
        let past = (x - at) / self.metrics.narrow;
        let extra = match snap {
            true => past.round(),
            false => past.floor(),
        };
        cells + extra.max(0.0) as u32
    }
}

pub fn span_style(span: &StyledSpan) -> String {
    let [r, g, b] = span.fg;
    let mut s = format!("color:rgb({r},{g},{b});");
    if span.bold {
        s.push_str("font-weight:700;");
    }
    if span.italic {
        s.push_str("font-style:italic;");
    }
    s
}

pub fn heading_class(level: u8) -> &'static str {
    match level {
        1 => "mb-3 mt-6 text-3xl font-bold tracking-tight text-foreground",
        2 => {
            "mb-2 mt-5 border-b border-border pb-2 text-2xl font-semibold tracking-tight text-foreground"
        }
        3 => "mb-2 mt-4 text-xl font-semibold text-foreground/95",
        4 => "mb-1 mt-3 text-lg font-semibold text-foreground/90",
        5 => "mb-1 mt-3 text-base font-semibold text-foreground/85",
        _ => "mb-1 mt-3 text-sm font-semibold uppercase tracking-wide text-foreground/70",
    }
}

pub fn table_align_style(align: MdTableAlign) -> &'static str {
    match align {
        MdTableAlign::Left => "text-align:left",
        MdTableAlign::Center => "text-align:center",
        MdTableAlign::Right => "text-align:right",
        MdTableAlign::None => "",
    }
}

pub fn line_severity(diags: &[FileDiagnostic], line: u32) -> Option<DiagSeverity> {
    diags
        .iter()
        .filter(|d| d.line == line)
        .map(|d| d.severity)
        .min_by_key(|s| match s {
            DiagSeverity::Error => 0,
            DiagSeverity::Warning => 1,
            DiagSeverity::Info => 2,
            DiagSeverity::Hint => 3,
        })
}

pub fn severity_color_class(sev: DiagSeverity) -> &'static str {
    match sev {
        DiagSeverity::Error => "text-ansi-1",
        DiagSeverity::Warning => "text-ansi-3",
        DiagSeverity::Info => "text-ansi-4",
        DiagSeverity::Hint => "text-ansi-6",
    }
}

pub fn squiggle_style(left: f64, width: f64, color_rgb: &str) -> String {
    format!(
        "position:absolute;left:{left}px;width:{width}px;bottom:0;height:1.1em;\
         border-bottom:2px solid {color};pointer-events:auto;",
        left = left,
        width = width.max(1.0),
        color = color_rgb,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn append_live_text(
        source: &[char],
        nodes: &[NoteInlineNode],
        caret: u32,
        output: &mut String,
    ) {
        for node in nodes {
            match node {
                NoteInlineNode::Text { start, end } => {
                    output.extend(
                        source[*start as usize..*end as usize]
                            .iter()
                            .map(
                                |character| {
                                    if *character == '\n' { ' ' } else { *character }
                                },
                            ),
                    );
                }
                NoteInlineNode::Syntax {
                    start,
                    prefix_end,
                    suffix_start,
                    end,
                    children,
                    ..
                } => {
                    let reveal = *start <= caret && caret <= *end;
                    if reveal {
                        output.extend(source[*start as usize..*prefix_end as usize].iter());
                    }
                    append_live_text(source, children, caret, output);
                    if reveal {
                        output.extend(source[*suffix_start as usize..*end as usize].iter());
                    }
                }
            }
        }
    }

    fn live_text(source: &str, caret: u32) -> String {
        let chars = source.chars().collect::<Vec<_>>();
        let nodes = note_inline_nodes(source, None);
        let mut output = String::new();
        append_live_text(&chars, &nodes, caret, &mut output);
        output
    }

    #[test]
    fn gutter_width_min_three() {
        assert_eq!(gutter_width(0), 3);
        assert_eq!(gutter_width(9), 3);
        assert_eq!(gutter_width(1000), 4);
        assert_eq!(gutter_width(99999), 5);
    }

    #[test]
    fn span_style_emits_color_and_styles() {
        let s = span_style(&StyledSpan {
            text: "x".into(),
            fg: [10, 20, 30],
            bold: true,
            italic: true,
        });
        assert!(s.contains("color:rgb(10,20,30)"));
        assert!(s.contains("font-weight:700"));
        assert!(s.contains("font-style:italic"));
    }

    #[test]
    fn line_severity_takes_most_severe() {
        let mk = |line, sev| FileDiagnostic {
            line,
            start_col: 0,
            end_col: 1,
            severity: sev,
            message: String::new(),
            source: None,
        };
        let v = vec![mk(3, DiagSeverity::Warning), mk(3, DiagSeverity::Error)];
        assert_eq!(line_severity(&v, 3), Some(DiagSeverity::Error));
        assert_eq!(line_severity(&v, 4), None);
    }

    #[test]
    fn squiggle_style_keeps_a_hit_target_on_an_empty_range() {
        let s = squiggle_style(16.0, 0.0, "rgb(255,0,0)");
        assert!(s.contains("left:16px"));
        assert!(s.contains("width:1px"));
    }

    #[test]
    fn cursor_centering_places_target_at_viewport_midpoint() {
        assert_eq!(centered_scroll_top(500.0, 400.0), 300.0);
        assert_eq!(centered_scroll_top(100.0, 400.0), 0.0);
    }

    #[test]
    fn editor_drag_requires_deliberate_pointer_movement() {
        assert!(!editor_drag_started((100, 100), (103, 102)));
        assert!(editor_drag_started((100, 100), (104, 100)));
        assert!(editor_drag_started((100, 100), (96, 96)));
    }

    #[test]
    fn note_cursor_restore_preserves_viewport_until_explicit_reveal() {
        assert_eq!(
            note_cursor_activation(Some(12), true, 8),
            Some(NoteCursorActivation::Center(12))
        );
        assert_eq!(
            note_cursor_activation(None, true, 8),
            Some(NoteCursorActivation::PreserveViewport(8))
        );
        assert_eq!(note_cursor_activation(None, false, 8), None);
    }

    #[test]
    fn note_list_prefix_excludes_marker_and_task_checkbox() {
        assert_eq!(note_list_marker_prefix_len("- item"), Some((0, 2)));
        assert_eq!(note_list_marker_prefix_len("  12. item"), Some((2, 6)));
        assert_eq!(note_list_marker_prefix_len("- [ ] task"), Some((0, 6)));
        assert_eq!(note_list_marker_prefix_len("  * [x] done"), Some((2, 8)));
        assert_eq!(note_list_marker_prefix_len("paragraph"), None);
    }

    #[test]
    fn note_live_preview_preserves_paragraph_flow() {
        let source = "first line\nsecond line\nthird line";
        assert_eq!(live_text(source, 4), "first line second line third line");
        assert_eq!(note_source_offset(source, 5, 7, 3), 26);
        assert_eq!(note_source_position(source, 5, 26), (7, 3));
    }

    #[test]
    fn note_live_preview_reveals_only_active_inline_syntax() {
        let source = "plain `code` and **bold** with [link](https://vmux.ai)";
        assert_eq!(live_text(source, 2), "plain code and bold with link");
        assert_eq!(live_text(source, 8), "plain `code` and bold with link");
        assert_eq!(live_text(source, 20), "plain code and **bold** with link");
        assert_eq!(
            live_text(source, 35),
            "plain code and bold with [link](https://vmux.ai)"
        );
    }

    #[test]
    fn note_live_preview_uses_wiki_link_label() {
        let source = "See [[projects/vmux|vmux project]] now";
        assert_eq!(live_text(source, 1), "See vmux project now");
        assert_eq!(
            live_text(source, 10),
            "See [[projects/vmux|vmux project]] now"
        );
    }
}

#[cfg(test)]
mod column_tests {
    use super::*;

    const MENLO: CellMetrics = CellMetrics {
        narrow: 8.4287109375,
        wide: 14.0,
        height: 17.0,
    };

    #[test]
    fn a_wide_glyph_is_placed_at_its_measured_advance_not_two_narrow_cells() {
        let ruler = ColumnRuler::new("今日の予定は？", MENLO);

        assert_eq!(ruler.x_of(14), 7.0 * MENLO.wide);
        assert_eq!(ruler.x_of(4), 2.0 * MENLO.wide);
        assert!(
            ruler.x_of(14) < 14.0 * MENLO.narrow,
            "the caret used to sit {} px past the text",
            14.0 * MENLO.narrow - ruler.x_of(14)
        );
    }

    #[test]
    fn a_mixed_line_round_trips_every_character_boundary() {
        let text = "ab今c😀d\u{0301}e";
        let ruler = ColumnRuler::new(text, MENLO);

        assert_eq!(DisplayCells::from(text).width(), 9);

        let mut boundaries = vec![0];
        let mut cells = 0;
        for ch in text.chars() {
            cells += DisplayCellWidth::from(ch).get();
            boundaries.push(cells);
        }
        boundaries.dedup();

        for col in boundaries {
            let x = ruler.x_of(col);
            assert_eq!(
                ruler.col_at(x, true),
                col,
                "column {col} at {x}px did not come back"
            );
        }
    }

    #[test]
    fn a_column_inside_a_wide_glyph_snaps_out_to_a_boundary() {
        let ruler = ColumnRuler::new("ab今c", MENLO);

        assert_eq!(ruler.col_at(ruler.x_of(3), true), 4);
        assert_eq!(ruler.col_at(ruler.x_of(3), false), 2);
    }

    #[test]
    fn x_grows_with_every_column_that_has_width() {
        let ruler = ColumnRuler::new("a今b", MENLO);
        let widths = [
            ruler.x_of(0),
            ruler.x_of(1),
            ruler.x_of(2),
            ruler.x_of(3),
            ruler.x_of(4),
        ];

        assert_eq!(widths[0], 0.0);
        assert_eq!(widths[1], MENLO.narrow);
        assert_eq!(widths[2], MENLO.narrow + MENLO.wide / 2.0);
        assert_eq!(widths[3], MENLO.narrow + MENLO.wide);
        assert_eq!(widths[4], MENLO.narrow * 2.0 + MENLO.wide);
    }

    #[test]
    fn a_click_snaps_to_the_nearer_edge_of_a_wide_glyph() {
        let ruler = ColumnRuler::new("今日", MENLO);

        assert_eq!(ruler.col_at(MENLO.wide * 0.4, true), 0);
        assert_eq!(ruler.col_at(MENLO.wide * 0.6, true), 2);
        assert_eq!(ruler.col_at(MENLO.wide * 0.6, false), 0);
        assert_eq!(ruler.col_at(MENLO.wide * 1.6, false), 2);
    }

    #[test]
    fn a_click_past_the_end_counts_narrow_cells() {
        let ruler = ColumnRuler::new("今", MENLO);

        assert_eq!(ruler.col_at(MENLO.wide + MENLO.narrow * 3.0, false), 5);
    }

    #[test]
    fn a_wrapped_row_measures_only_its_own_segment() {
        let ruler = ColumnRuler::wrapped_row("今日の予定は？", MENLO, 4, 1);

        assert_eq!(ruler.x_of(4), 2.0 * MENLO.wide);
        assert_eq!(ruler.col_at(2.0 * MENLO.wide, true), 4);
    }

    #[test]
    fn cell_and_character_columns_diverge_on_wide_text() {
        assert_eq!(DisplayCells::from("今日の予定は？").width(), 14);
        assert_eq!(DisplayCells::from("今日の予定は？").char_at(14), 7);
        assert_eq!(DisplayCells::from("今日の予定は？").char_at(4), 2);
        assert_eq!(DisplayCells::from("ab今").char_at(3), 2);
        assert_eq!(DisplayCells::from("ab今").char_at(4), 3);
    }
}
