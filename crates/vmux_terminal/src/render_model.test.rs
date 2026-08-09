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
