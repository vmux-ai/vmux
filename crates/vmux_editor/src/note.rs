use dioxus::prelude::*;
use vmux_core::event::{MdBlock, MdInline, MdListItem, MdTableAlign};
use vmux_ui::hooks::try_cef_bin_emit_rkyv;
use vmux_ui::i18n::translate;

use crate::page_model::{heading_class, span_style, table_align_style};

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
) -> Element {
    let key = list_key;
    let items = items.as_slice();
    let inner = rsx! {
        for (index, item) in items.iter().enumerate() {
            {
                let item_hidden = hidden || hidden_list_line == Some(item.source_line);
                rsx! {
            li {
                key: "{index}",
                "data-note-list-line": "{item.source_line}",
                class: "my-1",
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
                        let _ = try_cef_bin_emit_rkyv(&vmux_core::event::KnowledgeLinkOpen {
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
