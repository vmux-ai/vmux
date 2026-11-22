use std::path::Path;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use vmux_ecs::event::{MdBlock, MdInline, MdListItem, MdTableAlign, NoteBlock};
use vmux_knowledge::{KnowledgeProperty, MarkdownMetadata, WikiLink};

use crate::host::highlight::Highlighter;

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

#[derive(Clone)]
pub(crate) struct ParsedNote {
    pub(crate) title: String,
    pub(crate) properties: Vec<KnowledgeProperty>,
    pub(crate) blocks: Vec<NoteBlock>,
}

impl ParsedNote {
    pub(crate) fn supports(path: &Path) -> bool {
        matches!(
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.to_ascii_lowercase())
                .as_deref(),
            Some("md" | "markdown" | "mdx")
        )
    }

    pub(crate) fn vertical_target(blocks: &[NoteBlock], line: u32, direction: i8) -> Option<u32> {
        let adjacent = if direction > 0 {
            line.saturating_add(1)
        } else {
            match line.checked_sub(1) {
                Some(line) => line,
                None => return Some(line),
            }
        };
        if blocks.iter().any(|block| {
            block.start_line <= adjacent && adjacent < Self::note_block_content_end(block)
        }) {
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
                .find(|block| Self::note_block_content_end(block) <= line)
                .map(|block| Self::note_block_content_end(block).saturating_sub(1))
                .or(Some(line))
        }
    }

    #[cfg(test)]
    pub(crate) fn parse(text: &str) -> Self {
        Self::parse_with_scheme(text, true)
    }

    pub(crate) fn parse_with_scheme(text: &str, dark: bool) -> Self {
        let metadata = MarkdownMetadata::parse(text);
        let body = &text[metadata.body_offset..];
        let line_starts = Self::line_start_offsets(text);
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
            Self::handle_event(&mut stack, event, &line_starts, range_start, dark);
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
        Self::finalize_blocks(text, body, metadata, &line_starts, blocks, ranges)
    }

    fn handle_event(
        stack: &mut Vec<Frame>,
        event: Event<'_>,
        line_starts: &[usize],
        range_start: usize,
        dark: bool,
    ) {
        match event {
            Event::Start(tag) => Self::start_tag(stack, tag, line_starts, range_start),
            Event::End(tag) => Self::end_tag(stack, tag, dark),
            Event::Text(text) => match stack.last_mut() {
                Some(Frame::Code(_, code)) => code.push_str(&text),
                Some(Frame::Image(_, alt)) => alt.push_str(&text),
                _ => Self::push_text_with_wiki_links(stack, &text),
            },
            Event::Code(code) => Self::push_inline(stack, MdInline::Code(code.to_string())),
            Event::SoftBreak => Self::push_inline(stack, MdInline::SoftBreak),
            Event::HardBreak => Self::push_inline(stack, MdInline::HardBreak),
            Event::Rule => Self::push_block(stack, MdBlock::ThematicBreak),
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item(_, task, _, _)) = stack.last_mut() {
                    *task = Some(checked);
                }
            }
            Event::Html(raw) => Self::push_block(
                stack,
                MdBlock::Html {
                    raw: raw.to_string(),
                },
            ),
            Event::InlineHtml(raw) => Self::push_inline(stack, MdInline::Text(raw.to_string())),
            Event::FootnoteReference(label) => {
                Self::push_inline(stack, MdInline::Text(format!("[^{label}]")))
            }
            _ => {}
        }
    }

    fn start_tag(stack: &mut Vec<Frame>, tag: Tag<'_>, line_starts: &[usize], range_start: usize) {
        match tag {
            Tag::Paragraph => stack.push(Frame::Para(Vec::new())),
            Tag::Heading { level, .. } => {
                stack.push(Frame::Heading(Self::heading_level(level), Vec::new()))
            }
            Tag::BlockQuote(_) => stack.push(Frame::Quote(Vec::new())),
            Tag::List(start) => {
                stack.push(Frame::List(start.is_some(), start.unwrap_or(1), Vec::new()))
            }
            Tag::Item => stack.push(Frame::Item(
                Self::offset_to_line(line_starts, range_start),
                None,
                Vec::new(),
                Vec::new(),
            )),
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => info.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                stack.push(Frame::Code(language, String::new()));
            }
            Tag::Emphasis => stack.push(Frame::Emph(Vec::new())),
            Tag::Strong => stack.push(Frame::Strong(Vec::new())),
            Tag::Strikethrough => stack.push(Frame::Strike(Vec::new())),
            Tag::Link { dest_url, .. } => stack.push(Frame::Link(dest_url.to_string(), Vec::new())),
            Tag::Image { dest_url, .. } => {
                stack.push(Frame::Image(dest_url.to_string(), String::new()))
            }
            Tag::Table(aligns) => {
                let mut mapped = Vec::with_capacity(aligns.len());
                for alignment in aligns {
                    mapped.push(Self::table_alignment(alignment));
                }
                stack.push(Frame::Table(mapped, Vec::new(), Vec::new()));
            }
            Tag::TableHead => stack.push(Frame::Head(Vec::new())),
            Tag::TableRow => stack.push(Frame::Row(Vec::new())),
            Tag::TableCell => stack.push(Frame::Cell(Vec::new())),
            _ => stack.push(Frame::Sink(Vec::new())),
        }
    }

    fn end_tag(stack: &mut Vec<Frame>, tag: TagEnd, dark: bool) {
        match tag {
            TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::CodeBlock => Self::end_block_tag(stack, tag, dark),
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Link
            | TagEnd::Image => Self::end_inline_tag(stack, tag),
            TagEnd::Table | TagEnd::TableHead | TagEnd::TableRow | TagEnd::TableCell => {
                Self::end_table_tag(stack, tag);
            }
            _ => {
                stack.pop();
            }
        }
    }

    fn end_block_tag(stack: &mut Vec<Frame>, tag: TagEnd, dark: bool) {
        match tag {
            TagEnd::Paragraph => {
                if let Some(Frame::Para(inlines)) = stack.pop() {
                    Self::push_block(stack, MdBlock::Paragraph { inlines });
                }
            }
            TagEnd::Heading(_) => {
                if let Some(Frame::Heading(level, inlines)) = stack.pop() {
                    Self::push_block(stack, MdBlock::Heading { level, inlines });
                }
            }
            TagEnd::BlockQuote(_) => {
                if let Some(Frame::Quote(blocks)) = stack.pop() {
                    Self::push_block(stack, MdBlock::BlockQuote { blocks });
                }
            }
            TagEnd::List(_) => {
                if let Some(Frame::List(ordered, start, items)) = stack.pop() {
                    Self::push_block(
                        stack,
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
                    Self::push_block(
                        stack,
                        MdBlock::CodeBlock {
                            lang: language,
                            lines: Highlighter::snippet(&code, &token, dark),
                        },
                    );
                }
            }
            _ => {}
        }
    }

    fn end_inline_tag(stack: &mut Vec<Frame>, tag: TagEnd) {
        match tag {
            TagEnd::Emphasis => {
                if let Some(Frame::Emph(inlines)) = stack.pop() {
                    Self::push_inline(stack, MdInline::Emph(inlines));
                }
            }
            TagEnd::Strong => {
                if let Some(Frame::Strong(inlines)) = stack.pop() {
                    Self::push_inline(stack, MdInline::Strong(inlines));
                }
            }
            TagEnd::Strikethrough => {
                if let Some(Frame::Strike(inlines)) = stack.pop() {
                    Self::push_inline(stack, MdInline::Strike(inlines));
                }
            }
            TagEnd::Link => {
                if let Some(Frame::Link(href, inlines)) = stack.pop() {
                    Self::push_inline(stack, MdInline::Link { href, inlines });
                }
            }
            TagEnd::Image => {
                if let Some(Frame::Image(src, alt)) = stack.pop() {
                    Self::push_inline(stack, MdInline::Image { src, alt });
                }
            }
            _ => {}
        }
    }

    fn end_table_tag(stack: &mut Vec<Frame>, tag: TagEnd) {
        match tag {
            TagEnd::Table => {
                if let Some(Frame::Table(aligns, header, rows)) = stack.pop() {
                    Self::push_block(
                        stack,
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
            _ => {}
        }
    }

    fn finalize_blocks(
        text: &str,
        body: &str,
        metadata: MarkdownMetadata,
        line_starts: &[usize],
        blocks: Vec<MdBlock>,
        ranges: Vec<(usize, usize)>,
    ) -> Self {
        let mut parsed = Vec::with_capacity(blocks.len());
        for (block, (start, end)) in blocks.into_iter().zip(ranges) {
            parsed.push(NoteBlock {
                start_line: Self::offset_to_line(line_starts, start),
                end_line: Self::offset_to_line(line_starts, end.saturating_sub(1)) + 1,
                source: text[start..end].to_string(),
                block,
            });
        }
        if !metadata.title.is_empty()
            && !parsed
                .first()
                .is_some_and(|block| Self::heading_matches_title(&block.block, &metadata.title))
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
            let body_line = Self::offset_to_line(line_starts, metadata.body_offset);
            parsed.push(NoteBlock {
                start_line: body_line,
                end_line: body_line + 1,
                source: body.to_string(),
                block: MdBlock::Paragraph {
                    inlines: Vec::new(),
                },
            });
        }
        Self {
            title: metadata.title,
            properties: metadata.properties,
            blocks: parsed,
        }
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
            Some(Frame::Doc(blocks) | Frame::Quote(blocks) | Frame::Sink(blocks)) => {
                blocks.push(block)
            }
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
                } => Self::inline_text(children, output),
                MdInline::Image { alt, .. } => output.push_str(alt),
                MdInline::WikiLink { label, .. } => output.push_str(label),
                MdInline::SoftBreak | MdInline::HardBreak => output.push(' '),
            }
        }
    }

    fn push_text_with_wiki_links(stack: &mut [Frame], text: &str) {
        let links = WikiLink::parse_all(text);
        if links.is_empty() {
            Self::push_inline(stack, MdInline::Text(text.to_string()));
            return;
        }
        let mut offset = 0;
        for link in links {
            if offset < link.start {
                Self::push_inline(stack, MdInline::Text(text[offset..link.start].to_string()));
            }
            let target = match link.anchor.as_deref() {
                Some(anchor) if link.note.is_empty() => format!("#{anchor}"),
                Some(anchor) => format!("{}#{anchor}", link.note),
                None => link.note.clone(),
            };
            let label = link.label.clone().unwrap_or_else(|| target.clone());
            Self::push_inline(
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
            Self::push_inline(stack, MdInline::Text(text[offset..].to_string()));
        }
    }

    fn heading_matches_title(block: &MdBlock, title: &str) -> bool {
        let MdBlock::Heading { level: 1, inlines } = block else {
            return false;
        };
        let mut text = String::new();
        Self::inline_text(inlines, &mut text);
        text.trim() == title.trim()
    }

    fn offset_to_line(line_starts: &[usize], byte: usize) -> u32 {
        match line_starts.binary_search(&byte) {
            Ok(index) => index as u32,
            Err(index) => index.saturating_sub(1) as u32,
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_paths() {
        assert!(ParsedNote::supports(Path::new("a.md")));
        assert!(ParsedNote::supports(Path::new("A.MARKDOWN")));
        assert!(ParsedNote::supports(Path::new("a.mdx")));
        assert!(!ParsedNote::supports(Path::new("a.rs")));
    }

    #[test]
    fn note_blocks_have_source_ranges() {
        let blocks = ParsedNote::parse("# Title\n\nParagraph\n\n- one\n- two\n").blocks;
        assert_eq!(blocks.len(), 3);
        assert_eq!((blocks[0].start_line, blocks[0].end_line), (0, 1));
        assert_eq!((blocks[1].start_line, blocks[1].end_line), (2, 3));
        assert_eq!((blocks[2].start_line, blocks[2].end_line), (4, 6));
    }

    #[test]
    fn blank_lines_separate_paragraphs() {
        let blocks = ParsedNote::parse("first\nline\n\nsecond\nline\n").blocks;
        assert_eq!(blocks.len(), 2);
        assert_eq!((blocks[0].start_line, blocks[0].end_line), (0, 2));
        assert_eq!((blocks[1].start_line, blocks[1].end_line), (3, 5));
        assert_eq!(ParsedNote::vertical_target(&blocks, 1, 1), Some(3));
        assert_eq!(ParsedNote::vertical_target(&blocks, 3, -1), Some(1));
        assert_eq!(ParsedNote::vertical_target(&blocks, 0, 1), None);
        assert_eq!(ParsedNote::vertical_target(&blocks, 4, -1), None);
        assert_eq!(ParsedNote::vertical_target(&blocks, 0, -1), Some(0));
        assert_eq!(ParsedNote::vertical_target(&blocks, 4, 1), Some(4));
    }

    #[test]
    fn blank_lines_inside_a_fenced_code_block_stay_editable() {
        let blocks = ParsedNote::parse("```text\none\n\ntwo\n```\n\nafter\n").blocks;
        assert_eq!(blocks.len(), 2);
        assert_eq!(ParsedNote::vertical_target(&blocks, 1, 1), None);
        assert_eq!(ParsedNote::vertical_target(&blocks, 3, -1), None);
    }

    #[test]
    fn a_paragraph_after_a_loose_list_item_tracks_the_list_content_end() {
        let blocks =
            ParsedNote::parse("- `vmux/` — references.\n\nThis structure is a starting point.\n")
                .blocks;
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].start_line, 0);
        assert_eq!(blocks[1].start_line, 2);
        assert_eq!(ParsedNote::vertical_target(&blocks, 0, 1), Some(2));
        assert_eq!(ParsedNote::vertical_target(&blocks, 2, -1), Some(0));
    }

    #[test]
    fn list_items_record_source_lines() {
        let blocks = ParsedNote::parse("1. one\n   - nested\n2. two\n").blocks;
        let MdBlock::List { items, .. } = &blocks[0].block else {
            panic!("expected list block");
        };
        assert_eq!(items[0].source_line, 0);
        assert_eq!(items[1].source_line, 2);
        let MdBlock::List { items: nested, .. } = &items[0].blocks[1] else {
            panic!("expected nested list");
        };
        assert_eq!(nested[0].source_line, 1);
    }

    #[test]
    fn frontmatter_title_appears_when_body_has_no_matching_heading() {
        let note = ParsedNote::parse("---\ntitle: Welcome Home\n---\n\nBody\n");
        assert_eq!(note.title, "Welcome Home");
        assert!(matches!(
            &note.blocks[0].block,
            MdBlock::Heading { level: 1, inlines }
                if matches!(inlines.as_slice(), [MdInline::Text(text)] if text == "Welcome Home")
        ));
        assert_eq!((note.blocks[0].start_line, note.blocks[0].end_line), (1, 2));
        assert!(matches!(&note.blocks[1].block, MdBlock::Paragraph { .. }));
    }

    #[test]
    fn frontmatter_title_is_not_duplicated_above_matching_heading() {
        let note = ParsedNote::parse("---\ntitle: Welcome Home\n---\n\n# Welcome Home\n\nBody\n");
        assert_eq!(note.blocks.len(), 2);
        assert!(matches!(
            &note.blocks[0].block,
            MdBlock::Heading { level: 1, .. }
        ));
        assert_eq!((note.blocks[0].start_line, note.blocks[0].end_line), (4, 5));
    }
}
