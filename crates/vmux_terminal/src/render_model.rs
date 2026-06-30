use crate::event::{FLAG_BOLD, FLAG_DIM, FLAG_INVERSE, FLAG_ITALIC, FLAG_STRIKETHROUGH};
use crate::event::{FLAG_UNDERLINE, TermColor, TermSpan};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpanBackgroundOverlay {
    pub class: String,
    pub style: String,
}

fn effective_colors(span: &TermSpan) -> (&TermColor, &TermColor) {
    if span.flags & FLAG_INVERSE != 0 {
        (&span.bg, &span.fg)
    } else {
        (&span.fg, &span.bg)
    }
}

pub fn span_classes(span: &TermSpan) -> String {
    let mut classes = Vec::new();

    let (fg, _) = effective_colors(span);

    match fg {
        TermColor::Default => {
            if span.flags & FLAG_INVERSE != 0 {
                classes.push("text-term-bg".into());
            }
        }
        TermColor::Indexed(i) => classes.push(format!("text-ansi-{i}")),
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

    classes.join(" ")
}

pub fn span_inline_style(span: &TermSpan) -> String {
    let mut parts = Vec::new();

    let (fg, _) = effective_colors(span);

    if let TermColor::Rgb(r, g, b) = fg {
        parts.push(format!("color:rgb({r},{g},{b})"));
    }

    parts.join(";")
}

pub fn span_background_overlay(span: &TermSpan) -> Option<SpanBackgroundOverlay> {
    let (_, bg) = effective_colors(span);
    let width = span_grid_cols(span);
    if width == 0 {
        return None;
    }

    let mut class = "absolute top-0 bottom-0 z-0 pointer-events-none".to_string();
    let mut style = format!(
        "left:calc(var(--cw, 1ch) * {});width:calc(var(--cw, 1ch) * {});",
        span.col, width
    );

    match bg {
        TermColor::Default => {
            if span.flags & FLAG_INVERSE == 0 {
                return None;
            }
            class.push_str(" bg-term-fg");
        }
        TermColor::Indexed(i) => class.push_str(&format!(" bg-ansi-{i}")),
        TermColor::Rgb(r, g, b) => style.push_str(&format!("background:rgb({r},{g},{b});")),
    }

    Some(SpanBackgroundOverlay { class, style })
}

fn span_grid_cols(span: &TermSpan) -> u16 {
    if span.grid_cols > 0 {
        return span.grid_cols;
    }
    span.text.chars().count() as u16
}

pub fn span_looks_like_suggestion(span: &TermSpan) -> bool {
    span.flags & FLAG_DIM != 0 || matches!(span.fg, TermColor::Indexed(8))
}

pub fn cursor_cell_style(
    span_classes: &str,
    span_style: &str,
    cursor_style: &str,
    suggestion: bool,
) -> (String, String) {
    if suggestion {
        let cursor_class = match cursor_style {
            "underline" => "border-b-2 border-term-cursor",
            "bar" => "border-l-2 border-term-cursor",
            _ => "bg-term-cursor",
        };
        let classes = if span_classes.is_empty() {
            cursor_class.to_string()
        } else {
            format!("{span_classes} {cursor_class}")
        };
        return (classes, span_style.to_string());
    }

    let (classes, style) = match cursor_style {
        "underline" => ("border-b-2 border-term-cursor".to_string(), ""),
        "bar" => ("border-l-2 border-term-cursor".to_string(), ""),
        _ => ("bg-term-cursor".to_string(), "color:var(--term-bg);"),
    };
    (classes, style.to_string())
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
        let classes = span_classes(&span);
        let style = span_inline_style(&span);

        let (cursor_classes, cursor_style) = cursor_cell_style(&classes, &style, "block", true);

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
        let classes = span_classes(&span);

        let (cursor_classes, cursor_style) = cursor_cell_style(&classes, "", "block", true);

        assert!(cursor_classes.contains("opacity-50"));
        assert!(!cursor_style.contains("animation:"));
    }

    #[test]
    fn block_cursor_has_static_inverse_colors() {
        let (cursor_classes, cursor_style) = cursor_cell_style("", "", "block", false);

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

        let overlay = span_background_overlay(&span).expect("rgb bg should draw overlay");

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

        let overlay = span_background_overlay(&span).expect("indexed bg should draw overlay");

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

        assert!(!span_inline_style(&span).contains("background:"));
        assert!(span_background_overlay(&span).is_some());
    }

    #[test]
    fn indexed_background_renders_only_in_overlay() {
        let span = TermSpan {
            text: "selected".into(),
            bg: TermColor::Indexed(4),
            ..TermSpan::default()
        };

        assert!(!span_classes(&span).contains("bg-ansi-4"));
        assert!(span_background_overlay(&span).is_some());
    }

    #[test]
    fn inverse_default_background_renders_only_in_overlay() {
        let span = TermSpan {
            text: "selected".into(),
            flags: FLAG_INVERSE,
            ..TermSpan::default()
        };

        assert!(!span_classes(&span).contains("bg-term-fg"));
        assert!(span_background_overlay(&span).is_some());
    }
}
