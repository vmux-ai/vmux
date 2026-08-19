use dioxus::html::geometry::ClientPoint;
use dioxus::prelude::*;
use vmux_core::editor::SelSpan;
use vmux_core::event::{MdBlock, MdInline, MdListItem, MdTableAlign};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

use crate::page_model::{heading_class, span_style, table_align_style};

/// The caret drawn into a source line. Named so the page can scroll to it.
pub const NOTE_CARET_ID: &str = "note-caret";

/// Which list item a pointer last went down on.
///
/// A list's items are rendered here, recursively, while the gesture that opens a block for editing
/// is handled on the block. Rather than have the block hunt back down the tree for which item was
/// hit — which is what a `closest()` walk was doing — the item that knows says so on the way past.
#[derive(Clone, Copy)]
pub struct ListLineHit(pub Signal<Option<u32>>);

/// The source of the list item the caret is in, and how to draw it.
///
/// Drawn over the rendered item rather than instead of it, so the list keeps the height it had and
/// does not jump as the markers appear. Positioned by `inset-0` within the item, because the item
/// is the only thing that knows where it is and nothing here can measure that.
#[derive(Clone, PartialEq)]
pub struct ListEditLine {
    pub line: u32,
    pub chunks: Vec<NoteLineChunk>,
    pub caret_width_class: String,
}

/// One run of a source line styled the same all the way through.
///
/// A line is cut at the caret and at the selection's edges, so every piece is either wholly
/// selected or wholly not, and the caret falls between two of them rather than inside one.
#[derive(Clone, PartialEq)]
pub struct NoteLineChunk {
    pub text: String,
    pub selected: bool,
    pub caret_before: bool,
}

impl NoteLineChunk {
    pub fn split(text: &str, caret: Option<u32>, selection: Option<SelSpan>) -> Vec<Self> {
        let chars = text.chars().collect::<Vec<_>>();
        let len = chars.len() as u32;
        let caret = caret.map(|column| column.min(len));
        let selection = selection.map(|span| {
            let start = span.start.min(len);
            let end = if span.end == u32::MAX {
                len
            } else {
                span.end.min(len)
            };
            (start.min(end), start.max(end))
        });
        let mut boundaries = vec![0, len];
        if let Some(caret) = caret {
            boundaries.push(caret);
        }
        if let Some((start, end)) = selection {
            boundaries.push(start);
            boundaries.push(end);
        }
        boundaries.sort_unstable();
        boundaries.dedup();

        let mut chunks = Vec::new();
        for range in boundaries.windows(2) {
            let (start, end) = (range[0], range[1]);
            chunks.push(Self {
                text: chars[start as usize..end as usize].iter().collect(),
                selected: selection.is_some_and(|(from, to)| start < to && end > from),
                caret_before: caret == Some(start),
            });
        }
        if chunks.is_empty() || caret == Some(len) {
            chunks.push(Self {
                text: String::new(),
                selected: false,
                caret_before: caret == Some(len),
            });
        }

        chunks
    }
}

/// The raw source of one line, with the caret and the selection drawn into it.
#[component]
pub fn NoteSourceLine(chunks: Vec<NoteLineChunk>, caret_width_class: String) -> Element {
    rsx! {
        span {
            "data-note-line-text": "true",
            class: "inline-block min-w-[1ch]",
            for (index, chunk) in chunks.iter().enumerate() {
                if chunk.caret_before {
                    span {
                        key: "caret-{index}",
                        id: NOTE_CARET_ID,
                        class: "relative inline-block h-[1.15em] w-0 scroll-mb-8 scroll-mt-8 align-text-bottom",
                        span { class: "pointer-events-none absolute inset-y-0 left-0 {caret_width_class} bg-current" }
                    }
                }
                if !chunk.text.is_empty() {
                    span {
                        key: "text-{index}",
                        class: if chunk.selected { "bg-cyan-400/20" } else { "" },
                        "{chunk.text}"
                    }
                }
            }
        }
    }
}

fn hidden_class(class: &'static str, hidden: bool) -> String {
    if hidden {
        format!("{class} invisible")
    } else {
        class.to_string()
    }
}

/// One markdown block, recursing into the blocks and inlines it contains.
///
/// `hidden_list_line` blanks the source line the caret is editing, so the raw markdown shows
/// through in its place without the rendered copy jumping.
#[component]
pub fn MdBlockView(
    block: MdBlock,
    block_key: usize,
    #[props(default)] hidden: bool,
    #[props(default)] hidden_list_line: Option<u32>,
    #[props(default)] list_edit: Option<ListEditLine>,
    #[props(default)] on_line_down: Option<EventHandler<(u32, ClientPoint, bool)>>,
) -> Element {
    let key = block_key;
    let block = &block;
    match block {
        MdBlock::Heading { level, inlines } => rsx! {
            div { key: "{key}", class: hidden_class(heading_class(*level), hidden), MdInlines { inlines: inlines.clone() } }
        },
        MdBlock::Paragraph { inlines } => rsx! {
            p { key: "{key}", class: hidden_class("my-3", hidden), MdInlines { inlines: inlines.clone() } }
        },
        MdBlock::List {
            ordered,
            start,
            items,
        } => rsx! {
            MdList {
                ordered: *ordered,
                start: *start,
                items: items.clone(),
                list_key: key,
                hidden,
                hidden_list_line,
                list_edit: list_edit.clone(),
                on_line_down,
            }
        },
        MdBlock::CodeBlock { lines, .. } => rsx! {
            pre {
                key: "{key}",
                class: hidden_class("my-4 overflow-auto rounded-xl bg-foreground/[0.05] p-4 font-mono text-xs ring-1 ring-inset ring-border", hidden),
                for (line_index, line) in lines.iter().enumerate() {
                    div { key: "{line_index}",
                        for (span_index, span) in line.spans.iter().enumerate() {
                            span { key: "{span_index}", style: span_style(span), "{span.text}" }
                        }
                    }
                }
            }
        },
        MdBlock::BlockQuote { blocks } => rsx! {
            blockquote {
                key: "{key}",
                class: hidden_class("my-4 rounded-r-lg border-l-2 border-primary/50 bg-primary/[0.04] py-1 pl-4 pr-3 text-foreground/70", hidden),
                for (index, block) in blocks.iter().enumerate() {
                    MdBlockView { block: block.clone(), block_key: index, hidden: hidden, hidden_list_line: hidden_list_line }
                }
            }
        },
        MdBlock::Table {
            aligns,
            header,
            rows,
        } => rsx! {
            MdTable {
                aligns: aligns.clone(),
                header: header.clone(),
                rows: rows.clone(),
                table_key: key,
                hidden,
            }
        },
        MdBlock::ThematicBreak => rsx! {
            hr { key: "{key}", class: hidden_class("my-6 border-border", hidden) }
        },
        MdBlock::Html { raw } => rsx! {
            div { key: "{key}", class: hidden_class("my-3 whitespace-pre-wrap text-foreground/60", hidden), "{raw}" }
        },
    }
}

/// An ordered or bulleted list, whose items hold blocks of their own.
#[component]
fn MdList(
    ordered: bool,
    start: u64,
    items: Vec<MdListItem>,
    list_key: usize,
    hidden: bool,
    hidden_list_line: Option<u32>,
    #[props(default)] list_edit: Option<ListEditLine>,
    #[props(default)] on_line_down: Option<EventHandler<(u32, ClientPoint, bool)>>,
) -> Element {
    let key = list_key;
    let items = items.as_slice();
    let hit = try_consume_context::<ListLineHit>();
    let inner = rsx! {
        for (index, item) in items.iter().enumerate() {
            {
                let item_hidden = hidden || hidden_list_line == Some(item.source_line);
                let editing_here = list_edit
                    .as_ref()
                    .filter(|edit| edit.line == item.source_line);
                rsx! {
            li {
                key: "{index}",
                "data-note-list-line": "{item.source_line}",
                class: "relative my-1",
                onpointerdown: {
                    let line = item.source_line;
                    move |_| {
                        if let Some(ListLineHit(mut hit)) = hit {
                            hit.set(Some(line));
                        }
                    }
                },
                if let Some(checked) = item.task {
                    input {
                        r#type: "checkbox",
                        checked,
                        disabled: true,
                        class: "mr-2 align-middle accent-primary",
                    }
                }
                for (block_index, block) in item.blocks.iter().enumerate() {
                    MdBlockView { block: block.clone(), block_key: block_index, hidden: item_hidden, hidden_list_line: hidden_list_line }
                }
                if let Some(edit) = editing_here {
                    ListSourceOverlay { edit: edit.clone(), on_line_down }
                }
            }
                }
            }
        }
    };
    let class = hidden_class(
        if ordered {
            "my-3 list-decimal pl-6"
        } else {
            "my-3 list-disc pl-6"
        },
        hidden,
    );
    if ordered {
        rsx! { ol { key: "{key}", start: "{start}", class, {inner} } }
    } else {
        rsx! { ul { key: "{key}", class, {inner} } }
    }
}

/// The source of one list item, over the rendered copy it is replacing.
#[component]
fn ListSourceOverlay(
    edit: ListEditLine,
    on_line_down: Option<EventHandler<(u32, ClientPoint, bool)>>,
) -> Element {
    let line = edit.line;
    rsx! {
        div {
            id: "note-line-{line}",
            "data-note-edit-line": "{line}",
            class: "absolute inset-0 min-h-[1lh] w-full cursor-text whitespace-pre-wrap break-words",
            onclick: move |event: Event<MouseData>| {
                event.stop_propagation();
                event.prevent_default();
            },
            onmousedown: move |event: Event<MouseData>| {
                event.stop_propagation();
                event.prevent_default();
            },
            onpointerdown: move |event: Event<PointerData>| {
                event.stop_propagation();
                event.prevent_default();
                let Some(handler) = on_line_down else {
                    return;
                };
                handler.call((
                    line,
                    event.client_coordinates(),
                    event.modifiers().shift(),
                ));
            },
            NoteSourceLine {
                chunks: edit.chunks.clone(),
                caret_width_class: edit.caret_width_class.clone(),
            }
        }
    }
}

/// A markdown table, with its per-column alignment.
#[component]
fn MdTable(
    aligns: Vec<MdTableAlign>,
    header: Vec<Vec<MdInline>>,
    rows: Vec<Vec<Vec<MdInline>>>,
    table_key: usize,
    hidden: bool,
) -> Element {
    let key = table_key;
    let aligns = aligns.as_slice();
    let header = header.as_slice();
    let rows = rows.as_slice();
    let col_style = |column: usize| {
        aligns
            .get(column)
            .map(|alignment| table_align_style(*alignment))
            .unwrap_or_default()
            .to_string()
    };
    rsx! {
        div { key: "{key}", class: hidden_class("my-4 overflow-auto rounded-xl ring-1 ring-inset ring-border", hidden),
            table { class: "w-full border-collapse text-xs",
                thead {
                    tr { class: "bg-foreground/[0.04]",
                        for (column, cell) in header.iter().enumerate() {
                            th {
                                key: "{column}",
                                class: "border-b border-border px-3 py-2 font-semibold",
                                style: col_style(column),
                                MdInlines { inlines: cell.clone() }
                            }
                        }
                    }
                }
                tbody {
                    for (row_index, row) in rows.iter().enumerate() {
                        tr { key: "{row_index}", class: "odd:bg-foreground/[0.02]",
                            for (column, cell) in row.iter().enumerate() {
                                td {
                                    key: "{column}",
                                    class: "border-b border-border px-3 py-2",
                                    style: col_style(column),
                                    MdInlines { inlines: cell.clone() }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A run of inline markdown nodes.
#[component]
fn MdInlines(inlines: Vec<MdInline>) -> Element {
    rsx! {
        for (index , inline) in inlines.iter().enumerate() {
            MdInlineView { inline: inline.clone(), inline_key: index }
        }
    }
}

/// One inline markdown node, recursing into the nodes it wraps.
#[component]
fn MdInlineView(inline: MdInline, inline_key: usize) -> Element {
    let key = inline_key;
    let inline = &inline;
    match inline {
        MdInline::Text(text) => rsx! { span { key: "{key}", "{text}" } },
        MdInline::Code(text) => rsx! {
            code {
                key: "{key}",
                class: "rounded bg-foreground/10 px-1 py-0.5 font-mono text-[0.85em] text-primary",
                "{text}"
            }
        },
        MdInline::Strong(inlines) => rsx! {
            strong { key: "{key}", class: "font-semibold text-foreground", MdInlines { inlines: inlines.clone() } }
        },
        MdInline::Emph(inlines) => rsx! {
            em { key: "{key}", class: "italic", MdInlines { inlines: inlines.clone() } }
        },
        MdInline::Strike(inlines) => rsx! {
            s { key: "{key}", class: "line-through opacity-70", MdInlines { inlines: inlines.clone() } }
        },
        MdInline::Link { href, inlines } => rsx! {
            a {
                key: "{key}",
                href: "{href}",
                class: "text-primary underline decoration-primary/40 hover:decoration-primary",
                MdInlines { inlines: inlines.clone() }
            }
        },
        MdInline::Image { src, alt } => rsx! {
            img { key: "{key}", src: "{src}", alt: "{alt}", class: "inline max-h-6 align-middle" }
        },
        MdInline::SoftBreak => rsx! { span { key: "{key}", " " } },
        MdInline::HardBreak => rsx! { br { key: "{key}" } },
        MdInline::WikiLink {
            target,
            label,
            path,
            line,
            exists,
            embed,
        } => {
            let open_path = path.clone();
            let open_title = target.split('#').next().unwrap_or(target).to_string();
            let open_line = *line;
            let create = !*exists;
            let disabled = open_path.is_empty();
            rsx! {
                button {
                    key: "{key}",
                    r#type: "button",
                    disabled,
                    title: if *exists { translate("knowledge-open-linked-note") } else { translate("knowledge-create-linked-note") },
                    class: if *exists {
                        "inline cursor-pointer rounded px-0.5 text-primary underline decoration-primary/35 underline-offset-2 hover:bg-primary/10 hover:decoration-primary"
                    } else {
                        "inline cursor-pointer rounded px-0.5 text-destructive underline decoration-dashed underline-offset-2 hover:bg-destructive/10 disabled:cursor-default disabled:opacity-50"
                    },
                    onmousedown: move |event: Event<MouseData>| {
                        event.stop_propagation();
                    },
                    onclick: move |event: Event<MouseData>| {
                        event.stop_propagation();
                        let _ = send(&vmux_core::event::KnowledgeLinkOpen {
                            path: open_path.clone(),
                            title: open_title.clone(),
                            line: open_line,
                            create,
                        });
                    },
                    if *embed { "↳ " }
                    "{label}"
                }
            }
        }
    }
}
