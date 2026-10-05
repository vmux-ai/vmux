use crate::event::{FLAG_BOLD, FLAG_DIM, FLAG_INVERSE, FLAG_ITALIC, FLAG_STRIKETHROUGH};
use crate::event::{FLAG_UNDERLINE, TermColor, TermSpan};
use vmux_api::terminal::CursorStyle;
use vmux_ui::cn::cn;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpanBackgroundOverlay {
    pub(crate) class: String,
    pub(crate) style: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TermSpanPresentation {
    pub(crate) classes: String,
    pub(crate) style: String,
    pub(crate) background: Option<SpanBackgroundOverlay>,
    suggestion: bool,
}

impl TermSpanPresentation {
    pub(crate) fn new(span: &TermSpan) -> Self {
        let (foreground, background) = if span.flags & FLAG_INVERSE != 0 {
            (&span.bg, &span.fg)
        } else {
            (&span.fg, &span.bg)
        };

        let mut classes = Vec::new();
        match foreground {
            TermColor::Default => {
                if span.flags & FLAG_INVERSE != 0 {
                    classes.push("text-term-bg".into());
                }
            }
            TermColor::Indexed(index) => classes.push(format!("text-ansi-{index}")),
            TermColor::Rgb(..) => {}
        }
        if span.flags & FLAG_BOLD != 0 {
            classes.push("font-bold".into());
        }
        if span.flags & FLAG_ITALIC != 0 {
            classes.push("italic".into());
        }
        if span.flags & FLAG_UNDERLINE != 0 {
            classes.push("underline".into());
        }
        if span.flags & FLAG_STRIKETHROUGH != 0 {
            classes.push("line-through".into());
        }
        if span.flags & FLAG_DIM != 0 {
            classes.push("opacity-50".into());
        }

        let style = match foreground {
            TermColor::Rgb(red, green, blue) => format!("color:rgb({red},{green},{blue})"),
            _ => String::new(),
        };

        let width = if span.grid_cols > 0 {
            span.grid_cols
        } else {
            span.text.chars().count() as u16
        };
        let background = if width == 0 {
            None
        } else {
            let mut class = "absolute top-0 bottom-0 z-0 pointer-events-none".to_string();
            let mut style = format!(
                "left:calc(var(--cw, 1ch) * {});width:calc(var(--cw, 1ch) * {});",
                span.col, width
            );
            match background {
                TermColor::Default if span.flags & FLAG_INVERSE == 0 => None,
                TermColor::Default => {
                    class.push_str(" bg-term-fg");
                    Some(SpanBackgroundOverlay { class, style })
                }
                TermColor::Indexed(index) => {
                    class.push_str(&format!(" bg-ansi-{index}"));
                    Some(SpanBackgroundOverlay { class, style })
                }
                TermColor::Rgb(red, green, blue) => {
                    style.push_str(&format!("background:rgb({red},{green},{blue});"));
                    Some(SpanBackgroundOverlay { class, style })
                }
            }
        };

        Self {
            classes: classes.join(" "),
            style,
            background,
            suggestion: span.flags & FLAG_DIM != 0 || matches!(span.fg, TermColor::Indexed(8)),
        }
    }

    pub(crate) fn cursor(&self, style: CursorStyle) -> (String, String) {
        if self.suggestion {
            let cursor_class = match style {
                CursorStyle::Underline => "border-b-2 border-term-cursor",
                CursorStyle::Bar => "border-l-2 border-term-cursor",
                CursorStyle::Block => "bg-term-cursor",
            };
            let classes = cn([self.classes.as_str(), cursor_class]);
            return (classes, self.style.clone());
        }

        let (classes, style) = match style {
            CursorStyle::Underline => ("border-b-2 border-term-cursor".to_string(), ""),
            CursorStyle::Bar => ("border-l-2 border-term-cursor".to_string(), ""),
            CursorStyle::Block => ("bg-term-cursor".to_string(), "color:var(--term-bg);"),
        };
        (classes, style.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::FLAG_DIM;

    #[test]
    fn block_suggestion_cursor_keeps_suggestion_text_color() {
        let span = TermSpan {
            text: "azi".into(),
            fg: TermColor::Indexed(8),
            ..TermSpan::default()
        };
        let presentation = TermSpanPresentation::new(&span);
        let (cursor_classes, cursor_style) = presentation.cursor(CursorStyle::Block);

        assert!(cursor_classes.contains("text-ansi-8"));
        assert!(cursor_classes.contains("bg-term-cursor"));
        assert!(!cursor_classes.contains("border-b-2"));
        assert!(!cursor_style.contains("animation:"));
        assert!(!cursor_style.contains("color:var(--term-bg)"));
    }

    #[test]
    fn dim_suggestion_cursor_keeps_opacity_class() {
        let span = TermSpan {
            text: "azi".into(),
            fg: TermColor::Default,
            flags: FLAG_DIM,
            ..TermSpan::default()
        };
        let presentation = TermSpanPresentation::new(&span);
        let (cursor_classes, cursor_style) = presentation.cursor(CursorStyle::Block);

        assert!(cursor_classes.contains("opacity-50"));
        assert!(!cursor_style.contains("animation:"));
    }

    #[test]
    fn block_cursor_has_static_inverse_colors() {
        let presentation = TermSpanPresentation::new(&TermSpan::default());
        let (cursor_classes, cursor_style) = presentation.cursor(CursorStyle::Block);

        assert_eq!(cursor_classes, "bg-term-cursor");
        assert_eq!(cursor_style, "color:var(--term-bg);");
    }

    #[test]
    fn background_overlay_preserves_full_width_rgb_highlight() {
        let span = TermSpan {
            text: "selected".into(),
            bg: TermColor::Rgb(32, 80, 160),
            col: 4,
            grid_cols: 20,
            ..TermSpan::default()
        };

        let overlay = TermSpanPresentation::new(&span)
            .background
            .expect("rgb bg should draw overlay");

        assert!(overlay.class.contains("absolute top-0 bottom-0"));
        assert!(overlay.class.contains("z-0"));
        assert!(overlay.style.contains("left:calc(var(--cw, 1ch) * 4)"));
        assert!(overlay.style.contains("width:calc(var(--cw, 1ch) * 20)"));
        assert!(overlay.style.contains("background:rgb(32,80,160)"));
    }

    #[test]
    fn background_overlay_preserves_indexed_highlight() {
        let span = TermSpan {
            text: "selected".into(),
            bg: TermColor::Indexed(4),
            col: 1,
            grid_cols: 80,
            ..TermSpan::default()
        };

        let overlay = TermSpanPresentation::new(&span)
            .background
            .expect("indexed bg should draw overlay");

        assert!(overlay.class.contains("bg-ansi-4"));
        assert!(overlay.style.contains("width:calc(var(--cw, 1ch) * 80)"));
    }

    #[test]
    fn rgb_background_renders_only_in_overlay() {
        let span = TermSpan {
            text: "selected".into(),
            bg: TermColor::Rgb(32, 80, 160),
            ..TermSpan::default()
        };

        let presentation = TermSpanPresentation::new(&span);
        assert!(!presentation.style.contains("background:"));
        assert!(presentation.background.is_some());
    }

    #[test]
    fn indexed_background_renders_only_in_overlay() {
        let span = TermSpan {
            text: "selected".into(),
            bg: TermColor::Indexed(4),
            ..TermSpan::default()
        };

        let presentation = TermSpanPresentation::new(&span);
        assert!(!presentation.classes.contains("bg-ansi-4"));
        assert!(presentation.background.is_some());
    }

    #[test]
    fn inverse_default_background_renders_only_in_overlay() {
        let span = TermSpan {
            text: "selected".into(),
            flags: FLAG_INVERSE,
            ..TermSpan::default()
        };

        let presentation = TermSpanPresentation::new(&span);
        assert!(!presentation.classes.contains("bg-term-fg"));
        assert!(presentation.background.is_some());
    }
}
