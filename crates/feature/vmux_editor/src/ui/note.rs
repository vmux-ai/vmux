use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use dioxus::html::geometry::{ClientPoint, ElementPoint};
use dioxus::prelude::*;
use vmux_ecs::event::{
    CompletionItem, FileNoteEditDismissRequest, FileNoteEditRequest,
    FileNotePropertiesToggleRequest, FilePanelPick, FilePointerEvent, FilePropertyDraftRequest,
    FilePropertyDraftState, FilePropertyEdit, MdBlock, NoteBlock,
};
use vmux_git::event::GitLineStatus;
use vmux_knowledge::{KnowledgeProperty, KnowledgePropertyKind};
use vmux_ui::components::icon::Icon;
use vmux_ui::dom::DomSelection;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::ime::use_ime_guard;
use vmux_ui::platform::Platform;
use vmux_ui::scroll::ScrollIntoView;
use vmux_ui::text_run::TextRun;

use super::EditorFocus;
use super::markdown::{
    CaretStyle, HeadingStyle, ListEditLine, ListLineHit, MdBlockView, NoteLineChunk, NoteSourceLine,
};
use super::note_text::{NoteInlineKind, NoteInlineNode, NoteText};
use super::text_style::GitLineTone;

const NOTE_CARET_ID: &str = "note-caret";

#[derive(Clone, Copy, PartialEq)]
pub(super) struct NoteCursor {
    active: Memo<Option<u32>>,
    editing: Memo<bool>,
    edit_line: Memo<Option<u32>>,
    mounted: Signal<BTreeMap<usize, Rc<MountedData>>>,
}

impl NoteCursor {
    fn place_rendered(
        self,
        index: usize,
        start: u32,
        source: String,
        at: ClientPoint,
        extend: bool,
    ) {
        spawn(async move {
            let Some(offset) = TextRun::in_element(format!("note-block-{index}"))
                .offset_at(at.x, at.y)
                .await
            else {
                return;
            };
            let (line, col) = NoteText::new(&source).rendered_position(start, offset);
            self.edit();
            let _ = send(&FilePointerEvent {
                line,
                col,
                extend,
                add: false,
            });
            EditorFocus::file();
        });
    }

    pub(super) fn new(
        active: Memo<Option<u32>>,
        editing: Memo<bool>,
        edit_line: Memo<Option<u32>>,
    ) -> Self {
        Self {
            active,
            editing,
            edit_line,
            mounted: use_signal(BTreeMap::new),
        }
    }

    fn mounted(mut self, index: usize, element: Rc<MountedData>) {
        self.mounted.write().insert(index, element);
    }

    fn unmounted(mut self, index: usize) {
        if let Ok(mut elements) = self.mounted.try_write() {
            elements.remove(&index);
        }
    }

    pub(super) fn place_nearest(self, blocks: Vec<NoteBlock>, at: ClientPoint, extend: bool) {
        spawn(async move {
            let elements = self
                .mounted
                .peek()
                .iter()
                .map(|(index, element)| (*index, Rc::clone(element)))
                .collect::<Vec<_>>();
            let mut low = 0;
            let mut high = elements.len();
            let mut nearest = None;
            let mut distance = f64::INFINITY;
            while low < high {
                let middle = (low + high) / 2;
                let (index, element) = &elements[middle];
                let Ok(rect) = element.get_client_rect().await else {
                    break;
                };
                let top = rect.origin.y;
                let bottom = top + rect.size.height;
                let delta = (top - at.y).max(at.y - bottom).max(0.0);
                if delta < distance {
                    distance = delta;
                    nearest = Some((*index, rect));
                }
                if at.y < top {
                    high = middle;
                } else if at.y > bottom {
                    low = middle + 1;
                } else {
                    break;
                }
            }
            let Some((index, rect)) = nearest else {
                return;
            };
            let Some(block) = blocks.get(index) else {
                return;
            };
            let point = ClientPoint::new(
                at.x.clamp(rect.origin.x, rect.origin.x + rect.size.width.max(1.0)),
                at.y.clamp(rect.origin.y, rect.origin.y + rect.size.height.max(1.0)),
            );
            if self.editing() && self.active() == Some(index as u32) {
                self.edit();
                place_note_block_caret(index, block.start_line, block.source.clone(), point);
            } else {
                self.place_rendered(index, block.start_line, block.source.clone(), point, extend);
            }
        });
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

    pub(super) fn reset(self) {
        let _ = send(&FileNoteEditDismissRequest);
    }

    pub(super) fn set_editing(self, editing: bool) {
        if editing {
            let _ = send(&FileNoteEditRequest);
        } else {
            let _ = send(&FileNoteEditDismissRequest);
        }
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

    fn edit(self) {
        let _ = send(&FileNoteEditRequest);
    }

    fn activate_with_scroll(self, block_index: usize, line: u32, center: bool) {
        self.edit();
        spawn(async move {
            Platform::sleep(0).await;
            EditorFocus::file();
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
        MdBlock::Heading { level, .. } => HeadingStyle::class(*level),
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
    "visible absolute inset-0 z-10 flex items-center cursor-text overflow-visible"
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
pub(super) fn NoteBlankLine(line: u32, col: u32, keymap: vmux_api::editor::KeymapKind) -> Element {
    let text = " ".repeat(col as usize);
    let chunks = NoteLineChunk::split(&text, Some(col), None);
    let caret_width_class = CaretStyle::width(keymap);
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
        let Some(offset) = TextRun::in_element(element_id).offset_at(at.x, at.y).await else {
            return;
        };
        let _ = send(&FilePointerEvent {
            line,
            col: prefix + offset,
            extend,
            add: false,
        });
        EditorFocus::file();
    });
}

fn place_rendered_list_caret(line: u32, source: String, at: ClientPoint) {
    spawn(async move {
        let Some(offset) = TextRun::in_element(format!("note-list-line-{line}"))
            .offset_at(at.x, at.y)
            .await
        else {
            return;
        };
        let (line, col) = NoteText::new(&source).rendered_position(line, offset);
        let _ = send(&FilePointerEvent {
            line,
            col,
            extend: false,
            add: false,
        });
        EditorFocus::file();
    });
}

fn place_note_block_caret(index: usize, start_line: u32, source: String, at: ClientPoint) {
    spawn(async move {
        let Some(offset) = TextRun::in_element(format!("note-live-block-{index}"))
            .offset_at(at.x, at.y)
            .await
        else {
            return;
        };
        let (line, col) = NoteText::new(&source).cell_position(start_line, offset);
        let _ = send(&FilePointerEvent {
            line,
            col,
            extend: false,
            add: false,
        });
        EditorFocus::file();
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
    let caret_class = CaretStyle::BAR;
    rsx! {
        span {
            id: NOTE_CARET_ID,
            class: "relative inline-block h-[1.15em] w-0 scroll-mb-8 scroll-mt-8 align-text-bottom",
            span { class: "{caret_class} {width_class}" }
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
    selections: &[vmux_api::editor::SelSpan],
) -> Vec<(u32, u32)> {
    let text = NoteText::new(source);
    selections
        .iter()
        .map(|selection| {
            let start = text.offset(start_line, selection.line, selection.start);
            let end_col = if selection.end == u32::MAX {
                source
                    .split('\n')
                    .nth(selection.line.saturating_sub(start_line) as usize)
                    .map_or(0, |line| line.chars().count() as u32)
            } else {
                selection.end
            };
            let end = text.offset(start_line, selection.line, end_col);
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
pub(super) fn NoteProperties(
    properties: Vec<KnowledgeProperty>,
    open: bool,
    drafts: Vec<FilePropertyDraftState>,
) -> Element {
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
        div { class: "mb-3 rounded-lg bg-foreground/[0.025] ring-1 ring-inset ring-foreground/[0.07]",
            div { class: "flex h-8 items-center gap-2 px-3",
                button {
                    r#type: "button",
                    class: "flex min-w-0 flex-1 items-center gap-2 text-left text-xs font-medium text-foreground/65 hover:text-foreground",
                    onclick: move |_| {
                        let _ = send(&FileNotePropertiesToggleRequest);
                    },
                    Icon { class: if open { "h-3.5 w-3.5 rotate-90 transition-transform" } else { "h-3.5 w-3.5 transition-transform" }, path { d: "m9 18 6-6-6-6" } }
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
                        if !open {
                            let _ = send(&FileNotePropertiesToggleRequest);
                        }
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
                        if !open {
                            let _ = send(&FileNotePropertiesToggleRequest);
                        }
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
            if open {
                div { class: "border-t border-foreground/[0.06] px-1 py-1",
                    if properties.is_empty() {
                        div { class: "px-3 py-2 text-xs text-muted-foreground", {translate("editor-no-properties")} }
                    }
                    for property in properties {
                        {
                            let draft = drafts
                                .iter()
                                .find(|draft| draft.original_key == property.key)
                                .cloned();
                            rsx! {
                                NotePropertyRow {
                                    key: "{property.key}:{property.kind:?}:{property.values:?}",
                                    property,
                                    draft,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn NoteBlockView(
    note_blocks: Memo<Vec<NoteBlock>>,
    diff_markers: ReadSignal<HashMap<u32, GitLineStatus>>,
    index: usize,
    editing: bool,
    source_cursor: ReadSignal<vmux_api::editor::CursorPos>,
    source_selections: ReadSignal<Vec<vmux_api::editor::SelSpan>>,
    note_diff_marker: Option<GitLineStatus>,
    keymap: vmux_api::editor::KeymapKind,
    note_cursor: NoteCursor,
    mut note_dragging: Signal<bool>,
    comp_open: bool,
    comp_filtered: Vec<CompletionItem>,
    comp_sel_clamped: usize,
) -> Element {
    use_drop(move || note_cursor.unmounted(index));
    let Some(note_block) = note_blocks.read().get(index).cloned() else {
        return rsx! {};
    };
    let current = if editing {
        *source_cursor.read()
    } else {
        vmux_api::editor::CursorPos::default()
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
        let prefix = NoteText::list_marker_prefix_len(raw).map_or(0, |(_, prefix)| prefix);
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
            NoteText::new(&source).inline_nodes(heading_level),
            source.chars().collect::<Vec<_>>(),
            NoteText::new(&source).offset(start, current.line, current.col),
            note_selection_ranges(&source, start, &selections),
        )
    } else {
        (Vec::new(), Vec::new(), 0, Vec::new())
    };
    let caret_width_class = CaretStyle::width(keymap);
    let caret_class = CaretStyle::BAR;
    let line_chunks = |line: u32, raw: &str, prefix: u32| {
        let selection = selections
            .iter()
            .find(|selection| selection.line == line)
            .map(|selection| vmux_api::editor::SelSpan {
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
            NoteText::list_marker_prefix_len(raw).map_or(0, |(_, prefix)| prefix as u32)
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
            "data-vmux-scroll-section": matches!(note_block.block, MdBlock::Heading { .. }).then(|| start.to_string()),
            class: "relative flow-root w-full cursor-text",
            onmounted: move |event: Event<MountedData>| note_cursor.mounted(index, event.data()),
            onresize: move |event: Event<ResizeData>| {
                if let Ok(size) = event.get_border_box_size() {
                    block_height.set(size.height);
                }
            },
            onclick: move |event: Event<MouseData>| {
                event.stop_propagation();
                if editing && !is_list {
                    note_cursor.edit();
                    place_note_block_caret(index, start, live_pointer_source.clone(), event.client_coordinates());
                    return;
                }
                if DomSelection::in_document() {
                    return;
                }
                let at = event.client_coordinates();
                if !editing {
                    note_cursor.place_rendered(index, start, pointer_source.clone(), at, event.modifiers().shift());
                    return;
                }
                if is_live_inline {
                    note_cursor.edit();
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
                note_cursor.edit();
                if is_list && line != active_edit_line {
                    let source = pointer_source
                        .lines()
                        .nth(line.saturating_sub(start) as usize)
                        .unwrap_or_default()
                        .to_string();
                    place_rendered_list_caret(line, source, at);
                    return;
                }
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
                    class: "pointer-events-none absolute -left-4 bottom-1 top-1 w-[3px] rounded-full opacity-80 {GitLineTone::of(marker).marker_class()}"
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
                            class: "w-full min-w-0 shrink-0 {edit_class}",
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
                            class: "w-full min-w-0 shrink-0 {edit_class}",
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
                                                            span { class: "{caret_class} {caret_width_class}" }
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
                                    onmousedown: move |event: Event<MouseData>| {
                                        event.prevent_default();
                                        let _ = send(&FilePanelPick { index: item_index as u32 });
                                    },
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
fn NotePropertyRow(property: KnowledgeProperty, draft: Option<FilePropertyDraftState>) -> Element {
    let original_key = property.key.clone();
    let kind = property.kind;
    let draft = draft.unwrap_or_else(|| FilePropertyDraftState {
        original_key: original_key.clone(),
        key: property.key.clone(),
        scalar: property.values.first().cloned().unwrap_or_default(),
        item: String::new(),
    });
    let key = draft.key.clone();
    let scalar = draft.scalar.clone();
    let item = draft.item.clone();
    let ime = use_ime_guard();
    let values = property.values.clone();
    let key_for_kind = original_key.clone();
    let key_for_delete = original_key.clone();
    rsx! {
        div { class: "group flex min-h-9 items-start gap-2 rounded-lg px-2 py-1.5 hover:bg-foreground/[0.035]",
            input {
                value: "{key}",
                class: "w-28 shrink-0 bg-transparent text-xs font-medium text-foreground/65 outline-none focus:text-foreground",
                oninput: {
                    let draft = draft.clone();
                    move |event: FormEvent| {
                        let mut draft = draft.clone();
                        draft.key = event.value();
                        let _ = send(&FilePropertyDraftRequest {
                            draft,
                        });
                    }
                },
                onblur: {
                    let original_key = original_key.clone();
                    let property_key = key.clone();
                    let values = values.clone();
                    move |_| emit_property_edit(original_key.clone(), property_key.clone(), kind, values.clone(), false)
                },
            }
            button {
                r#type: "button",
                title: translate("editor-change-property-type"),
                class: "shrink-0 rounded-md bg-foreground/[0.05] px-1.5 py-0.5 text-[9px] uppercase tracking-wide text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
                onclick: {
                    let values = values.clone();
                    let property_key = key.clone();
                    move |_| {
                        let next = next_property_kind(kind);
                        emit_property_edit(key_for_kind.clone(), property_key.clone(), next, values.clone(), false);
                    }
                },
                {property_kind_label(kind)}
            }
            div { class: "min-w-0 flex-1",
                if kind == KnowledgePropertyKind::Checkbox {
                    button {
                        r#type: "button",
                        class: if scalar.eq_ignore_ascii_case("true") { "flex h-5 w-9 items-center justify-end rounded-full bg-primary px-0.5" } else { "flex h-5 w-9 items-center justify-start rounded-full bg-foreground/15 px-0.5" },
                        onclick: {
                            let original_key = original_key.clone();
                            let property_key = key.clone();
                            let checkbox_scalar = scalar.clone();
                            move |_| {
                                let next = (!checkbox_scalar.eq_ignore_ascii_case("true")).to_string();
                                emit_property_edit(original_key.clone(), property_key.clone(), kind, vec![next], false);
                            }
                        },
                        span { class: "h-4 w-4 rounded-full bg-background shadow-sm" }
                    }
                } else if matches!(kind, KnowledgePropertyKind::List | KnowledgePropertyKind::Tags) {
                    div { class: "flex flex-wrap items-center gap-1",
                        for (index, value) in values.iter().enumerate() {
                            {
                                let remove_key = original_key.clone();
                                let property_key = key.clone();
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
                                            emit_property_edit(remove_key.clone(), property_key.clone(), kind, next, false);
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
                            oninput: {
                                let draft = draft.clone();
                                move |event: FormEvent| {
                                    let mut draft = draft.clone();
                                    draft.item = event.value();
                                    let _ = send(&FilePropertyDraftRequest {
                                        draft,
                                    });
                                }
                            },
                            oncompositionstart: move |_| ime.start(),
                            oncompositionend: move |_| ime.commit(),
                            onkeydown: {
                                let add_key = original_key.clone();
                                let property_key = key.clone();
                                let draft_item = item.clone();
                                let add_values = values.clone();
                                move |event: Event<KeyboardData>| {
                                    if ime.swallows(&event) {
                                        return;
                                    }
                                    if event.key() != Key::Enter {
                                        return;
                                    }
                                    event.prevent_default();
                                    let value = draft_item.trim().trim_start_matches('#').to_string();
                                    if value.is_empty() {
                                        return;
                                    }
                                    let mut next = add_values.clone();
                                    if !next.iter().any(|existing| existing.eq_ignore_ascii_case(&value)) {
                                        next.push(value);
                                    }
                                    emit_property_edit(add_key.clone(), property_key.clone(), kind, next, false);
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
                        oninput: {
                            let draft = draft.clone();
                            move |event: FormEvent| {
                                let mut draft = draft.clone();
                                draft.scalar = event.value();
                                let _ = send(&FilePropertyDraftRequest {
                                    draft,
                                });
                            }
                        },
                        onblur: {
                            let original_key = original_key.clone();
                            let property_key = key.clone();
                            let property_scalar = scalar.clone();
                            move |_| emit_property_edit(original_key.clone(), property_key.clone(), kind, vec![property_scalar.clone()], false)
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
    use dioxus::core::{AttributeValue, Mutation, VirtualDom};
    use dioxus::html::{EventData, SerializedMouseData, SerializedPointerData};
    use std::{cell::RefCell, future::Future, rc::Rc};
    use vmux_api::BinEvent;
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostScope, PageHost, TextOffsetAnswer};

    #[derive(Default)]
    struct PointerHost {
        events: RefCell<Vec<String>>,
        positions: RefCell<Vec<FilePointerEvent>>,
        measured_elements: RefCell<std::collections::HashSet<String>>,
    }

    impl PageHost for PointerHost {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            self.events.borrow_mut().push(id.to_string());
            if id == FilePointerEvent::id() {
                self.positions.borrow_mut().push(
                    rkyv::from_bytes::<FilePointerEvent, rkyv::rancor::Error>(bytes).unwrap(),
                );
            }
            Ok(())
        }

        fn listen(&self, _: &str, _: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }

        fn text_offset_at(&self, element: &str, _: f64, _: f64) -> TextOffsetAnswer {
            self.events.borrow_mut().push(format!("measure:{element}"));
            Box::pin(std::future::ready(
                (element == "note-block-0" || self.measured_elements.borrow().contains(element))
                    .then_some(8),
            ))
        }
    }

    #[component]
    fn ClickableNote() -> Element {
        let blocks = use_memo(|| {
            vec![NoteBlock {
                start_line: 3,
                end_line: 4,
                source: "日本 `code` end".to_string(),
                block: MdBlock::Paragraph {
                    inlines: vec![
                        vmux_ecs::event::MdInline::Text("日本 ".to_string()),
                        vmux_ecs::event::MdInline::Code("code".to_string()),
                        vmux_ecs::event::MdInline::Text(" end".to_string()),
                    ],
                },
            }]
        });
        let active = use_memo(|| None);
        let editing = use_memo(|| false);
        let line = use_memo(|| None);
        let cursor = NoteCursor::new(active, editing, line);
        let source_cursor = use_signal(vmux_api::editor::CursorPos::default);
        let selections = use_signal(Vec::new);
        let markers = use_signal(HashMap::new);
        let dragging = use_signal(|| false);
        rsx! { NoteBlockView {
            note_blocks: blocks, diff_markers: markers, index: 0, editing: false,
            source_cursor, source_selections: selections, note_diff_marker: None,
            keymap: vmux_api::editor::KeymapKind::Vim, note_cursor: cursor,
            note_dragging: dragging, comp_open: false, comp_filtered: Vec::new(), comp_sel_clamped: 0,
        } }
    }

    #[test]
    fn one_click_measures_the_rendered_block_before_activating_the_note() {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let host = Rc::new(PointerHost::default());
        let _host = HostScope::enter(host.clone());
        let mut dom = VirtualDom::new(ClickableNote);
        let initial = dom.rebuild_to_vec();
        let target = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "click" => Some(*id),
                _ => None,
            })
            .unwrap();
        dom.render_immediate_to_vec();
        host.events.borrow_mut().clear();
        let data = EventData::Mouse(SerializedMouseData::default());
        dom.runtime()
            .handle_event("click", Event::new(data.into_any(), true), target);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        let _ = std::pin::pin!(dom.wait_for_work()).poll(&mut context);
        dom.render_immediate_to_vec();
        assert_eq!(
            host.events.borrow().as_slice(),
            [
                "measure:note-block-0",
                FileNoteEditRequest::id(),
                FilePointerEvent::id()
            ]
        );
        let positions = host.positions.borrow();
        assert_eq!(positions.len(), 1);
        assert_eq!((positions[0].line, positions[0].col), (3, 12));
    }

    #[component]
    fn EditingList() -> Element {
        let blocks = use_memo(|| {
            vec![NoteBlock {
                start_line: 3,
                end_line: 5,
                source: "- first\n- 日本 `code` end".to_string(),
                block: MdBlock::List {
                    ordered: false,
                    start: 1,
                    items: vec![
                        vmux_ecs::event::MdListItem {
                            source_line: 3,
                            task: None,
                            blocks: vec![MdBlock::Paragraph {
                                inlines: vec![vmux_ecs::event::MdInline::Text("first".to_string())],
                            }],
                        },
                        vmux_ecs::event::MdListItem {
                            source_line: 4,
                            task: None,
                            blocks: vec![MdBlock::Paragraph {
                                inlines: vec![
                                    vmux_ecs::event::MdInline::Text("日本 ".to_string()),
                                    vmux_ecs::event::MdInline::Code("code".to_string()),
                                    vmux_ecs::event::MdInline::Text(" end".to_string()),
                                ],
                            }],
                        },
                    ],
                },
            }]
        });
        let active = use_memo(|| Some(0));
        let editing = use_memo(|| true);
        let line = use_memo(|| Some(3));
        let cursor = NoteCursor::new(active, editing, line);
        let source_cursor = use_signal(|| vmux_api::editor::CursorPos {
            line: 3,
            col: 2,
            ..Default::default()
        });
        let selections = use_signal(Vec::new);
        let markers = use_signal(HashMap::new);
        let dragging = use_signal(|| false);
        rsx! { NoteBlockView {
            note_blocks: blocks, diff_markers: markers, index: 0, editing: true,
            source_cursor, source_selections: selections, note_diff_marker: None,
            keymap: vmux_api::editor::KeymapKind::Vim, note_cursor: cursor,
            note_dragging: dragging, comp_open: false, comp_filtered: Vec::new(), comp_sel_clamped: 0,
        } }
    }

    #[test]
    fn clicking_another_list_item_uses_its_rendered_text_before_the_edit_overlay_exists() {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let host = Rc::new(PointerHost::default());
        let _host = HostScope::enter(host.clone());
        let mut dom = VirtualDom::new(EditingList);
        let initial = dom.rebuild_to_vec();
        let mut identifiers = HashMap::new();
        for edit in &initial.edits {
            if let Mutation::SetAttribute {
                name: "id",
                value: AttributeValue::Text(value),
                id,
                ..
            } = edit
            {
                identifiers.insert(value.to_string(), *id);
                if value.starts_with("note-list-line-") {
                    host.measured_elements
                        .borrow_mut()
                        .insert(value.to_string());
                }
            }
        }
        assert!(!identifiers.contains_key("note-line-4"));
        let item = identifiers["note-list-line-4"];
        let block = identifiers["note-block-0"];
        dom.render_immediate_to_vec();
        host.events.borrow_mut().clear();
        let mut pointer = serde_json::to_value(SerializedMouseData::default()).unwrap();
        let properties = pointer.as_object_mut().unwrap();
        for (name, value) in [
            ("pointer_id", 1),
            ("width", 1),
            ("height", 1),
            ("pressure", 0),
            ("tangential_pressure", 0),
            ("tilt_x", 0),
            ("tilt_y", 0),
            ("twist", 0),
        ] {
            properties.insert(name.to_string(), value.into());
        }
        properties.insert("pointer_type".to_string(), "mouse".into());
        properties.insert("is_primary".to_string(), true.into());
        let pointer: SerializedPointerData = serde_json::from_value(pointer).unwrap();
        dom.runtime().handle_event(
            "pointerdown",
            Event::new(EventData::Pointer(pointer).into_any(), true),
            item,
        );
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            block,
        );
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        let _ = std::pin::pin!(dom.wait_for_work()).poll(&mut context);
        dom.render_immediate_to_vec();
        assert_eq!(
            host.positions
                .borrow()
                .iter()
                .map(|position| (position.line, position.col))
                .collect::<Vec<_>>(),
            [(4, 14)]
        );
        assert!(
            host.events
                .borrow()
                .iter()
                .any(|event| event == "measure:note-list-line-4")
        );
    }

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
