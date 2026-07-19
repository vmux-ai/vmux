use dioxus::prelude::*;
use vmux_core::event::{MdBlock, MdInline, MdListItem, MdTableAlign};

use crate::page_model::{heading_class, span_style, table_align_style};

pub fn render_block(block: &MdBlock, key: usize) -> Element {
    match block {
        MdBlock::Heading { level, inlines } => rsx! {
            div { key: "{key}", class: heading_class(*level), {render_inlines(inlines)} }
        },
        MdBlock::Paragraph { inlines } => rsx! {
            p { key: "{key}", class: "my-3", {render_inlines(inlines)} }
        },
        MdBlock::List {
            ordered,
            start,
            items,
        } => render_list(*ordered, *start, items, key),
        MdBlock::CodeBlock { lines, .. } => rsx! {
            pre {
                key: "{key}",
                class: "my-4 overflow-auto rounded-xl bg-foreground/[0.05] p-4 font-mono text-xs ring-1 ring-inset ring-border",
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
                class: "my-4 rounded-r-lg border-l-2 border-primary/50 bg-primary/[0.04] py-1 pl-4 pr-3 text-foreground/70",
                for (index, block) in blocks.iter().enumerate() { {render_block(block, index)} }
            }
        },
        MdBlock::Table {
            aligns,
            header,
            rows,
        } => render_table(aligns, header, rows, key),
        MdBlock::ThematicBreak => rsx! {
            hr { key: "{key}", class: "my-6 border-border" }
        },
        MdBlock::Html { raw } => rsx! {
            div { key: "{key}", class: "my-3 whitespace-pre-wrap text-foreground/60", "{raw}" }
        },
    }
}

fn render_list(ordered: bool, start: u64, items: &[MdListItem], key: usize) -> Element {
    let inner = rsx! {
        for (index, item) in items.iter().enumerate() {
            li { key: "{index}", class: "my-1",
                if let Some(checked) = item.task {
                    input {
                        r#type: "checkbox",
                        checked,
                        disabled: true,
                        class: "mr-2 align-middle accent-primary",
                    }
                }
                for (block_index, block) in item.blocks.iter().enumerate() {
                    {render_block(block, block_index)}
                }
            }
        }
    };
    if ordered {
        rsx! { ol { key: "{key}", start: "{start}", class: "my-3 list-decimal pl-6", {inner} } }
    } else {
        rsx! { ul { key: "{key}", class: "my-3 list-disc pl-6", {inner} } }
    }
}

fn render_table(
    aligns: &[MdTableAlign],
    header: &[Vec<MdInline>],
    rows: &[Vec<Vec<MdInline>>],
    key: usize,
) -> Element {
    let col_style = |column: usize| {
        aligns
            .get(column)
            .map(|alignment| table_align_style(*alignment))
            .unwrap_or_default()
            .to_string()
    };
    rsx! {
        div { key: "{key}", class: "my-4 overflow-auto rounded-xl ring-1 ring-inset ring-border",
            table { class: "w-full border-collapse text-xs",
                thead {
                    tr { class: "bg-foreground/[0.04]",
                        for (column, cell) in header.iter().enumerate() {
                            th {
                                key: "{column}",
                                class: "border-b border-border px-3 py-2 font-semibold",
                                style: col_style(column),
                                {render_inlines(cell)}
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
                                    {render_inlines(cell)}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn render_inlines(inlines: &[MdInline]) -> Element {
    rsx! {
        for (index, inline) in inlines.iter().enumerate() {
            {render_inline(inline, index)}
        }
    }
}

fn render_inline(inline: &MdInline, key: usize) -> Element {
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
            strong { key: "{key}", class: "font-semibold text-foreground", {render_inlines(inlines)} }
        },
        MdInline::Emph(inlines) => rsx! {
            em { key: "{key}", class: "italic", {render_inlines(inlines)} }
        },
        MdInline::Strike(inlines) => rsx! {
            s { key: "{key}", class: "line-through opacity-70", {render_inlines(inlines)} }
        },
        MdInline::Link { href, inlines } => rsx! {
            a {
                key: "{key}",
                href: "{href}",
                class: "text-primary underline decoration-primary/40 hover:decoration-primary",
                {render_inlines(inlines)}
            }
        },
        MdInline::Image { src, alt } => rsx! {
            img { key: "{key}", src: "{src}", alt: "{alt}", class: "inline max-h-6 align-middle" }
        },
        MdInline::SoftBreak => rsx! { span { key: "{key}", " " } },
        MdInline::HardBreak => rsx! { br { key: "{key}" } },
    }
}
