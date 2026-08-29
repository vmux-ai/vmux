use ropey::Rope;
use vmux_core::event::{FileIndent, FileLineEnding};

const SAMPLE_LINES: usize = 2000;
const WIDTHS: &[u16] = &[2, 4, 8];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        for candidate in WIDTHS.iter().rev() {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reindent {
    pub from: FileIndent,
    pub to: BufferShape,
}

impl Reindent {
    pub fn applied(&self, text: &str) -> String {
        let newline = match self.to.line_ending {
            FileLineEnding::Crlf => "\r\n",
            FileLineEnding::Lf => "\n",
        };
        let stop = usize::from(self.from.width).max(1);
        let step = usize::from(self.to.indent.width).max(1);
        let mut out = String::with_capacity(text.len());
        for line in text.split_inclusive('\n') {
            let (body, terminated) = match line.strip_suffix('\n') {
                Some(body) => (body.strip_suffix('\r').unwrap_or(body), true),
                None => (line, false),
            };
            let mut columns = 0usize;
            let mut lead = 0usize;
            for c in body.chars() {
                match c {
                    '\t' => columns += stop - columns % stop,
                    ' ' => columns += 1,
                    _ => break,
                }
                lead += 1;
            }
            let levels = columns / stop;
            let remainder = columns % stop;
            if self.to.indent.spaces {
                for _ in 0..levels * step + remainder {
                    out.push(' ');
                }
            } else {
                for _ in 0..levels {
                    out.push('\t');
                }
                for _ in 0..remainder {
                    out.push(' ');
                }
            }
            out.push_str(&body[lead..]);
            if terminated {
                out.push_str(newline);
            }
        }
        out
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
    fn four_space_source_is_not_read_as_two() {
        let rope = Rope::from_str(
            "fn a() {\n    let x = 1;\n    if x {\n        y();\n    }\n}\n\nfn b() {\n    z();\n}\n",
        );

        let shape = BufferShape::of(&rope);

        assert!(shape.indent.spaces);
        assert_eq!(shape.indent.width, 4);
    }

    #[test]
    fn eight_space_source_is_not_read_as_four() {
        let rope = Rope::from_str("fn a() {\n        x();\n}\n");

        assert_eq!(BufferShape::of(&rope).indent.width, 8);
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

    impl BufferShape {
        fn spaces(width: u16) -> Self {
            Self {
                indent: FileIndent {
                    spaces: true,
                    width,
                },
                line_ending: FileLineEnding::Lf,
            }
        }

        fn tabs(width: u16) -> Self {
            Self {
                indent: FileIndent {
                    spaces: false,
                    width,
                },
                line_ending: FileLineEnding::Lf,
            }
        }
    }

    impl Reindent {
        fn of(text: &str, to: BufferShape) -> String {
            let from = BufferShape::of(&Rope::from_str(text)).indent;
            Self { from, to }.applied(text)
        }
    }

    #[test]
    fn converting_tabs_to_spaces_widens_each_level_to_the_target_width() {
        let out = Reindent::of("fn a() {\n\tb();\n\t\tc();\n}\n", BufferShape::spaces(4));

        assert_eq!(out, "fn a() {\n    b();\n        c();\n}\n");
    }

    #[test]
    fn converting_spaces_to_tabs_folds_whole_levels_and_keeps_the_remainder() {
        let out = Reindent {
            from: FileIndent {
                spaces: true,
                width: 4,
            },
            to: BufferShape::tabs(4),
        }
        .applied("a\n    b\n      c\n");

        assert_eq!(out, "a\n\tb\n\t  c\n");
    }

    #[test]
    fn a_new_width_reindents_every_level_rather_than_retyping_the_same_columns() {
        let out = Reindent::of("a\n  b\n    c\n", BufferShape::spaces(4));

        assert_eq!(out, "a\n    b\n        c\n");
    }

    #[test]
    fn conversion_leaves_text_after_the_indentation_untouched() {
        let out = Reindent::of("\tlet s = \"\tkeep\tthese\";\n", BufferShape::spaces(4));

        assert_eq!(out, "    let s = \"\tkeep\tthese\";\n");
    }

    #[test]
    fn line_endings_are_rewritten_in_both_directions() {
        let crlf = BufferShape {
            indent: FileIndent {
                spaces: true,
                width: 4,
            },
            line_ending: FileLineEnding::Crlf,
        };

        assert_eq!(Reindent::of("a\nb\n", crlf), "a\r\nb\r\n");
        assert_eq!(Reindent::of("a\r\nb\r\n", BufferShape::spaces(4)), "a\nb\n");
    }

    #[test]
    fn a_file_without_a_trailing_newline_does_not_gain_one() {
        assert_eq!(Reindent::of("a\nb", BufferShape::spaces(4)), "a\nb");
        assert_eq!(Reindent::of("", BufferShape::spaces(4)), "");
    }

    #[test]
    fn reshaping_to_the_shape_a_buffer_already_has_is_a_no_op() {
        let text = "fn a() {\n    b();\n        c();\n}\n";
        let shape = BufferShape::of(&Rope::from_str(text));

        assert_eq!(Reindent::of(text, shape), text);
    }
}
