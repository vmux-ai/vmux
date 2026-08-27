use ropey::Rope;
use vmux_core::event::{FileIndent, FileLineEnding};

const SAMPLE_LINES: usize = 2000;
const WIDTHS: &[u16] = &[2, 4, 8];

pub struct BufferShape {
    pub indent: FileIndent,
    pub line_ending: FileLineEnding,
}

impl BufferShape {
    pub fn of(rope: &Rope) -> Self {
        let total = rope.len_lines().min(SAMPLE_LINES);
        let mut crlf = 0usize;
        let mut tabs = 0usize;
        let mut widths = Vec::new();
        for i in 0..total {
            let line: String = rope.line(i).chars().collect();
            if line.ends_with("\r\n") {
                crlf += 1;
            }
            let body = line.trim_end_matches(['\n', '\r']);
            match body.chars().next() {
                Some('\t') => tabs += 1,
                Some(' ') => {
                    let spaces = body.chars().take_while(|c| *c == ' ').count();
                    if spaces > 0 && body.len() > spaces {
                        widths.push(spaces);
                    }
                }
                _ => {}
            }
        }
        let line_ending = match crlf * 2 > total {
            true => FileLineEnding::Crlf,
            false => FileLineEnding::Lf,
        };
        if tabs > widths.len() {
            return Self {
                indent: FileIndent {
                    spaces: false,
                    width: 4,
                },
                line_ending,
            };
        }
        Self {
            indent: FileIndent {
                spaces: true,
                width: Self::common_width(&widths),
            },
            line_ending,
        }
    }

    fn common_width(widths: &[usize]) -> u16 {
        let mut best = 4;
        let mut best_hits = 0;
        for candidate in WIDTHS {
            let mut hits = 0;
            for width in widths {
                if *width % usize::from(*candidate) == 0 {
                    hits += 1;
                }
            }
            if hits > best_hits {
                best_hits = hits;
                best = *candidate;
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_space_source_is_not_read_as_four() {
        let rope = Rope::from_str("fn a() {\n  let x = 1;\n  if x {\n    y();\n  }\n}\n");

        let shape = BufferShape::of(&rope);

        assert!(shape.indent.spaces);
        assert_eq!(shape.indent.width, 2);
        assert_eq!(shape.line_ending, FileLineEnding::Lf);
    }

    #[test]
    fn a_tab_indented_file_reports_tabs() {
        let rope = Rope::from_str("fn a() {\n\tlet x = 1;\n\tlet y = 2;\n}\n");

        let shape = BufferShape::of(&rope);

        assert!(!shape.indent.spaces);
    }

    #[test]
    fn windows_line_endings_are_reported_as_crlf() {
        let rope = Rope::from_str("a\r\nb\r\nc\r\n");

        assert_eq!(BufferShape::of(&rope).line_ending, FileLineEnding::Crlf);
    }
}
