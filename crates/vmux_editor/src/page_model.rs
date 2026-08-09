use std::collections::{HashMap, HashSet};

use vmux_core::event::{
    DiagSeverity, FileDiagnostic, FileDirEntry, LspPkgStatus, MdTableAlign, StyledSpan, TreeRow,
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

pub fn viewport_reveal_delta(
    target_top: f64,
    target_bottom: f64,
    viewport_top: f64,
    viewport_bottom: f64,
) -> f64 {
    if target_top < viewport_top {
        target_top - viewport_top
    } else if target_bottom > viewport_bottom {
        target_bottom - viewport_bottom
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteCaretVisibilityRequest {
    pub block_index: usize,
    pub line: u32,
    pub retry: bool,
}

#[derive(Debug, Default)]
pub struct NoteCaretVisibilityQueue {
    pending: Option<NoteCaretVisibilityRequest>,
    scheduled: bool,
}

impl NoteCaretVisibilityQueue {
    pub fn enqueue(&mut self, request: NoteCaretVisibilityRequest) -> bool {
        self.pending = Some(request);
        if self.scheduled {
            false
        } else {
            self.scheduled = true;
            true
        }
    }

    pub fn take(&mut self) -> Option<NoteCaretVisibilityRequest> {
        self.scheduled = false;
        self.pending.take()
    }
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

pub fn squiggle_style(start_col: u32, end_col: u32, color_rgb: &str) -> String {
    let width = end_col.saturating_sub(start_col).max(1);
    format!(
        "position:absolute;left:calc(var(--cw,1ch) * {start});\
         width:calc(var(--cw,1ch) * {width});bottom:0;height:1.1em;\
         border-bottom:2px solid {color};pointer-events:auto;",
        start = start_col,
        width = width,
        color = color_rgb,
    )
}

#[cfg(test)]
#[path = "page_model.dir_browser.test.rs"]
mod dir_browser_tests;
#[cfg(test)]
#[path = "page_model.test.rs"]
mod tests;
