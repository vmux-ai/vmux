//! Markdown render tree generation for editable Note view.

use std::path::Path;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use vmux_core::event::{MdBlock, MdInline, MdListItem, MdTableAlign, NoteBlock};
use vmux_core::knowledge::markdown_metadata;

use crate::highlight::highlight_snippet;

pub fn is_markdown_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("md" | "markdown" | "mdx")
    )
}

enum Frame {
    Doc(Vec<MdBlock>),
    Para(Vec<MdInline>),
    Heading(u8, Vec<MdInline>),
    Quote(Vec<MdBlock>),
    List(bool, u64, Vec<MdListItem>),
    Item(u32, Option<bool>, Vec<MdInline>, Vec<MdBlock>),
    Strong(Vec<MdInline>),
    Emph(Vec<MdInline>),
    Strike(Vec<MdInline>),
    Link(String, Vec<MdInline>),
    Image(String, String),
    Code(String, String),
    Table(
        Vec<MdTableAlign>,
        Vec<Vec<MdInline>>,
        Vec<Vec<Vec<MdInline>>>,
    ),
    Head(Vec<Vec<MdInline>>),
    Row(Vec<Vec<MdInline>>),
    Cell(Vec<MdInline>),
    Sink(Vec<MdBlock>),
}

pub struct ParsedNote {
    pub title: String,
    pub properties: Vec<vmux_core::knowledge::KnowledgeProperty>,
    pub blocks: Vec<NoteBlock>,
}

fn push_inline(stack: &mut [Frame], inline: MdInline) {
    if let Some(
        Frame::Para(inlines)
        | Frame::Heading(_, inlines)
        | Frame::Strong(inlines)
        | Frame::Emph(inlines)
        | Frame::Strike(inlines)
        | Frame::Link(_, inlines)
        | Frame::Cell(inlines)
        | Frame::Item(_, _, inlines, _),
    ) = stack.last_mut()
    {
        inlines.push(inline);
    }
}

fn push_block(stack: &mut [Frame], block: MdBlock) {
    match stack.last_mut() {
        Some(Frame::Doc(blocks) | Frame::Quote(blocks) | Frame::Sink(blocks)) => blocks.push(block),
        Some(Frame::Item(_, _, pending, blocks)) => {
            if !pending.is_empty() {
                blocks.push(MdBlock::Paragraph {
                    inlines: std::mem::take(pending),
                });
            }
            blocks.push(block);
        }
        _ => {}
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn table_alignment(alignment: Alignment) -> MdTableAlign {
    match alignment {
        Alignment::None => MdTableAlign::None,
        Alignment::Left => MdTableAlign::Left,
        Alignment::Center => MdTableAlign::Center,
        Alignment::Right => MdTableAlign::Right,
    }
}

fn line_start_offsets(text: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            offsets.push(index + 1);
        }
    }
    offsets
}

fn inline_text(inlines: &[MdInline], output: &mut String) {
    for inline in inlines {
        match inline {
            MdInline::Text(text) | MdInline::Code(text) => output.push_str(text),
            MdInline::Strong(children)
            | MdInline::Emph(children)
            | MdInline::Strike(children)
            | MdInline::Link {
                inlines: children, ..
            } => inline_text(children, output),
            MdInline::Image { alt, .. } => output.push_str(alt),
            MdInline::WikiLink { label, .. } => output.push_str(label),
            MdInline::SoftBreak | MdInline::HardBreak => output.push(' '),
        }
    }
}

fn push_text_with_wiki_links(stack: &mut [Frame], text: &str) {
    let links = vmux_core::knowledge::wiki_links(text);
    if links.is_empty() {
        push_inline(stack, MdInline::Text(text.to_string()));
        return;
    }
    let mut offset = 0;
    for link in links {
        if offset < link.start {
            push_inline(stack, MdInline::Text(text[offset..link.start].to_string()));
        }
        let target = match link.anchor.as_deref() {
            Some(anchor) if link.note.is_empty() => format!("#{anchor}"),
            Some(anchor) => format!("{}#{anchor}", link.note),
            None => link.note.clone(),
        };
        let label = link.label.clone().unwrap_or_else(|| target.clone());
        push_inline(
            stack,
            MdInline::WikiLink {
                target,
                label,
                path: String::new(),
                line: None,
                exists: false,
                embed: link.embed,
            },
        );
        offset = link.end;
    }
    if offset < text.len() {
        push_inline(stack, MdInline::Text(text[offset..].to_string()));
    }
}

fn heading_matches_title(block: &MdBlock, title: &str) -> bool {
    let MdBlock::Heading { level: 1, inlines } = block else {
        return false;
    };
    let mut text = String::new();
    inline_text(inlines, &mut text);
    text.trim() == title.trim()
}

fn offset_to_line(line_starts: &[usize], byte: usize) -> u32 {
    match line_starts.binary_search(&byte) {
        Ok(index) => index as u32,
        Err(index) => index.saturating_sub(1) as u32,
    }
}

pub fn parse_note(text: &str) -> Vec<NoteBlock> {
    parse_note_document(text).blocks
}

fn note_block_content_end(block: &NoteBlock) -> u32 {
    if matches!(block.block, MdBlock::CodeBlock { .. }) {
        return block.end_line;
    }
    let line_count = block
        .source
        .trim_end_matches(['\r', '\n'])
        .lines()
        .count()
        .max(1) as u32;
    block
        .start_line
        .saturating_add(line_count)
        .min(block.end_line)
}

pub fn note_vertical_target(blocks: &[NoteBlock], line: u32, direction: i8) -> Option<u32> {
    let adjacent = if direction > 0 {
        line.saturating_add(1)
    } else {
        match line.checked_sub(1) {
            Some(line) => line,
            None => return Some(line),
        }
    };
    if blocks
        .iter()
        .any(|block| block.start_line <= adjacent && adjacent < note_block_content_end(block))
    {
        return None;
    }
    if direction > 0 {
        blocks
            .iter()
            .find(|block| block.start_line > line)
            .map(|block| block.start_line)
            .or(Some(line))
    } else {
        blocks
            .iter()
            .rev()
            .find(|block| note_block_content_end(block) <= line)
            .map(|block| note_block_content_end(block).saturating_sub(1))
            .or(Some(line))
    }
}

pub fn parse_note_document(text: &str) -> ParsedNote {
    let metadata = markdown_metadata(text);
    let body = &text[metadata.body_offset..];
    let line_starts = line_start_offsets(text);
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_FOOTNOTES);

    let mut stack = vec![Frame::Doc(Vec::new())];
    let mut ranges = Vec::new();
    let mut top_start = None;

    for (event, range) in Parser::new_ext(body, options).into_offset_iter() {
        let (range_start, range_end) = (
            metadata.body_offset + range.start,
            metadata.body_offset + range.end,
        );
        if stack.len() == 1 && matches!(&event, Event::Start(_)) {
            top_start = Some(range_start);
        }
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => stack.push(Frame::Para(Vec::new())),
                Tag::Heading { level, .. } => {
                    stack.push(Frame::Heading(heading_level(level), Vec::new()))
                }
                Tag::BlockQuote(_) => stack.push(Frame::Quote(Vec::new())),
                Tag::List(start) => {
                    stack.push(Frame::List(start.is_some(), start.unwrap_or(1), Vec::new()))
                }
                Tag::Item => stack.push(Frame::Item(
                    offset_to_line(&line_starts, range_start),
                    None,
                    Vec::new(),
                    Vec::new(),
                )),
                Tag::CodeBlock(kind) => {
                    let language = match kind {
                        CodeBlockKind::Fenced(language) => language.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    stack.push(Frame::Code(language, String::new()));
                }
                Tag::Emphasis => stack.push(Frame::Emph(Vec::new())),
                Tag::Strong => stack.push(Frame::Strong(Vec::new())),
                Tag::Strikethrough => stack.push(Frame::Strike(Vec::new())),
                Tag::Link { dest_url, .. } => {
                    stack.push(Frame::Link(dest_url.to_string(), Vec::new()))
                }
                Tag::Image { dest_url, .. } => {
                    stack.push(Frame::Image(dest_url.to_string(), String::new()))
                }
                Tag::Table(alignments) => stack.push(Frame::Table(
                    alignments.into_iter().map(table_alignment).collect(),
                    Vec::new(),
                    Vec::new(),
                )),
                Tag::TableHead => stack.push(Frame::Head(Vec::new())),
                Tag::TableRow => stack.push(Frame::Row(Vec::new())),
                Tag::TableCell => stack.push(Frame::Cell(Vec::new())),
                _ => stack.push(Frame::Sink(Vec::new())),
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if let Some(Frame::Para(inlines)) = stack.pop() {
                        push_block(&mut stack, MdBlock::Paragraph { inlines });
                    }
                }
                TagEnd::Heading(_) => {
                    if let Some(Frame::Heading(level, inlines)) = stack.pop() {
                        push_block(&mut stack, MdBlock::Heading { level, inlines });
                    }
                }
                TagEnd::BlockQuote(_) => {
                    if let Some(Frame::Quote(blocks)) = stack.pop() {
                        push_block(&mut stack, MdBlock::BlockQuote { blocks });
                    }
                }
                TagEnd::List(_) => {
                    if let Some(Frame::List(ordered, start, items)) = stack.pop() {
                        push_block(
                            &mut stack,
                            MdBlock::List {
                                ordered,
                                start,
                                items,
                            },
                        );
                    }
                }
                TagEnd::Item => {
                    if let Some(Frame::Item(source_line, task, pending, mut blocks)) = stack.pop() {
                        if !pending.is_empty() {
                            blocks.push(MdBlock::Paragraph { inlines: pending });
                        }
                        if let Some(Frame::List(_, _, items)) = stack.last_mut() {
                            items.push(MdListItem {
                                source_line,
                                task,
                                blocks,
                            });
                        }
                    }
                }
                TagEnd::CodeBlock => {
                    if let Some(Frame::Code(language, code)) = stack.pop() {
                        let token = language
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_string();
                        push_block(
                            &mut stack,
                            MdBlock::CodeBlock {
                                lang: language,
                                lines: highlight_snippet(&code, &token),
                            },
                        );
                    }
                }
                TagEnd::Emphasis => {
                    if let Some(Frame::Emph(inlines)) = stack.pop() {
                        push_inline(&mut stack, MdInline::Emph(inlines));
                    }
                }
                TagEnd::Strong => {
                    if let Some(Frame::Strong(inlines)) = stack.pop() {
                        push_inline(&mut stack, MdInline::Strong(inlines));
                    }
                }
                TagEnd::Strikethrough => {
                    if let Some(Frame::Strike(inlines)) = stack.pop() {
                        push_inline(&mut stack, MdInline::Strike(inlines));
                    }
                }
                TagEnd::Link => {
                    if let Some(Frame::Link(href, inlines)) = stack.pop() {
                        push_inline(&mut stack, MdInline::Link { href, inlines });
                    }
                }
                TagEnd::Image => {
                    if let Some(Frame::Image(src, alt)) = stack.pop() {
                        push_inline(&mut stack, MdInline::Image { src, alt });
                    }
                }
                TagEnd::Table => {
                    if let Some(Frame::Table(aligns, header, rows)) = stack.pop() {
                        push_block(
                            &mut stack,
                            MdBlock::Table {
                                aligns,
                                header,
                                rows,
                            },
                        );
                    }
                }
                TagEnd::TableHead => {
                    if let Some(Frame::Head(cells)) = stack.pop()
                        && let Some(Frame::Table(_, header, _)) = stack.last_mut()
                    {
                        *header = cells;
                    }
                }
                TagEnd::TableRow => {
                    if let Some(Frame::Row(cells)) = stack.pop()
                        && let Some(Frame::Table(_, _, rows)) = stack.last_mut()
                    {
                        rows.push(cells);
                    }
                }
                TagEnd::TableCell => {
                    if let Some(Frame::Cell(inlines)) = stack.pop()
                        && let Some(Frame::Head(cells) | Frame::Row(cells)) = stack.last_mut()
                    {
                        cells.push(inlines);
                    }
                }
                _ => {
                    stack.pop();
                }
            },
            Event::Text(text) => match stack.last_mut() {
                Some(Frame::Code(_, code)) => code.push_str(&text),
                Some(Frame::Image(_, alt)) => alt.push_str(&text),
                _ => push_text_with_wiki_links(&mut stack, &text),
            },
            Event::Code(code) => push_inline(&mut stack, MdInline::Code(code.to_string())),
            Event::SoftBreak => push_inline(&mut stack, MdInline::SoftBreak),
            Event::HardBreak => push_inline(&mut stack, MdInline::HardBreak),
            Event::Rule => push_block(&mut stack, MdBlock::ThematicBreak),
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item(_, task, _, _)) = stack.last_mut() {
                    *task = Some(checked);
                }
            }
            Event::Html(raw) => push_block(
                &mut stack,
                MdBlock::Html {
                    raw: raw.to_string(),
                },
            ),
            Event::InlineHtml(raw) => push_inline(&mut stack, MdInline::Text(raw.to_string())),
            Event::FootnoteReference(label) => {
                push_inline(&mut stack, MdInline::Text(format!("[^{label}]")))
            }
            _ => {}
        }

        if stack.len() == 1 {
            let block_count = match stack.first() {
                Some(Frame::Doc(blocks)) => blocks.len(),
                _ => 0,
            };
            while ranges.len() < block_count {
                ranges.push((top_start.take().unwrap_or(range_start), range_end));
            }
        }
    }

    let blocks = match stack.into_iter().next() {
        Some(Frame::Doc(blocks)) => blocks,
        _ => Vec::new(),
    };
    let mut parsed = blocks
        .into_iter()
        .zip(ranges)
        .map(|(block, (start, end))| NoteBlock {
            start_line: offset_to_line(&line_starts, start),
            end_line: offset_to_line(&line_starts, end.saturating_sub(1)) + 1,
            source: text[start..end].to_string(),
            block,
        })
        .collect::<Vec<_>>();
    if !metadata.title.is_empty()
        && !parsed
            .first()
            .is_some_and(|block| heading_matches_title(&block.block, &metadata.title))
        && let Some(title_line) = metadata.title_line
    {
        let source = text
            .lines()
            .nth(title_line as usize)
            .unwrap_or_default()
            .to_string();
        parsed.insert(
            0,
            NoteBlock {
                start_line: title_line,
                end_line: title_line + 1,
                source,
                block: MdBlock::Heading {
                    level: 1,
                    inlines: vec![MdInline::Text(metadata.title.clone())],
                },
            },
        );
    }
    if parsed.is_empty() {
        let body_line = offset_to_line(&line_starts, metadata.body_offset);
        parsed.push(NoteBlock {
            start_line: body_line,
            end_line: body_line + 1,
            source: body.to_string(),
            block: MdBlock::Paragraph {
                inlines: Vec::new(),
            },
        });
    }
    ParsedNote {
        title: metadata.title,
        properties: metadata.properties,
        blocks: parsed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_paths() {
        assert!(is_markdown_path(Path::new("a.md")));
        assert!(is_markdown_path(Path::new("A.MARKDOWN")));
        assert!(is_markdown_path(Path::new("a.mdx")));
        assert!(!is_markdown_path(Path::new("a.rs")));
    }

    #[test]
    fn note_blocks_have_source_ranges() {
        let blocks = parse_note("# Title\n\nParagraph\n\n- one\n- two\n");
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].start_line, 0);
        assert_eq!(blocks[1].start_line, 2);
        assert_eq!(blocks[2].start_line, 4);
    }

    #[test]
    fn note_vertical_navigation_skips_block_separators() {
        let blocks = parse_note("first\nline\n\nsecond\nline\n");
        assert_eq!(note_vertical_target(&blocks, 1, 1), Some(3));
        assert_eq!(note_vertical_target(&blocks, 3, -1), Some(1));
        assert_eq!(note_vertical_target(&blocks, 0, 1), None);
        assert_eq!(note_vertical_target(&blocks, 4, -1), None);
        assert_eq!(note_vertical_target(&blocks, 0, -1), Some(0));
        assert_eq!(note_vertical_target(&blocks, 4, 1), Some(4));
    }

    #[test]
    fn note_vertical_navigation_keeps_blank_code_lines() {
        let blocks = parse_note("```text\none\n\ntwo\n```\n\nafter\n");
        assert_eq!(note_vertical_target(&blocks, 1, 1), None);
        assert_eq!(note_vertical_target(&blocks, 2, 1), None);
        assert_eq!(note_vertical_target(&blocks, 4, 1), Some(6));
    }

    #[test]
    fn note_vertical_navigation_leaves_a_list_for_the_following_paragraph() {
        let blocks = parse_note("- `vmux/` — references.\n\nThis structure is a starting point.\n");
        assert_eq!(blocks[0].start_line, 0);
        assert_eq!(blocks[0].end_line, 2);
        assert_eq!(blocks[1].start_line, 2);
        assert_eq!(note_vertical_target(&blocks, 0, 1), Some(2));
        assert_eq!(note_vertical_target(&blocks, 2, -1), Some(0));
    }

    #[test]
    fn nested_list_items_keep_exact_source_lines() {
        let blocks = parse_note("1. one\n   - nested\n2. two\n");
        let MdBlock::List { items, .. } = &blocks[0].block else {
            panic!("expected list");
        };
        assert_eq!(items[0].source_line, 0);
        assert_eq!(items[1].source_line, 2);
        let MdBlock::List { items, .. } = &items[0].blocks[1] else {
            panic!("expected nested list");
        };
        assert_eq!(items[0].source_line, 1);
    }

    #[test]
    fn frontmatter_title_renders_as_heading_without_metadata_delimiters() {
        let note = parse_note_document("---\ntitle: Welcome Home\n---\n\nBody\n");
        assert_eq!(note.title, "Welcome Home");
        assert_eq!(note.blocks.len(), 2);
        assert_eq!(note.blocks[0].start_line, 1);
        assert_eq!(note.blocks[0].source, "title: Welcome Home");
        assert!(matches!(
            &note.blocks[0].block,
            MdBlock::Heading { level: 1, .. }
        ));
        assert_eq!(note.blocks[1].start_line, 4);
    }

    #[test]
    fn frontmatter_title_does_not_duplicate_matching_heading() {
        let note = parse_note_document("---\ntitle: Welcome Home\n---\n\n# Welcome Home\n\nBody\n");

        assert_eq!(note.title, "Welcome Home");
        assert_eq!(note.blocks.len(), 2);
        assert_eq!(note.blocks[0].start_line, 4);
        assert!(matches!(
            &note.blocks[0].block,
            MdBlock::Heading { level: 1, .. }
        ));
    }
}
