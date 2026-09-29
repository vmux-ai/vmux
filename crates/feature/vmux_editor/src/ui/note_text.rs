#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoteInlineKind {
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
pub(super) enum NoteInlineNode {
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
    pub(super) fn start(&self) -> u32 {
        match self {
            Self::Text { start, .. } | Self::Syntax { start, .. } => *start,
        }
    }

    pub(super) fn end(&self) -> u32 {
        match self {
            Self::Text { end, .. } | Self::Syntax { end, .. } => *end,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct NoteText<'a> {
    source: &'a str,
}

impl<'a> NoteText<'a> {
    pub(super) fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub(super) fn list_marker_prefix_len(line: &str) -> Option<(usize, usize)> {
        let chars = line.chars().collect::<Vec<_>>();
        let indent = chars
            .iter()
            .take_while(|character| character.is_whitespace())
            .count();
        let rest = &chars[indent..];
        let marker_end =
            if rest.len() >= 2 && matches!(rest[0], '-' | '*' | '+') && rest[1].is_whitespace() {
                2
            } else {
                let digits = rest
                    .iter()
                    .take_while(|character| character.is_ascii_digit())
                    .count();
                if digits == 0
                    || rest.len() <= digits + 1
                    || !matches!(rest[digits], '.' | ')')
                    || !rest[digits + 1].is_whitespace()
                {
                    return None;
                }
                digits + 2
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

    pub(super) fn inline_nodes(self, heading_level: Option<u8>) -> Vec<NoteInlineNode> {
        let chars = self.source.chars().collect::<Vec<_>>();
        let prefix = heading_level
            .map(|level| level as usize)
            .filter(|level| {
                chars.len() > *level
                    && chars[..*level].iter().all(|character| *character == '#')
                    && chars[*level].is_whitespace()
            })
            .map_or(0, |level| level + 1);
        let children = Self::parse_inline_range(&chars, prefix, chars.len());
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

    pub(super) fn offset(self, start_line: u32, line: u32, col: u32) -> u32 {
        let target = line.saturating_sub(start_line) as usize;
        let lines = self.source.split('\n').collect::<Vec<_>>();
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

    pub(super) fn position(self, start_line: u32, offset: u32) -> (u32, u32) {
        let mut line = start_line;
        let mut col = 0;
        for character in self.source.chars().take(offset as usize) {
            if character == '\n' {
                line = line.saturating_add(1);
                col = 0;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    fn find_chars(chars: &[char], from: usize, end: usize, needle: &[char]) -> Option<usize> {
        if needle.is_empty() || from >= end || needle.len() > end.saturating_sub(from) {
            return None;
        }
        (from..=end - needle.len()).find(|index| chars[*index..].starts_with(needle))
    }

    fn inline_syntax_at(chars: &[char], index: usize, end: usize) -> Option<NoteInlineNode> {
        let syntax =
            |kind, start, prefix_end, suffix_start, end, children| NoteInlineNode::Syntax {
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
            && let Some(close) = Self::find_chars(chars, index + open_len, end, &[']', ']'])
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
                Self::parse_inline_range(chars, label, close),
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
            && let Some(label_end) = Self::find_chars(chars, index + open_len, end, &[']', '('])
            && let Some(close) = Self::find_chars(chars, label_end + 2, end, &[')'])
        {
            return Some(syntax(
                NoteInlineKind::Link,
                index,
                index + open_len,
                label_end,
                close + 1,
                Self::parse_inline_range(chars, index + open_len, label_end),
            ));
        }

        if chars[index] == '`' {
            let run = chars[index..end]
                .iter()
                .take_while(|character| **character == '`')
                .count();
            if let Some(close) = Self::find_chars(chars, index + run, end, &vec!['`'; run])
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
                && let Some(close) =
                    Self::find_chars(chars, index + delimiter.len(), end, delimiter)
                && close > index + delimiter.len()
            {
                return Some(syntax(
                    kind,
                    index,
                    index + delimiter.len(),
                    close,
                    close + delimiter.len(),
                    Self::parse_inline_range(chars, index + delimiter.len(), close),
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
                    Self::parse_inline_range(chars, index + 1, close),
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
            let Some(node) = Self::inline_syntax_at(chars, index, end) else {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    struct LiveText;

    impl LiveText {
        fn from_source(source: &str, caret: u32) -> String {
            let chars = source.chars().collect::<Vec<_>>();
            let nodes = NoteText::new(source).inline_nodes(None);
            let mut output = String::new();
            Self::append(&chars, &nodes, caret, &mut output);
            output
        }

        fn append(source: &[char], nodes: &[NoteInlineNode], caret: u32, output: &mut String) {
            for node in nodes {
                match node {
                    NoteInlineNode::Text { start, end } => {
                        output.extend(source[*start as usize..*end as usize].iter().map(
                            |character| match *character == '\n' {
                                true => ' ',
                                false => *character,
                            },
                        ));
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
                        Self::append(source, children, caret, output);
                        if reveal {
                            output.extend(source[*suffix_start as usize..*end as usize].iter());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn list_prefix_excludes_marker_and_task_checkbox() {
        assert_eq!(NoteText::list_marker_prefix_len("- item"), Some((0, 2)));
        assert_eq!(NoteText::list_marker_prefix_len("  12. item"), Some((2, 6)));
        assert_eq!(NoteText::list_marker_prefix_len("- [ ] task"), Some((0, 6)));
        assert_eq!(
            NoteText::list_marker_prefix_len("  * [x] done"),
            Some((2, 8))
        );
        assert_eq!(NoteText::list_marker_prefix_len("paragraph"), None);
    }

    #[test]
    fn live_preview_preserves_paragraph_flow() {
        let source = "first line\nsecond line\nthird line";
        let text = NoteText::new(source);
        assert_eq!(
            LiveText::from_source(source, 4),
            "first line second line third line"
        );
        assert_eq!(text.offset(5, 7, 3), 26);
        assert_eq!(text.position(5, 26), (7, 3));
    }

    #[test]
    fn live_preview_reveals_only_active_inline_syntax() {
        let source = "plain `code` and **bold** with [link](https://vmux.ai)";
        assert_eq!(
            LiveText::from_source(source, 2),
            "plain code and bold with link"
        );
        assert_eq!(
            LiveText::from_source(source, 8),
            "plain `code` and bold with link"
        );
        assert_eq!(
            LiveText::from_source(source, 20),
            "plain code and **bold** with link"
        );
        assert_eq!(
            LiveText::from_source(source, 35),
            "plain code and bold with [link](https://vmux.ai)"
        );
    }

    #[test]
    fn live_preview_uses_wiki_link_label() {
        let source = "See [[projects/vmux|vmux project]] now";
        assert_eq!(LiveText::from_source(source, 1), "See vmux project now");
        assert_eq!(
            LiveText::from_source(source, 10),
            "See [[projects/vmux|vmux project]] now"
        );
    }
}
