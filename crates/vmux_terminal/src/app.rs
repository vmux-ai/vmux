#![allow(non_snake_case)]

use dioxus::html::input_data::MouseButton;
use dioxus::html::Modifiers;
use dioxus::prelude::*;
use unicode_width::UnicodeWidthChar;
use vmux_terminal::event::*;
use vmux_ui::cef_bridge::try_cef_emit_keyed;
use vmux_ui::hooks::{use_event_listener, use_theme};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

// Tailwind safelist -- these classes are generated dynamically via format!() and
// must appear as literal strings for Tailwind's content scanner to detect them.
#[rustfmt::skip]
const _TW_SAFELIST: &[&str] = &[
    "text-ansi-0",  "text-ansi-1",  "text-ansi-2",  "text-ansi-3",
    "text-ansi-4",  "text-ansi-5",  "text-ansi-6",  "text-ansi-7",
    "text-ansi-8",  "text-ansi-9",  "text-ansi-10", "text-ansi-11",
    "text-ansi-12", "text-ansi-13", "text-ansi-14", "text-ansi-15",
    "bg-ansi-0",  "bg-ansi-1",  "bg-ansi-2",  "bg-ansi-3",
    "bg-ansi-4",  "bg-ansi-5",  "bg-ansi-6",  "bg-ansi-7",
    "bg-ansi-8",  "bg-ansi-9",  "bg-ansi-10", "bg-ansi-11",
    "bg-ansi-12", "bg-ansi-13", "bg-ansi-14", "bg-ansi-15",
    "text-term-bg", "bg-term-fg",
];

/// ID for the outermost terminal container div.
const CONTAINER_ID: &str = "term-container";
/// ID for the hidden measurement span used to compute character dimensions.
const MEASURE_ID: &str = "term-measure";

#[component]
pub fn App() -> Element {
    use_theme();
    let mut viewport = use_signal(TermViewportEvent::default);
    let mut theme = use_signal(|| None::<TermThemeEvent>);

    let _listener = use_event_listener::<TermViewportEvent, _>(
        TERM_VIEWPORT_EVENT,
        move |data| {
            viewport.set(data);
        },
    );

    let _theme_listener = use_event_listener::<TermThemeEvent, _>(
        TERM_THEME_EVENT,
        move |data| {
            theme.set(Some(data));
        },
    );

    let vp = viewport();

    // Cell dimensions (char_width, char_height), updated by resize observer.
    let cell_dims = use_signal(|| (0.0f64, 0.0f64));
    // Last emitted mouse cell position for move-event throttling.
    let mut last_mouse_cell = use_signal(|| (-1i32, -1i32));

    // Set up character measurement span and ResizeObserver (runs once after mount).
    use_effect(move || {
        setup_measurement(cell_dims);
    });

    let theme_style = {
        let t = theme();
        match t {
            Some(t) => {
                let [fr, fg, fb] = t.foreground;
                let [br, bg, bb] = t.background;
                let [cr, cg, cb] = t.cursor;
                let mut s = format!(
                    "--term-fg:rgb({fr},{fg},{fb});--term-bg:rgb({br},{bg},{bb});--term-cursor:rgb({cr},{cg},{cb});"
                );
                for (i, [r, g, b]) in t.ansi.iter().enumerate() {
                    s.push_str(&format!("--ansi-{i}:rgb({r},{g},{b});"));
                }
                if !t.font_family.is_empty() {
                    // Always include bundled Nerd Font as fallback for PUA glyphs
                    s.push_str(&format!(
                        "font-family:\"{}\",\"JetBrainsMono NF\",monospace;",
                        t.font_family
                    ));
                }
                if t.font_size > 0.0 {
                    s.push_str(&format!("font-size:{}px;", t.font_size));
                }
                if t.line_height > 0.0 {
                    s.push_str(&format!("line-height:{};", t.line_height));
                }
                s
            }
            None => String::new(),
        }
    };

    let padding = theme().map(|t| t.padding).unwrap_or(4.0) as f64;
    let cursor_blink = theme().map(|t| t.cursor_blink).unwrap_or(true);
    let cursor_style = theme()
        .map(|t| t.cursor_style.clone())
        .unwrap_or_else(|| "block".into());

    // Include measured cell dimensions as CSS custom properties so they
    // survive Dioxus style re-renders and are available for row height,
    // cursor, and selection overlay positioning.
    let (cw, ch) = cell_dims();
    let cell_style = if cw > 0.0 && ch > 0.0 {
        format!("--cw:{cw}px;--ch:{ch}px;")
    } else {
        String::new()
    };

    rsx! {
        div {
            id: CONTAINER_ID,
            class: "relative h-full w-full overflow-hidden bg-term-bg text-term-fg font-mono text-sm leading-tight select-none",
            style: "{theme_style}{cell_style}",

            onmousedown: move |e: Event<MouseData>| {
                e.prevent_default();
                let dims = cell_dims();
                if let Some((col, row)) = mouse_to_cell(&e, padding, dims) {
                    emit_mouse(trigger_button_id(&e), col, row, modifier_bits(&e), true, false);
                }
            },

            onmouseup: move |e: Event<MouseData>| {
                let dims = cell_dims();
                if let Some((col, row)) = mouse_to_cell(&e, padding, dims) {
                    emit_mouse(trigger_button_id(&e), col, row, modifier_bits(&e), false, false);
                }
            },

            onmousemove: move |e: Event<MouseData>| {
                let dims = cell_dims();
                if let Some((col, row)) = mouse_to_cell(&e, padding, dims) {
                    let last = last_mouse_cell();
                    if col as i32 == last.0 && row as i32 == last.1 {
                        return;
                    }
                    last_mouse_cell.set((col as i32, row as i32));
                    let btn = held_button_id(&e);
                    emit_mouse(btn, col, row, modifier_bits(&e), true, true);
                }
            },

            oncontextmenu: move |e: Event<MouseData>| {
                e.prevent_default();
            },

            div {
                style: "padding:{padding}px;",
                for (row_idx , line) in vp.lines.iter().enumerate() {
                    {
                        // Hash span attributes so Dioxus detects row changes
                        // (class/style diffs on keyed children can be missed).
                        let sel_hash = selection_row_hash(&vp.selection, row_idx);
                        let row_hash = line.spans.iter().fold(sel_hash, |h, s| {
                            h.wrapping_mul(31)
                                .wrapping_add(s.flags as u64)
                                .wrapping_mul(31)
                                .wrapping_add(match s.bg {
                                    TermColor::Default => 0,
                                    TermColor::Indexed(i) => i as u64 + 1,
                                    TermColor::Rgb(r,g,b) => ((r as u64) << 16) | ((g as u64) << 8) | b as u64,
                                })
                        });
                        let sel_range = row_selection_cols(&vp.selection, row_idx, vp.cols);
                        {
                        let is_cursor_row = row_idx == vp.cursor.row as usize && vp.cursor.visible;
                        let cursor_col = vp.cursor.col as u16;
                        rsx! {
                    div {
                        key: "{row_idx}-{row_hash}",
                        class: "relative whitespace-pre",
                        style: "height: var(--ch, 1.2em);",
                        for (span_idx , span) in line.spans.iter().enumerate() {
                            {render_span(span, span_idx, is_cursor_row, cursor_col, &vp.cursor.ch, &cursor_style, cursor_blink)}
                        }
                        // Selection highlight overlay
                        if let Some((sel_start, sel_end)) = sel_range {
                            div {
                                class: "absolute top-0 bottom-0 pointer-events-none",
                                style: "left:calc(var(--cw, 1ch) * {sel_start});width:calc(var(--cw, 1ch) * {sel_end - sel_start});background:rgba(255,255,255,0.25);",
                            }
                        }
                    }
                        }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Measurement + ResizeObserver
// ---------------------------------------------------------------------------

/// Create a hidden measurement span, measure character dimensions, set CSS
/// custom properties, emit a resize event to Bevy, and install a
/// ResizeObserver to repeat on layout changes.
fn setup_measurement(cell_dims: Signal<(f64, f64)>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(container) = document.get_element_by_id(CONTAINER_ID) else {
        return;
    };

    // Create hidden measurement span (80 monospace characters).
    let measure: web_sys::Element = document.create_element("span").unwrap();
    measure
        .set_attribute(
            "style",
            "position:absolute;visibility:hidden;white-space:pre;font:inherit",
        )
        .unwrap();
    measure.set_attribute("id", MEASURE_ID).unwrap();
    let measure_node: &web_sys::Node = measure.as_ref();
    measure_node.set_text_content(Some(&"X".repeat(80)));
    container.append_child(&measure).unwrap();

    // Run initial measurement.
    do_measure(cell_dims);

    // Install ResizeObserver on container + measure span to catch both
    // viewport resizes and font-load-triggered reflows.
    let callback = Closure::wrap(Box::new(move |_entries: JsValue| {
        do_measure(cell_dims);
    }) as Box<dyn FnMut(JsValue)>);

    if let Ok(observer) =
        web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref())
    {
        observer.observe(&container);
        observer.observe(&measure);
        // Keep observer alive for the lifetime of the page.
        std::mem::forget(observer);
    }
    callback.forget();
}

/// Measure character dimensions from the hidden span, update CSS custom
/// properties on the container, update the Dioxus signal, and emit a
/// TermResizeEvent to the Bevy host.
fn do_measure(mut cell_dims: Signal<(f64, f64)>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(container) = document.get_element_by_id(CONTAINER_ID) else {
        return;
    };
    let Some(measure) = document.get_element_by_id(MEASURE_ID) else {
        return;
    };

    let rect = measure.get_bounding_client_rect();
    let cw = rect.width() / 80.0;

    // Prefer computed line-height (px value); fall back to measured span height.
    let ch = window
        .get_computed_style(&container)
        .ok()
        .flatten()
        .and_then(|cs| {
            cs.get_property_value("line-height")
                .ok()
                .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        })
        .unwrap_or(rect.height());

    if cw <= 0.0 || ch <= 0.0 {
        return;
    }

    cell_dims.set((cw, ch));

    // Set CSS custom properties for cursor/selection overlay positioning.
    let html: &web_sys::HtmlElement = container.unchecked_ref();
    let _ = html.style().set_property("--cw", &format!("{cw}px"));
    let _ = html.style().set_property("--ch", &format!("{ch}px"));

    // Compute viewport dimensions (container size minus inner padding).
    let (pad_x, pad_y) = container
        .first_element_child()
        .and_then(|inner| window.get_computed_style(&inner).ok().flatten())
        .map(|cs| {
            let px = parse_px(&cs, "padding-left") + parse_px(&cs, "padding-right");
            let py = parse_px(&cs, "padding-top") + parse_px(&cs, "padding-bottom");
            (px, py)
        })
        .unwrap_or((0.0, 0.0));

    let vw = container.client_width() as f64 - pad_x;
    let vh = container.client_height() as f64 - pad_y;

    try_cef_emit_keyed(&[
        ("char_width", JsValue::from_f64(cw)),
        ("char_height", JsValue::from_f64(ch)),
        ("viewport_width", JsValue::from_f64(vw)),
        ("viewport_height", JsValue::from_f64(vh)),
    ]);
}

fn parse_px(cs: &web_sys::CssStyleDeclaration, prop: &str) -> f64 {
    cs.get_property_value(prop)
        .ok()
        .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        .unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// Mouse helpers
// ---------------------------------------------------------------------------

/// Convert mouse client coordinates to terminal grid (col, row).
fn mouse_to_cell(
    e: &Event<MouseData>,
    padding: f64,
    (cw, ch): (f64, f64),
) -> Option<(u16, u16)> {
    if cw <= 0.0 || ch <= 0.0 {
        return None;
    }
    let container = web_sys::window()?
        .document()?
        .get_element_by_id(CONTAINER_ID)?;
    let rect = container.get_bounding_client_rect();
    let client = e.client_coordinates();
    let x = client.x - rect.left() - padding;
    let y = client.y - rect.top() - padding;
    let col = (x / cw).floor().max(0.0) as u16;
    let row = (y / ch).floor().max(0.0) as u16;
    Some((col, row))
}

/// Map Dioxus trigger_button to terminal protocol button number.
fn trigger_button_id(e: &Event<MouseData>) -> u8 {
    match e.trigger_button() {
        Some(MouseButton::Primary) => 0,
        Some(MouseButton::Auxiliary) => 1,
        Some(MouseButton::Secondary) => 2,
        _ => 0,
    }
}

/// Determine which button is held during a mousemove (for drag events).
fn held_button_id(e: &Event<MouseData>) -> u8 {
    let held = e.held_buttons();
    if held.contains(MouseButton::Primary) {
        0
    } else if held.contains(MouseButton::Auxiliary) {
        1
    } else if held.contains(MouseButton::Secondary) {
        2
    } else {
        3
    }
}

/// Convert Dioxus modifier flags to our MOD_* bitmask.
fn modifier_bits(e: &Event<MouseData>) -> u8 {
    let mods = e.modifiers();
    let mut m = 0u8;
    if mods.contains(Modifiers::CONTROL) {
        m |= MOD_CTRL;
    }
    if mods.contains(Modifiers::ALT) {
        m |= MOD_ALT;
    }
    if mods.contains(Modifiers::SHIFT) {
        m |= MOD_SHIFT;
    }
    if mods.contains(Modifiers::META) {
        m |= MOD_SUPER;
    }
    m
}

/// Emit a TermMouseEvent to the Bevy host via the CEF bridge.
fn emit_mouse(button: u8, col: u16, row: u16, modifiers: u8, pressed: bool, moving: bool) {
    try_cef_emit_keyed(&[
        ("button", JsValue::from_f64(button as f64)),
        ("col", JsValue::from_f64(col as f64)),
        ("row", JsValue::from_f64(row as f64)),
        ("modifiers", JsValue::from_f64(modifiers as f64)),
        ("pressed", JsValue::from_bool(pressed)),
        ("moving", JsValue::from_bool(moving)),
    ]);
}

// ---------------------------------------------------------------------------
// Span rendering
// ---------------------------------------------------------------------------

fn span_classes(span: &TermSpan) -> String {
    let mut classes = Vec::new();

    let (fg, bg) = if span.flags & FLAG_INVERSE != 0 {
        (&span.bg, &span.fg)
    } else {
        (&span.fg, &span.bg)
    };

    match fg {
        TermColor::Default => {
            if span.flags & FLAG_INVERSE != 0 {
                classes.push("text-term-bg".into());
            }
        }
        TermColor::Indexed(i) => classes.push(format!("text-ansi-{i}")),
        TermColor::Rgb(..) => {}
    }

    match bg {
        TermColor::Default => {
            if span.flags & FLAG_INVERSE != 0 {
                classes.push("bg-term-fg".into());
            }
        }
        TermColor::Indexed(i) => classes.push(format!("bg-ansi-{i}")),
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

fn span_inline_style(span: &TermSpan) -> String {
    let mut parts = Vec::new();

    let (fg, bg) = if span.flags & FLAG_INVERSE != 0 {
        (&span.bg, &span.fg)
    } else {
        (&span.fg, &span.bg)
    };

    if let TermColor::Rgb(r, g, b) = fg {
        parts.push(format!("color:rgb({r},{g},{b})"));
    }
    if let TermColor::Rgb(r, g, b) = bg {
        parts.push(format!("background:rgb({r},{g},{b})"));
    }

    parts.join(";")
}

/// Render a span, splitting it at the cursor position if the cursor falls within.
fn render_span(
    span: &TermSpan,
    span_idx: usize,
    is_cursor_row: bool,
    cursor_col: u16,
    cursor_ch: &str,
    cursor_style: &str,
    cursor_blink: bool,
) -> Element {
    let classes = span_classes(span);
    let style = span_inline_style(span);

    // Check if cursor falls within this span.
    // Use grid_cols (accounts for wide chars) with fallback to char count.
    let span_end_col = if span.grid_cols > 0 {
        span.col + span.grid_cols
    } else {
        span.col + span.text.chars().count() as u16
    };
    if is_cursor_row && cursor_col >= span.col && cursor_col < span_end_col {
        // Map grid column to char index, accounting for wide characters.
        let target_grid_col = cursor_col - span.col;
        let mut offset = 0usize;
        let mut grid_col_acc: u16 = 0;
        for (i, ch) in span.text.chars().enumerate() {
            if grid_col_acc >= target_grid_col {
                offset = i;
                break;
            }
            grid_col_acc += ch.width().unwrap_or(1) as u16;
            offset = i + 1;
        }
        let chars: Vec<char> = span.text.chars().collect();
        let before: String = chars[..offset].iter().collect();
        let after: String = chars[offset + 1..].iter().collect();

        let blink_css = if cursor_blink {
            "animation:blink 1s step-end infinite;"
        } else {
            ""
        };
        let (cursor_cls, color_css) = match cursor_style {
            "underline" => ("border-b-2 border-term-cursor", ""),
            "bar" => ("border-l-2 border-term-cursor", ""),
            _ => ("bg-term-cursor", "color:var(--term-bg);"),
        };

        rsx! {
            if !before.is_empty() {
                span {
                    key: "{span_idx}-pre",
                    class: "{classes}",
                    style: "{style}",
                    "{before}"
                }
            }
            span {
                key: "{span_idx}-cur",
                class: "{cursor_cls}",
                style: "{color_css}{blink_css}",
                "{cursor_ch}"
            }
            if !after.is_empty() {
                span {
                    key: "{span_idx}-post",
                    class: "{classes}",
                    style: "{style}",
                    "{after}"
                }
            }
        }
    } else {
        rsx! {
            span {
                key: "{span_idx}",
                class: "{classes}",
                style: "{style}",
                "{span.text}"
            }
        }
    }
}

/// Compute the selected column range for a given row, if any.
/// Returns Some((start_col, end_col_exclusive)) or None.
fn row_selection_cols(
    selection: &Option<TermSelectionRange>,
    row_idx: usize,
    total_cols: u16,
) -> Option<(usize, usize)> {
    let sel = selection.as_ref()?;
    let row = row_idx as u16;
    if row < sel.start_row || row > sel.end_row {
        return None;
    }
    if sel.is_block {
        // Block selection: same column range on every selected row
        Some((sel.start_col as usize, sel.end_col as usize + 1))
    } else if sel.start_row == sel.end_row {
        // Single line selection
        Some((sel.start_col as usize, sel.end_col as usize + 1))
    } else if row == sel.start_row {
        // First line of multi-line selection
        Some((sel.start_col as usize, total_cols as usize))
    } else if row == sel.end_row {
        // Last line of multi-line selection
        Some((0, sel.end_col as usize + 1))
    } else {
        // Middle line -- fully selected
        Some((0, total_cols as usize))
    }
}

/// Hash contribution from selection state for a given row, so Dioxus detects
/// selection changes even when the text content hasn't changed.
fn selection_row_hash(selection: &Option<TermSelectionRange>, row_idx: usize) -> u64 {
    match row_selection_cols(selection, row_idx, u16::MAX) {
        Some((start, end)) => (start as u64)
            .wrapping_mul(997)
            .wrapping_add(end as u64)
            .wrapping_mul(991)
            .wrapping_add(1),
        None => 0,
    }
}
