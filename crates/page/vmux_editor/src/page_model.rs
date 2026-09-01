use std::collections::{HashMap, HashSet};

use unicode_width::UnicodeWidthChar;
use vmux_core::event::{
    DiagSeverity, FileDiagnostic, FileDirEntry, LspPkgStatus, MdTableAlign, OpenEditorItem,
    StyledSpan, TreeRow,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkgAction {
    Install,
    Update,
    Uninstall,
    None,
}

pub fn should_apply_explorer_chrome(
    local_client_id: u64,
    latest_request_id: u64,
    event_client_id: u64,
    event_request_id: u64,
) -> bool {
    event_client_id != local_client_id || event_request_id >= latest_request_id
}

pub fn merge_tree_motion_rows(current: &[TreeRow], next: &[TreeRow]) -> Vec<(TreeRow, bool)> {
    let current_paths: HashSet<&str> = current.iter().map(|row| row.path.as_str()).collect();
    let next_indices: HashMap<&str, usize> = next
        .iter()
        .enumerate()
        .map(|(index, row)| (row.path.as_str(), index))
        .collect();
    let mut exiting_after = vec![Vec::new(); next.len() + 1];
    let mut anchor = 0usize;
    for row in current {
        if let Some(index) = next_indices.get(row.path.as_str()) {
            anchor = index + 1;
        } else {
            exiting_after[anchor].push(row.clone());
        }
    }
    let exiting_count = exiting_after.iter().map(Vec::len).sum::<usize>();
    let mut merged = Vec::with_capacity(next.len() + exiting_count);
    merged.extend(exiting_after[0].drain(..).map(|row| (row, false)));
    for (index, row) in next.iter().cloned().enumerate() {
        let visible = current_paths.contains(row.path.as_str());
        merged.push((row, visible));
        merged.extend(exiting_after[index + 1].drain(..).map(|row| (row, false)));
    }
    merged
}

pub fn pkg_status_label(status: LspPkgStatus) -> &'static str {
    match status {
        LspPkgStatus::Available => "Available",
        LspPkgStatus::OnPath => "On PATH",
        LspPkgStatus::Installing => "Installing…",
        LspPkgStatus::Installed => "Installed",
        LspPkgStatus::Outdated => "Update available",
        LspPkgStatus::Running => "Running",
        LspPkgStatus::Failed => "Failed",
    }
}

pub fn pkg_status_class(status: LspPkgStatus) -> &'static str {
    match status {
        LspPkgStatus::Installed | LspPkgStatus::Running => "text-ansi-2",
        LspPkgStatus::OnPath => "text-ansi-6",
        LspPkgStatus::Installing => "text-ansi-4",
        LspPkgStatus::Outdated => "text-ansi-3",
        LspPkgStatus::Failed => "text-ansi-1",
        LspPkgStatus::Available => "text-muted-foreground",
    }
}

pub fn pkg_action(status: LspPkgStatus, installable: bool) -> PkgAction {
    match status {
        LspPkgStatus::Installed | LspPkgStatus::Running => PkgAction::Uninstall,
        LspPkgStatus::Outdated => PkgAction::Update,
        LspPkgStatus::Installing => PkgAction::None,
        LspPkgStatus::OnPath => PkgAction::None,
        LspPkgStatus::Available | LspPkgStatus::Failed => {
            if installable {
                PkgAction::Install
            } else {
                PkgAction::None
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentClass {
    Dir,
    Image { mime: String },
    Text,
    Other,
}

pub fn image_mime(path: &str) -> Option<&'static str> {
    vmux_core::media::image_mime(path)
}

pub fn classify(path: &str, is_dir: bool) -> ContentClass {
    if is_dir {
        return ContentClass::Dir;
    }
    if let Some(mime) = image_mime(path) {
        return ContentClass::Image {
            mime: mime.to_string(),
        };
    }
    if path.rsplit('/').next().is_some_and(|s| s.contains('.')) {
        ContentClass::Text
    } else {
        ContentClass::Other
    }
}

pub fn clamp_selection(idx: usize, len: usize) -> usize {
    if len == 0 { 0 } else { idx.min(len - 1) }
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

pub fn dir_select_index(entries: &[FileDirEntry], came_from: &str) -> usize {
    let name = came_from
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    if name.is_empty() {
        return 0;
    }
    entries.iter().position(|e| e.name == name).unwrap_or(0)
}

pub fn gutter_width(total_lines: u32) -> usize {
    let digits = total_lines.max(1).to_string().len();
    digits.max(3)
}

pub struct DisplayCells;

impl DisplayCells {
    pub fn of_char(ch: char) -> u32 {
        UnicodeWidthChar::width(ch).unwrap_or(0) as u32
    }

    pub fn of_str(text: &str) -> u32 {
        let mut cells = 0;
        for ch in text.chars() {
            cells += Self::of_char(ch);
        }
        cells
    }

    pub fn char_at(text: &str, cell: u32) -> usize {
        let mut cells = 0;
        for (index, ch) in text.chars().enumerate() {
            if cells >= cell {
                return index;
            }
            let width = Self::of_char(ch);
            if cells + width > cell {
                return index;
            }
            cells += width;
        }
        text.chars().count()
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
        match DisplayCells::of_char(ch) {
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
        if wrap_columns == 0 || index == 0 && u32::from(wrap_columns) >= DisplayCells::of_str(text)
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
            cells += DisplayCells::of_char(ch);
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
            let width = DisplayCells::of_char(ch);
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
            let width = DisplayCells::of_char(ch);
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
            let width = DisplayCells::of_char(ch);
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

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EditorTabItem {
    pub name: String,
    pub context: String,
    pub path: String,
    pub active: bool,
    pub dirty: bool,
}

impl EditorTabItem {
    pub fn all(items: &[OpenEditorItem]) -> Vec<EditorTabItem> {
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for item in items {
            *seen.entry(item.name.as_str()).or_insert(0) += 1;
        }
        let mut tabs = Vec::with_capacity(items.len());
        for item in items {
            let shared = seen.get(item.name.as_str()).copied().unwrap_or(0) > 1;
            let context = match shared {
                true => Self::parent_of(&item.path),
                false => String::new(),
            };
            tabs.push(EditorTabItem {
                name: item.name.clone(),
                context,
                path: item.path.clone(),
                active: item.active,
                dirty: item.dirty,
            });
        }
        tabs
    }

    pub fn element_id(&self) -> String {
        format!("editor-tab-{}", self.path)
    }

    fn parent_of(path: &str) -> String {
        let Some((parent, _)) = path.trim_end_matches('/').rsplit_once('/') else {
            return String::new();
        };
        match parent.rsplit('/').next() {
            Some("") | None => "/".to_string(),
            Some(name) => name.to_string(),
        }
    }
}

#[cfg(test)]
mod editor_tab_tests {
    use super::*;

    fn open(name: &str, path: &str) -> OpenEditorItem {
        OpenEditorItem {
            name: name.to_string(),
            path: path.to_string(),
            active: false,
            dirty: false,
        }
    }

    #[test]
    fn only_a_shared_basename_carries_its_directory() {
        let tabs = EditorTabItem::all(&[
            open("mod.rs", "/w/alpha/mod.rs"),
            open("page.rs", "/w/beta/page.rs"),
            open("mod.rs", "/w/beta/mod.rs"),
        ]);
        assert_eq!(tabs[0].context, "alpha");
        assert_eq!(tabs[1].context, "");
        assert_eq!(tabs[2].context, "beta");
    }

    #[test]
    fn a_shared_basename_at_the_root_names_the_root() {
        let tabs = EditorTabItem::all(&[open("a.rs", "/a.rs"), open("a.rs", "/w/a.rs")]);
        assert_eq!(tabs[0].context, "/");
        assert_eq!(tabs[1].context, "w");
    }
}

#[cfg(test)]
mod dir_browser_tests {
    use super::*;

    fn entry(path: &str, is_dir: bool) -> FileDirEntry {
        FileDirEntry {
            name: path.rsplit('/').next().unwrap().to_string(),
            path: path.to_string(),
            is_dir,
        }
    }

    #[test]
    fn classify_dir_and_image_and_text() {
        assert_eq!(classify("/a/b", true), ContentClass::Dir);
        assert_eq!(
            classify("/a/p.PNG", false),
            ContentClass::Image {
                mime: "image/png".into()
            }
        );
        assert_eq!(classify("/a/main.rs", false), ContentClass::Text);
        assert_eq!(classify("/a/blob", false), ContentClass::Other);
    }

    #[test]
    fn clamp_selection_bounds() {
        assert_eq!(clamp_selection(5, 3), 2);
        assert_eq!(clamp_selection(0, 0), 0);
        assert_eq!(clamp_selection(1, 3), 1);
    }

    #[test]
    fn dir_select_index_matches_came_from_by_basename() {
        let parent = vec![
            entry("/a/x", true),
            entry("/a/.worktrees", true),
            entry("/a/y", false),
        ];
        assert_eq!(dir_select_index(&parent, "/a/.worktrees"), 1);
        assert_eq!(dir_select_index(&parent, "a/.worktrees/"), 1);
        assert_eq!(dir_select_index(&parent, "~/proj/a/.worktrees"), 1);
        assert_eq!(dir_select_index(&parent, "/a/zzz"), 0);
        assert_eq!(dir_select_index(&parent, ""), 0);
    }
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
    fn pkg_action_by_status() {
        assert_eq!(
            pkg_action(LspPkgStatus::Available, true),
            PkgAction::Install
        );
        assert_eq!(pkg_action(LspPkgStatus::Available, false), PkgAction::None);
        assert_eq!(
            pkg_action(LspPkgStatus::Installed, true),
            PkgAction::Uninstall
        );
        assert_eq!(pkg_action(LspPkgStatus::Outdated, true), PkgAction::Update);
        assert_eq!(pkg_action(LspPkgStatus::Installing, true), PkgAction::None);
        assert_eq!(pkg_action(LspPkgStatus::OnPath, true), PkgAction::None);
    }

    #[test]
    fn pkg_status_label_covers_states() {
        assert_eq!(pkg_status_label(LspPkgStatus::OnPath), "On PATH");
        assert_eq!(pkg_status_label(LspPkgStatus::Installed), "Installed");
        assert_eq!(pkg_status_label(LspPkgStatus::Available), "Available");
    }

    #[test]
    fn rapid_explorer_toggle_ignores_stale_echoes() {
        assert!(!should_apply_explorer_chrome(7, 3, 7, 1));
        assert!(!should_apply_explorer_chrome(7, 3, 7, 2));
        assert!(should_apply_explorer_chrome(7, 3, 7, 3));
        assert!(should_apply_explorer_chrome(7, 3, 9, 1));
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

    #[test]
    fn tree_motion_merge_is_linear_ordered_and_marks_entries() {
        let row = |path: &str| TreeRow {
            name: path.to_string(),
            path: path.to_string(),
            depth: 0,
            is_dir: false,
            expanded: false,
            loading: false,
        };
        let current = vec![row("a"), row("b"), row("c"), row("d")];
        let next = vec![row("a"), row("x"), row("d")];
        let merged = merge_tree_motion_rows(&current, &next);
        assert_eq!(
            merged
                .iter()
                .map(|(row, visible)| (row.path.as_str(), *visible))
                .collect::<Vec<_>>(),
            vec![
                ("a", true),
                ("b", false),
                ("c", false),
                ("x", false),
                ("d", true)
            ]
        );
    }

    #[test]
    fn a_tree_arriving_into_nothing_is_entirely_hidden() {
        let row = |path: &str| TreeRow {
            name: path.to_string(),
            path: path.to_string(),
            depth: 0,
            is_dir: false,
            expanded: false,
            loading: false,
        };

        let merged = merge_tree_motion_rows(&[], &[row("a"), row("b")]);

        assert!(
            merged.iter().all(|(_, visible)| !visible),
            "nothing on screen to animate from, so every row is staged for entry"
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

        assert_eq!(DisplayCells::of_str(text), 9);

        let mut boundaries = vec![0];
        let mut cells = 0;
        for ch in text.chars() {
            cells += DisplayCells::of_char(ch);
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
        assert_eq!(DisplayCells::of_str("今日の予定は？"), 14);
        assert_eq!(DisplayCells::char_at("今日の予定は？", 14), 7);
        assert_eq!(DisplayCells::char_at("今日の予定は？", 4), 2);
        assert_eq!(DisplayCells::char_at("ab今", 3), 2);
        assert_eq!(DisplayCells::char_at("ab今", 4), 3);
    }
}
