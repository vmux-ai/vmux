use std::collections::HashMap;

use dioxus::html::geometry::{ClientPoint, ElementPoint};
use dioxus::prelude::*;
use vmux_core::event::{CompletionItem, FilePointerEvent, FilePropertyEdit, MdBlock, NoteBlock};
use vmux_core::knowledge::{KnowledgeProperty, KnowledgePropertyKind};
use vmux_git::event::GitLineStatus;
use vmux_ui::caret::EventSelection;
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::ime::use_ime_guard;
use vmux_ui::platform::sleep_ms;
use vmux_ui::scroll::ScrollIntoView;
use vmux_ui::text_run::TextRun;

use super::{diff_tone, focus_file_input};
use crate::note::{ListEditLine, ListLineHit, MdBlockView, NoteLineChunk, NoteSourceLine};
use crate::page_model::{
    NoteInlineKind, NoteInlineNode, heading_class, note_inline_nodes, note_list_marker_prefix_len,
    note_source_offset, note_source_position,
};

const NOTE_CARET_ID: &str = "note-caret";

#[derive(Clone, Copy, PartialEq)]
pub(super) struct NoteCursor {
    active: Signal<Option<u32>>,
    editing: Signal<bool>,
    edit_line: Signal<Option<u32>>,
}

impl NoteCursor {
    pub(super) fn new() -> Self {
        Self {
            active: use_signal(|| None),
            editing: use_signal(|| false),
            edit_line: use_signal(|| None),
        }
    }

    pub(super) fn active(self) -> Option<u32> {
        (self.active)()
    }

    pub(super) fn editing(self) -> bool {
        (self.editing)()
    }

    pub(super) fn edit_line(self) -> Option<u32> {
        (self.edit_line)()
    }

    pub(super) fn reset(mut self) {
        self.active.set(None);
        self.editing.set(false);
        self.edit_line.set(None);
    }

    pub(super) fn set_active(mut self, active: Option<u32>) {
        self.active.set(active);
    }

    pub(super) fn set_editing(mut self, editing: bool) {
        self.editing.set(editing);
    }

    pub(super) fn set_edit_line(mut self, line: Option<u32>) {
        self.edit_line.set(line);
    }

    pub(super) fn activate(self, block_index: usize, line: u32) {
        self.activate_with_scroll(block_index, line, false);
    }

    pub(super) fn activate_centered(self, block_index: usize, line: u32) {
        self.activate_with_scroll(block_index, line, true);
    }

    pub(super) fn reveal(self, block_index: usize, line: u32) {
        NoteCaretAnchor::new(block_index, line).reveal();
    }

    fn activate_inline(mut self, block_index: usize) {
        self.active.set(Some(block_index as u32));
        self.editing.set(true);
        self.edit_line.set(None);
    }

    fn activate_line(mut self, block_index: usize, line: u32) {
        self.active.set(Some(block_index as u32));
        self.editing.set(true);
        self.edit_line.set(Some(line));
    }

    fn activate_with_scroll(self, block_index: usize, line: u32, center: bool) {
        self.activate_line(block_index, line);
        spawn(async move {
            sleep_ms(0).await;
            focus_file_input();
            if center {
                NoteCaretAnchor::new(block_index, line).center();
            }
        });
    }
}

fn note_pointer_line(
    at: ElementPoint,
    height: f64,
    start: u32,
    end: u32,
    block: &MdBlock,
    list_hit: Option<u32>,
) -> u32 {
    if matches!(block, MdBlock::List { .. })
        && let Some(line) = list_hit
    {
        return line;
    }
    let count = end.saturating_sub(start).max(1);
    if height <= 0.0 {
        return start;
    }
    let ratio = (at.y / height).clamp(0.0, 1.0);

    start + ((ratio * count as f64).floor() as u32).min(count - 1)
}

fn note_edit_block_class(block: &MdBlock) -> &'static str {
    match block {
        MdBlock::Heading { level, .. } => heading_class(*level),
        MdBlock::Paragraph { .. } => "my-3",
        MdBlock::List { .. } => "my-3 pl-6",
        MdBlock::CodeBlock { .. } => {
            "my-4 rounded-xl bg-foreground/[0.05] p-4 font-mono text-xs ring-1 ring-inset ring-border"
        }
        MdBlock::BlockQuote { .. } => {
            "my-4 rounded-r-lg border-l-2 border-primary/50 bg-primary/[0.04] py-1 pl-4 pr-3 text-foreground/70"
        }
        MdBlock::Table { .. } => {
            "my-4 rounded-xl p-3 font-mono text-xs ring-1 ring-inset ring-border"
        }
        MdBlock::ThematicBreak => "my-6",
        MdBlock::Html { .. } => "my-3 whitespace-pre-wrap text-foreground/60",
    }
}

fn note_edit_line_class(block: &MdBlock) -> &'static str {
    if matches!(block, MdBlock::List { .. }) {
        "my-1 min-h-[1lh] w-full whitespace-pre-wrap break-words"
    } else {
        "min-h-[1lh] w-full whitespace-pre-wrap break-words"
    }
}

fn note_edit_overlay_class() -> &'static str {
    "visible absolute inset-0 z-10 cursor-text overflow-visible"
}

pub(super) trait NoteBlocks {
    fn block_index_for_line(&self, line: u32) -> Option<usize>;
    fn blank_line_slot(&self, line: u32) -> Option<usize>;
}

impl NoteBlocks for [NoteBlock] {
    fn block_index_for_line(&self, line: u32) -> Option<usize> {
        self.iter()
            .position(|block| block.start_line <= line && line < block.end_line)
            .or_else(|| self.iter().rposition(|block| block.start_line <= line))
            .or_else(|| (!self.is_empty()).then_some(0))
    }

    fn blank_line_slot(&self, line: u32) -> Option<usize> {
        if self
            .iter()
            .any(|block| block.start_line <= line && line < block.end_line)
        {
            return None;
        }
        Some(
            self.iter()
                .position(|block| line < block.start_line)
                .unwrap_or(self.len()),
        )
    }
}

#[component]
pub(super) fn NoteBlankLine(line: u32, col: u32, keymap: vmux_core::KeymapKind) -> Element {
    let text = " ".repeat(col as usize);
    let chunks = NoteLineChunk::split(&text, Some(col), None);
    let caret_width_class = if keymap == vmux_core::KeymapKind::Vscode {
        "w-px"
    } else {
        "w-[2px]"
    };
    rsx! {
        div {
            id: "note-line-{line}",
            "data-note-edit-line": "{line}",
            class: "my-3 min-h-[1lh] w-full whitespace-pre-wrap break-words",
            NoteSourceLine {
                chunks,
                caret_width_class: caret_width_class.to_string(),
            }
        }
    }
}

fn place_note_caret(element_id: String, line: u32, prefix: u32, at: ClientPoint, extend: bool) {
    spawn(async move {
        let offset = TextRun::in_element(element_id)
            .offset_at(at.x, at.y)
            .await
            .unwrap_or_default();
        let _ = send(&FilePointerEvent {
            line,
            col: prefix + offset,
            extend,
            add: false,
        });
        focus_file_input();
    });
}

fn place_note_block_caret(index: usize, start_line: u32, source: String, at: ClientPoint) {
    spawn(async move {
        let offset = TextRun::in_element(format!("note-live-block-{index}"))
            .offset_at(at.x, at.y)
            .await
            .unwrap_or_default();
        let (line, col) = note_source_position(&source, start_line, offset);
        let _ = send(&FilePointerEvent {
            line,
            col,
            extend: false,
            add: false,
        });
        focus_file_input();
    });
}

#[derive(Clone, PartialEq)]
struct NoteSourceChunk {
    text: String,
    selected: bool,
    caret_before: bool,
}

fn note_source_chunks(
    source: &[char],
    start: u32,
    end: u32,
    caret: u32,
    selections: &[(u32, u32)],
) -> Vec<NoteSourceChunk> {
    let mut boundaries = vec![start, end];
    if start <= caret && caret < end {
        boundaries.push(caret);
    }
    for (selection_start, selection_end) in selections {
        let clipped_start = (*selection_start).clamp(start, end);
        let clipped_end = (*selection_end).clamp(start, end);
        if clipped_start < clipped_end {
            boundaries.push(clipped_start);
            boundaries.push(clipped_end);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
        .windows(2)
        .map(|range| {
            let chunk_start = range[0];
            let chunk_end = range[1];
            NoteSourceChunk {
                text: source[chunk_start as usize..chunk_end as usize]
                    .iter()
                    .map(|character| if *character == '\n' { ' ' } else { *character })
                    .collect(),
                selected: selections.iter().any(|(selection_start, selection_end)| {
                    chunk_start < *selection_end && chunk_end > *selection_start
                }),
                caret_before: caret == chunk_start,
            }
        })
        .collect()
}

fn note_inline_class(kind: NoteInlineKind) -> &'static str {
    match kind {
        NoteInlineKind::BlockMarker | NoteInlineKind::Escape => "",
        NoteInlineKind::Code => {
            "rounded bg-foreground/10 px-1 py-0.5 font-mono text-[0.85em] text-primary"
        }
        NoteInlineKind::Strong => "font-semibold text-foreground",
        NoteInlineKind::Emph => "italic",
        NoteInlineKind::Strike => "line-through opacity-70",
        NoteInlineKind::Link | NoteInlineKind::WikiLink => {
            "text-primary underline decoration-primary/40 underline-offset-2"
        }
    }
}

#[component]
fn NoteCaret(width_class: String) -> Element {
    rsx! {
        span {
            id: NOTE_CARET_ID,
            class: "relative inline-block h-[1.15em] w-0 scroll-mb-8 scroll-mt-8 align-text-bottom",
            span { class: "pointer-events-none absolute inset-y-0 left-0 {width_class} bg-current" }
        }
    }
}

#[component]
fn NoteSourceRange(
    source: Vec<char>,
    start: u32,
    end: u32,
    caret: u32,
    selections: Vec<(u32, u32)>,
    caret_width_class: String,
) -> Element {
    let source = source.as_slice();
    let selections = selections.as_slice();
    let caret_width_class = caret_width_class.as_str();
    let chunks = note_source_chunks(source, start, end, caret, selections);
    rsx! {
        for (index, chunk) in chunks.iter().enumerate() {
            if chunk.caret_before {
                NoteCaret { width_class: caret_width_class.to_string() }
            }
            if !chunk.text.is_empty() {
                span {
                    key: "source-{start}-{index}",
                    class: if chunk.selected { "bg-current/20" } else { "" },
                    "{chunk.text}"
                }
            }
        }
    }
}

#[component]
fn NoteInlineNodes(
    source: Vec<char>,
    nodes: Vec<NoteInlineNode>,
    caret: u32,
    selections: Vec<(u32, u32)>,
    caret_width_class: String,
) -> Element {
    let nodes = nodes.as_slice();
    rsx! {
            for (index, node) in nodes.iter().enumerate() {
                match node {
                    NoteInlineNode::Text { start, end } => rsx! {
                        span { key: "text-{index}",
                            NoteSourceRange {
        source: source.to_vec(),
        start: *start,
        end: *end,
        caret,
        selections: selections.to_vec(),
        caret_width_class: caret_width_class.to_string(),
    }
                        }
                    },
                    NoteInlineNode::Syntax {
                        kind,
                        start,
                        prefix_end,
                        suffix_start,
                        end,
                        children,
                    } => {
                        let reveal = *start <= caret && caret <= *end;
                        rsx! {
                            span { key: "syntax-{index}", class: note_inline_class(*kind),
                                span { class: if reveal { "text-foreground/55" } else { "hidden" },
                                    NoteSourceRange {
        source: source.to_vec(),
        start: *start,
        end: *prefix_end,
        caret,
        selections: selections.to_vec(),
        caret_width_class: caret_width_class.to_string(),
    }
                                }
                                NoteInlineNodes {
        source: source.to_vec(),
        nodes: children.to_vec(),
        caret,
        selections: selections.to_vec(),
        caret_width_class: caret_width_class.to_string(),
    }
                                span { class: if reveal { "text-foreground/55" } else { "hidden" },
                                    NoteSourceRange {
        source: source.to_vec(),
        start: *suffix_start,
        end: *end,
        caret,
        selections: selections.to_vec(),
        caret_width_class: caret_width_class.to_string(),
    }
                                }
                            }
                        }
                    }
                }
            }
        }
}

fn note_selection_ranges(
    source: &str,
    start_line: u32,
    selections: &[vmux_core::editor::SelSpan],
) -> Vec<(u32, u32)> {
    selections
        .iter()
        .map(|selection| {
            let start = note_source_offset(source, start_line, selection.line, selection.start);
            let end_col = if selection.end == u32::MAX {
                source
                    .split('\n')
                    .nth(selection.line.saturating_sub(start_line) as usize)
                    .map_or(0, |line| line.chars().count() as u32)
            } else {
                selection.end
            };
            let end = note_source_offset(source, start_line, selection.line, end_col);
            (start.min(end), start.max(end))
        })
        .filter(|(start, end)| start < end)
        .collect()
}

fn emit_property_edit(
    original_key: String,
    key: String,
    kind: KnowledgePropertyKind,
    values: Vec<String>,
    remove: bool,
) {
    let _ = send(&FilePropertyEdit {
        original_key,
        key,
        kind,
        values,
        remove,
    });
}

fn property_kind_label(kind: KnowledgePropertyKind) -> String {
    match kind {
        KnowledgePropertyKind::Text => translate("editor-property-kind-text"),
        KnowledgePropertyKind::Number => translate("editor-property-kind-number"),
        KnowledgePropertyKind::Checkbox => translate("editor-property-kind-checkbox"),
        KnowledgePropertyKind::Date => translate("editor-property-kind-date"),
        KnowledgePropertyKind::List => translate("editor-property-kind-list"),
        KnowledgePropertyKind::Link => translate("editor-property-kind-link"),
        KnowledgePropertyKind::Tags => translate("editor-property-kind-tags"),
    }
}

fn next_property_kind(kind: KnowledgePropertyKind) -> KnowledgePropertyKind {
    match kind {
        KnowledgePropertyKind::Text => KnowledgePropertyKind::Number,
        KnowledgePropertyKind::Number => KnowledgePropertyKind::Checkbox,
        KnowledgePropertyKind::Checkbox => KnowledgePropertyKind::Date,
        KnowledgePropertyKind::Date => KnowledgePropertyKind::List,
        KnowledgePropertyKind::List => KnowledgePropertyKind::Link,
        KnowledgePropertyKind::Link => KnowledgePropertyKind::Tags,
        KnowledgePropertyKind::Tags => KnowledgePropertyKind::Text,
    }
}

#[component]
pub(super) fn NoteProperties(properties: Vec<KnowledgeProperty>) -> Element {
    let mut open = use_signal(|| !properties.is_empty());
    let has_tags = properties
        .iter()
        .any(|property| property.kind == KnowledgePropertyKind::Tags);
    let add_key = {
        let mut suffix = 1;
        loop {
            let candidate = if suffix == 1 {
                "property".to_string()
            } else {
                format!("property-{suffix}")
            };
            if !properties
                .iter()
                .any(|property| property.key.eq_ignore_ascii_case(&candidate))
            {
                break candidate;
            }
            suffix += 1;
        }
    };
    rsx! {
        div { class: "mb-5 rounded-xl bg-foreground/[0.025] ring-1 ring-inset ring-foreground/[0.07]",
            div { class: "flex h-9 items-center gap-2 px-3",
                button {
                    r#type: "button",
                    class: "flex min-w-0 flex-1 items-center gap-2 text-left text-xs font-medium text-foreground/65 hover:text-foreground",
                    onclick: move |_| open.toggle(),
                    Icon { class: if open() { "h-3.5 w-3.5 rotate-90 transition-transform" } else { "h-3.5 w-3.5 transition-transform" }, path { d: "m9 18 6-6-6-6" } }
                    span { {translate("editor-properties")} }
                    if !properties.is_empty() {
                        span { class: "text-[10px] text-muted-foreground", "{properties.len()}" }
                    }
                }
                button {
                    r#type: "button",
                    title: translate("editor-add-tags"),
                    disabled: has_tags,
                    class: if has_tags { "rounded-md px-1 text-xs text-muted-foreground/30" } else { "rounded-md px-1 text-xs text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground" },
                    onclick: move |_| {
                        open.set(true);
                        emit_property_edit(
                            String::new(),
                            "tags".to_string(),
                            KnowledgePropertyKind::Tags,
                            Vec::new(),
                            false,
                        );
                    },
                    "#"
                }
                button {
                    r#type: "button",
                    title: translate("editor-add-property"),
                    class: "rounded-md p-1 text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground",
                    onclick: move |_| {
                        open.set(true);
                        emit_property_edit(
                            String::new(),
                            add_key.clone(),
                            KnowledgePropertyKind::Text,
                            vec![String::new()],
                            false,
                        );
                    },
                    Icon { class: "h-3.5 w-3.5", path { d: "M12 5v14" } path { d: "M5 12h14" } }
                }
            }
            if open() {
                div { class: "border-t border-foreground/[0.06] px-1 py-1",
                    if properties.is_empty() {
                        div { class: "px-3 py-2 text-xs text-muted-foreground", {translate("editor-no-properties")} }
                    }
                    for property in properties {
                        NotePropertyRow {
                            key: "{property.key}:{property.kind:?}:{property.values:?}",
                            property,
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn NoteBlockView(
    note_blocks: Signal<Vec<NoteBlock>>,
    diff_markers: ReadSignal<HashMap<u32, GitLineStatus>>,
    index: usize,
    editing: bool,
    source_cursor: Signal<vmux_core::editor::CursorPos>,
    source_selections: Signal<Vec<vmux_core::editor::SelSpan>>,
    note_diff_marker: Option<GitLineStatus>,
    keymap: vmux_core::KeymapKind,
    note_cursor: NoteCursor,
    mut note_dragging: Signal<bool>,
    comp_open: bool,
    comp_filtered: Vec<CompletionItem>,
    comp_sel_clamped: usize,
) -> Element {
    let Some(note_block) = note_blocks.read().get(index).cloned() else {
        return rsx! {};
    };
    let current = if editing {
        *source_cursor.read()
    } else {
        vmux_core::editor::CursorPos::default()
    };
    let selections = if editing {
        source_selections.read().clone()
    } else {
        Vec::new()
    };
    let active_edit_line = if editing {
        note_cursor.edit_line().unwrap_or(current.line)
    } else {
        0
    };
    let is_list = matches!(note_block.block, MdBlock::List { .. });
    let is_live_inline = matches!(
        note_block.block,
        MdBlock::Paragraph { .. } | MdBlock::Heading { .. }
    );
    let start = note_block.start_line;
    let end = note_block.end_line;
    let note_diff_marker = note_block_diff_marker(&diff_markers.read(), start, end);
    let source = note_block.source.clone();
    let pointer_source = source.clone();
    let live_pointer_source = source.clone();
    let live_down_source = if editing {
        source.clone()
    } else {
        String::new()
    };
    let pointer_block = note_block.block.clone();
    let edit_lines = if !editing {
        Vec::new()
    } else if is_list {
        let raw = source
            .lines()
            .nth(active_edit_line.saturating_sub(start) as usize)
            .unwrap_or_default();
        let prefix = note_list_marker_prefix_len(raw).map_or(0, |(_, prefix)| prefix);
        vec![(
            active_edit_line,
            raw.chars().skip(prefix).collect::<String>(),
            prefix as u32,
        )]
    } else if source.is_empty() {
        vec![(start, String::new(), 0)]
    } else {
        source
            .lines()
            .enumerate()
            .map(|(offset, raw)| (start + offset as u32, raw.to_string(), 0))
            .collect::<Vec<_>>()
    };
    let edit_class = note_edit_block_class(&note_block.block);
    let heading_level = match &note_block.block {
        MdBlock::Heading { level, .. } => Some(*level),
        _ => None,
    };
    let (live_nodes, live_source, live_caret, live_selections) = if editing && is_live_inline {
        (
            note_inline_nodes(&source, heading_level),
            source.chars().collect::<Vec<_>>(),
            note_source_offset(&source, start, current.line, current.col),
            note_selection_ranges(&source, start, &selections),
        )
    } else {
        (Vec::new(), Vec::new(), 0, Vec::new())
    };
    let caret_width_class = if keymap == vmux_core::KeymapKind::Vscode {
        "w-px"
    } else {
        "w-[2px]"
    };
    let line_chunks = |line: u32, raw: &str, prefix: u32| {
        let selection = selections
            .iter()
            .find(|selection| selection.line == line)
            .map(|selection| vmux_core::editor::SelSpan {
                line: selection.line,
                row: selection.row,
                start: selection.start.saturating_sub(prefix),
                end: if selection.end == u32::MAX {
                    u32::MAX
                } else {
                    selection.end.saturating_sub(prefix)
                },
            });
        NoteLineChunk::split(
            raw,
            (line == current.line).then_some(current.col.saturating_sub(prefix)),
            selection,
        )
    };
    let list_edit = match edit_lines.first() {
        Some((line, raw, prefix)) if is_list => Some(ListEditLine {
            line: *line,
            chunks: line_chunks(*line, raw, *prefix),
            caret_width_class: caret_width_class.to_string(),
        }),
        _ => None,
    };
    let mut block_height = use_signal(|| 0.0f64);
    let ListLineHit(list_hit) = use_context_provider(|| ListLineHit(Signal::new(None)));
    let marker_prefix = move |source: &str, line: u32| {
        let raw = source
            .lines()
            .nth(line.saturating_sub(start) as usize)
            .unwrap_or_default();
        if is_list {
            note_list_marker_prefix_len(raw).map_or(0, |(_, prefix)| prefix as u32)
        } else {
            0
        }
    };
    let down_source = source.clone();
    let on_line_down = use_callback(move |(line, at, extend): (u32, ClientPoint, bool)| {
        note_dragging.set(true);
        place_note_caret(
            format!("note-line-{line}"),
            line,
            marker_prefix(&down_source, line),
            at,
            extend,
        );
    });

    rsx! {
        div {
            id: "note-block-{index}",
            "data-note-block": "{index}",
            class: "relative flow-root w-full cursor-text",
            onresize: move |event: Event<ResizeData>| {
                if let Ok(size) = event.get_border_box_size() {
                    block_height.set(size.height);
                }
            },
            onclick: move |event: Event<MouseData>| {
                if editing && !is_list {
                    return;
                }
                event.stop_propagation();
                if EventSelection::in_document() {
                    return;
                }
                let at = event.client_coordinates();
                if is_live_inline {
                    note_cursor.activate_inline(index);
                    place_note_block_caret(index, start, live_pointer_source.clone(), at);
                    return;
                }
                let line = note_pointer_line(
                    event.element_coordinates(),
                    block_height(),
                    start,
                    end,
                    &pointer_block,
                    list_hit(),
                );
                note_cursor.activate_line(index, line);
                place_note_caret(
                    format!("note-line-{line}"),
                    line,
                    marker_prefix(&pointer_source, line),
                    at,
                    false,
                );
            },
            if let Some(marker) = note_diff_marker {
                span {
                    class: "pointer-events-none absolute -left-4 bottom-1 top-1 w-[3px] rounded-full opacity-80 {note_diff_marker_class(marker)}"
                }
            }
            RenderedNoteBlock {
                block: note_block.block.clone(),
                index,
                hidden_list_line: (editing && is_list).then_some(active_edit_line),
                invisible: editing && !is_list,
                list_edit,
                on_line_down,
            }
            if editing && !is_list {
                div {
                    class: note_edit_overlay_class(),
                    if is_live_inline {
                        div {
                            id: "note-live-block-{index}",
                            "data-note-edit-block": "{index}",
                            class: edit_class,
                            onclick: move |event: Event<MouseData>| {
                                event.stop_propagation();
                                event.prevent_default();
                            },
                            onpointerdown: move |event: Event<PointerData>| {
                                event.stop_propagation();
                                event.prevent_default();
                                note_dragging.set(true);
                                place_note_block_caret(
                                    index,
                                    start,
                                    live_down_source.clone(),
                                    event.client_coordinates(),
                                );
                            },
                            onmousedown: move |event: Event<MouseData>| {
                                event.stop_propagation();
                                event.prevent_default();
                            },
                            span {
                                "data-note-line-text": "true",
                                class: "inline",
                                NoteInlineNodes {
                                    source: live_source.clone(),
                                    nodes: live_nodes.clone(),
                                    caret: live_caret,
                                    selections: live_selections.clone(),
                                    caret_width_class: caret_width_class.to_string(),
                                }
                                if live_caret == live_source.len() as u32 {
                                    NoteCaret { width_class: caret_width_class.to_string() }
                                }
                            }
                        }
                    } else {
                        div {
                            class: if is_list { "" } else { edit_class },
                            for (line, raw, prefix) in edit_lines.iter() {
                                {
                                    let line = *line;
                                    let prefix = *prefix;
                                    let chunks = line_chunks(line, raw, prefix);
                                    let line_class = if is_list {
                                        "min-h-[1lh] w-full whitespace-pre-wrap break-words"
                                    } else {
                                        note_edit_line_class(&note_block.block)
                                    };
                                    rsx! {
                                        div {
                                            key: "{line}",
                                            id: "note-line-{line}",
                                            "data-note-edit-line": "{line}",
                                            class: line_class,
                                            onclick: move |event: Event<MouseData>| {
                                                event.stop_propagation();
                                                event.prevent_default();
                                            },
                                            onpointerdown: move |event: Event<PointerData>| {
                                                event.stop_propagation();
                                                event.prevent_default();
                                                on_line_down.call((
                                                    line,
                                                    event.client_coordinates(),
                                                    event.modifiers().shift(),
                                                ));
                                            },
                                            onpointermove: move |event: Event<PointerData>| {
                                                if !note_dragging() {
                                                    return;
                                                }
                                                on_line_down.call((
                                                    line,
                                                    event.client_coordinates(),
                                                    true,
                                                ));
                                            },
                                            onmousedown: move |event: Event<MouseData>| {
                                                event.stop_propagation();
                                                event.prevent_default();
                                            },
                                            span {
                                                "data-note-line-text": "true",
                                                class: "inline-block min-w-[1ch]",
                                                for (chunk_index, chunk) in chunks.iter().enumerate() {
                                                    if chunk.caret_before {
                                                        span {
                                                            key: "caret-{chunk_index}",
                                                            id: NOTE_CARET_ID,
                                                            class: "relative inline-block h-[1.15em] w-0 scroll-mb-8 scroll-mt-8 align-text-bottom",
                                                            span { class: "pointer-events-none absolute inset-y-0 left-0 {caret_width_class} bg-current" }
                                                        }
                                                    }
                                                    if !chunk.text.is_empty() {
                                                        span {
                                                            key: "text-{chunk_index}",
                                                            class: if chunk.selected { "bg-primary/20" } else { "" },
                                                            "{chunk.text}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if comp_open && !comp_filtered.is_empty() {
                        div {
                            class: "absolute left-0 top-full z-40 mt-1 max-h-56 min-w-56 overflow-auto rounded-lg bg-background/95 py-1 text-xs text-foreground/90 ring-1 ring-inset ring-primary/20 backdrop-blur-2xl shadow-lg",
                            for (item_index, item) in comp_filtered.iter().enumerate() {
                                div {
                                    key: "note-completion-{item_index}",
                                    class: if item_index == comp_sel_clamped { "flex items-center gap-2 bg-primary/15 px-3 py-1" } else { "flex items-center gap-2 px-3 py-1" },
                                    span { class: "truncate", "{item.label}" }
                                    span { class: "ml-auto truncate text-[10px] text-foreground/40", "{item.detail}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn note_diff_marker_class(marker: GitLineStatus) -> &'static str {
    diff_tone(marker).marker_class()
}

fn note_block_diff_marker(
    markers: &HashMap<u32, GitLineStatus>,
    start_line: u32,
    end_line: u32,
) -> Option<GitLineStatus> {
    let priority = |marker| match marker {
        GitLineStatus::Staged => 0,
        GitLineStatus::Deleted => 1,
        GitLineStatus::Added => 2,
        GitLineStatus::Modified => 3,
    };
    (start_line..=end_line)
        .filter_map(|line| markers.get(&(line + 1)).copied())
        .max_by_key(|marker| priority(*marker))
}

#[component]
fn NotePropertyRow(property: KnowledgeProperty) -> Element {
    let original_key = property.key.clone();
    let kind = property.kind;
    let mut key = use_signal(|| property.key.clone());
    let mut scalar = use_signal(|| property.values.first().cloned().unwrap_or_default());
    let mut item = use_signal(String::new);
    let ime = use_ime_guard();
    let values = property.values.clone();
    let key_for_kind = original_key.clone();
    let key_for_delete = original_key.clone();
    rsx! {
        div { class: "group flex min-h-9 items-start gap-2 rounded-lg px-2 py-1.5 hover:bg-foreground/[0.035]",
            input {
                value: "{key}",
                class: "w-28 shrink-0 bg-transparent text-xs font-medium text-foreground/65 outline-none focus:text-foreground",
                oninput: move |event| key.set(event.value()),
                onblur: {
                    let original_key = original_key.clone();
                    let values = values.clone();
                    move |_| emit_property_edit(original_key.clone(), key(), kind, values.clone(), false)
                },
            }
            button {
                r#type: "button",
                title: translate("editor-change-property-type"),
                class: "shrink-0 rounded-md bg-foreground/[0.05] px-1.5 py-0.5 text-[9px] uppercase tracking-wide text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
                onclick: {
                    let values = values.clone();
                    move |_| {
                        let next = next_property_kind(kind);
                        emit_property_edit(key_for_kind.clone(), key(), next, values.clone(), false);
                    }
                },
                {property_kind_label(kind)}
            }
            div { class: "min-w-0 flex-1",
                if kind == KnowledgePropertyKind::Checkbox {
                    button {
                        r#type: "button",
                        class: if scalar().eq_ignore_ascii_case("true") { "flex h-5 w-9 items-center justify-end rounded-full bg-primary px-0.5" } else { "flex h-5 w-9 items-center justify-start rounded-full bg-foreground/15 px-0.5" },
                        onclick: {
                            let original_key = original_key.clone();
                            move |_| {
                                let next = (!scalar().eq_ignore_ascii_case("true")).to_string();
                                scalar.set(next.clone());
                                emit_property_edit(original_key.clone(), key(), kind, vec![next], false);
                            }
                        },
                        span { class: "h-4 w-4 rounded-full bg-background shadow-sm" }
                    }
                } else if matches!(kind, KnowledgePropertyKind::List | KnowledgePropertyKind::Tags) {
                    div { class: "flex flex-wrap items-center gap-1",
                        for (index, value) in values.iter().enumerate() {
                            {
                                let remove_key = original_key.clone();
                                let remove_values = values.clone();
                                rsx! {
                                    button {
                                        key: "{index}:{value}",
                                        r#type: "button",
                                        title: translate("common-remove"),
                                        class: if kind == KnowledgePropertyKind::Tags { "rounded-full bg-primary/10 px-2 py-0.5 text-[11px] text-primary hover:bg-destructive/10 hover:text-destructive" } else { "rounded-md bg-foreground/[0.06] px-2 py-0.5 text-[11px] text-foreground/75 hover:bg-destructive/10 hover:text-destructive" },
                                        onclick: move |_| {
                                            let mut next = remove_values.clone();
                                            next.remove(index);
                                            emit_property_edit(remove_key.clone(), key(), kind, next, false);
                                        },
                                        if kind == KnowledgePropertyKind::Tags { "#" }
                                        "{value}"
                                    }
                                }
                            }
                        }
                        input {
                            value: "{item}",
                            placeholder: if kind == KnowledgePropertyKind::Tags { translate("editor-add-tag") } else { translate("editor-add-item") },
                            class: "min-w-20 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground/60",
                            oninput: move |event| item.set(event.value()),
                            oncompositionstart: move |_| ime.start(),
                            oncompositionend: move |_| ime.commit(),
                            onkeydown: {
                                let add_key = original_key.clone();
                                let add_values = values.clone();
                                move |event: Event<KeyboardData>| {
                                    if ime.swallows(&event) {
                                        return;
                                    }
                                    if event.key() != Key::Enter {
                                        return;
                                    }
                                    event.prevent_default();
                                    let value = item().trim().trim_start_matches('#').to_string();
                                    if value.is_empty() {
                                        return;
                                    }
                                    let mut next = add_values.clone();
                                    if !next.iter().any(|existing| existing.eq_ignore_ascii_case(&value)) {
                                        next.push(value);
                                    }
                                    item.set(String::new());
                                    emit_property_edit(add_key.clone(), key(), kind, next, false);
                                }
                            },
                        }
                    }
                } else {
                    input {
                        r#type: match kind {
                            KnowledgePropertyKind::Number => "number",
                            KnowledgePropertyKind::Date => "date",
                            _ => "text",
                        },
                        value: "{scalar}",
                        placeholder: if kind == KnowledgePropertyKind::Link { translate("editor-linked-note") } else { translate("editor-property-value") },
                        class: "w-full bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground/60",
                        oninput: move |event| scalar.set(event.value()),
                        onblur: {
                            let original_key = original_key.clone();
                            move |_| emit_property_edit(original_key.clone(), key(), kind, vec![scalar()], false)
                        },
                    }
                }
            }
            button {
                r#type: "button",
                title: translate("editor-delete-property"),
                class: "invisible shrink-0 rounded p-0.5 text-muted-foreground hover:bg-destructive/10 hover:text-destructive group-hover:visible",
                onclick: move |_| emit_property_edit(key_for_delete.clone(), String::new(), kind, Vec::new(), true),
                Icon { class: "h-3.5 w-3.5", path { d: "M18 6 6 18" } path { d: "m6 6 12 12" } }
            }
        }
    }
}

#[component]
fn RenderedNoteBlock(
    block: MdBlock,
    index: usize,
    hidden_list_line: Option<u32>,
    invisible: bool,
    #[props(default)] list_edit: Option<ListEditLine>,
    #[props(default)] on_line_down: Option<EventHandler<(u32, ClientPoint, bool)>>,
) -> Element {
    rsx! {
        div { class: if invisible { "invisible" } else { "" },
            MdBlockView {
                block: block.clone(),
                block_key: index,
                hidden_list_line,
                list_edit,
                on_line_down,
            }
        }
    }
}

struct NoteCaretAnchor([String; 4]);

impl NoteCaretAnchor {
    fn new(block_index: usize, line: u32) -> Self {
        Self([
            NOTE_CARET_ID.to_string(),
            format!("note-line-{line}"),
            format!("note-live-block-{block_index}"),
            format!("note-block-{block_index}"),
        ])
    }

    fn reveal(&self) {
        ScrollIntoView::first_rendered(&self.0.each_ref().map(String::as_str));
    }

    fn center(&self) {
        ScrollIntoView::first_rendered_centered(&self.0.each_ref().map(String::as_str));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(start_line: u32, end_line: u32) -> NoteBlock {
        NoteBlock {
            start_line,
            end_line,
            source: "text".into(),
            block: MdBlock::Paragraph {
                inlines: Vec::new(),
            },
        }
    }

    #[test]
    fn a_blank_source_line_gets_its_own_visual_slot() {
        let blocks = vec![block(1, 2), block(4, 5)];

        assert_eq!(blocks.as_slice().blank_line_slot(0), Some(0));
        assert_eq!(blocks.as_slice().blank_line_slot(2), Some(1));
        assert_eq!(blocks.as_slice().blank_line_slot(5), Some(2));
        assert_eq!(blocks.as_slice().blank_line_slot(1), None);
        assert_eq!(blocks.as_slice().blank_line_slot(4), None);
    }
}
