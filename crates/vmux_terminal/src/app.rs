#![allow(non_snake_case)]

use dioxus::html::Modifiers;
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use unicode_width::UnicodeWidthChar;
use vmux_terminal::event::*;
use vmux_terminal::render_model::{
    cursor_cell_style, span_background_overlay, span_classes, span_inline_style,
    span_looks_like_suggestion,
};
use vmux_ui::cef_bridge::try_cef_emit_keyed;
use vmux_ui::hooks::{use_event_listener, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

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
    let mut rows = use_signal(Vec::<Signal<TermLine>>::new);
    let mut cursor_rows = use_signal(Vec::<Signal<Option<TermCursor>>>::new);
    let mut cols = use_signal(|| 0u16);
    let mut cursor = use_signal(|| None::<TermCursor>);
    let mut selection = use_signal(|| None::<TermSelectionRange>);
    let mut copy_mode = use_signal(|| false);
    let mut theme = use_signal(|| None::<TermThemeEvent>);

    let _listener = use_event_listener::<TermViewportPatch, _>(TERM_VIEWPORT_EVENT, move |patch| {
        let current_cols = *cols.peek();
        let current_rows = rows.peek().len() as u16;
        if patch.requires_row_rebuild(current_cols, current_rows) {
            resize_row_signals(&mut rows, patch.rows as usize);
            resize_cursor_row_signals(&mut cursor_rows, patch.rows as usize);
        }

        let targets = rows.with_peek(|row_signals| {
            patch
                .changed_lines
                .iter()
                .filter_map(|(row_idx, line)| {
                    row_signals
                        .get(*row_idx as usize)
                        .copied()
                        .map(|row| (row, line.clone()))
                })
                .collect::<Vec<_>>()
        });
        for (mut row, line) in targets {
            if *row.peek() != line {
                row.set(line);
            }
        }

        if cursor.peek().as_ref() != Some(&patch.cursor) {
            let next_cursor = patch.cursor.clone();
            let update = cursor_row_update(cursor.peek().as_ref(), &next_cursor);
            let targets = cursor_rows.with_peek(|row_signals| CursorRowSignalUpdate {
                clear: update
                    .clear
                    .and_then(|row| row_signals.get(row as usize).copied()),
                set: update
                    .set
                    .and_then(|row| row_signals.get(row as usize).copied()),
            });
            if let Some(mut clear) = targets.clear
                && clear.peek().is_some()
            {
                clear.set(None);
            }
            if let Some(mut set) = targets.set
                && *set.peek() != Some(next_cursor.clone())
            {
                set.set(Some(next_cursor.clone()));
            }
            cursor.set(Some(next_cursor));
        }
        if *cols.peek() != patch.cols {
            cols.set(patch.cols);
        }
        if *selection.peek() != patch.selection {
            selection.set(patch.selection);
        }
        if *copy_mode.peek() != patch.copy_mode {
            copy_mode.set(patch.copy_mode);
        }
    });

    let _theme_listener = use_event_listener::<TermThemeEvent, _>(TERM_THEME_EVENT, move |data| {
        theme.set(Some(data));
    });

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

            if copy_mode() {
                if let Some(cursor) = cursor() {
                    {
                        let row = cursor.row.saturating_add(1);
                        let rows = rows().len().max(1);
                        rsx! {
                            div {
                                class: "absolute right-2 top-1 z-10 rounded bg-term-fg px-1 text-xs text-term-bg",
                                "[{row}/{rows}]"
                            }
                        }
                    }
                }
            }

            div {
                style: "padding:{padding}px;",
                div {
                    class: "relative",
                    {
                        let row_signals = rows();
                        let cursor_signals = cursor_rows();
                        rsx! {
                            for (row_idx, line) in row_signals.iter().copied().enumerate() {
                                if let Some(row_cursor) = cursor_signals.get(row_idx).copied() {
                                    TerminalRow {
                                        key: "{row_idx}",
                                        row_idx,
                                        line,
                                        cursor: row_cursor,
                                        selection,
                                        cols,
                                        theme,
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

fn resize_row_signals(rows: &mut Signal<Vec<Signal<TermLine>>>, target_len: usize) {
    rows.with_mut(|row_signals| {
        let current_len = row_signals.len();
        if current_len < target_len {
            row_signals.extend((current_len..target_len).map(|_| Signal::new(TermLine::default())));
        } else if current_len > target_len {
            row_signals.truncate(target_len);
        }
    });
}

fn resize_cursor_row_signals(
    cursor_rows: &mut Signal<Vec<Signal<Option<TermCursor>>>>,
    target_len: usize,
) {
    cursor_rows.with_mut(|row_signals| {
        let current_len = row_signals.len();
        if current_len < target_len {
            row_signals.extend((current_len..target_len).map(|_| Signal::new(None)));
        } else if current_len > target_len {
            row_signals.truncate(target_len);
        }
    });
}

struct CursorRowSignalUpdate {
    clear: Option<Signal<Option<TermCursor>>>,
    set: Option<Signal<Option<TermCursor>>>,
}

#[component]
fn TerminalRow(
    row_idx: usize,
    line: Signal<TermLine>,
    cursor: Signal<Option<TermCursor>>,
    selection: Signal<Option<TermSelectionRange>>,
    cols: Signal<u16>,
    theme: Signal<Option<TermThemeEvent>>,
) -> Element {
    let line = line();
    let cursor = cursor();
    let selected_cols = row_selection_cols(&selection(), row_idx, cols());
    let theme = theme();
    let cursor_style = theme
        .as_ref()
        .map(|theme| theme.cursor_style.as_str())
        .unwrap_or("block");

    rsx! {
        div {
            class: "relative isolate whitespace-pre",
            style: "height: var(--ch, 1.2em);",
            for (span_idx, span) in line.spans.iter().enumerate() {
                if let Some(background) = span_background_overlay(span) {
                    div {
                        key: "bg-{span_idx}",
                        class: "{background.class}",
                        style: "{background.style}",
                    }
                }
            }
            for (span_idx, span) in line.spans.iter().enumerate() {
                {render_span(span, span_idx, cursor.as_ref(), cursor_style)}
            }
            if let Some((sel_start, sel_end)) = selected_cols {
                div {
                    class: "absolute top-0 bottom-0 pointer-events-none",
                    style: "left:calc(var(--cw, 1ch) * {sel_start});width:calc(var(--cw, 1ch) * {sel_end - sel_start});background:rgba(255,255,255,0.25);",
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

    if let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
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
fn mouse_to_cell(e: &Event<MouseData>, padding: f64, (cw, ch): (f64, f64)) -> Option<(u16, u16)> {
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

fn render_span(
    span: &TermSpan,
    span_idx: usize,
    cursor: Option<&TermCursor>,
    cursor_style: &str,
) -> Element {
    let classes = span_classes(span);
    let style = span_inline_style(span);

    if let Some(cursor) = cursor
        && cursor.visible
        && span_contains_col(span, cursor.col)
    {
        let offset = span_char_offset_for_col(span, cursor.col);
        let chars = span.text.chars().collect::<Vec<_>>();
        let before = chars[..offset.min(chars.len())].iter().collect::<String>();
        let after = chars
            .get(offset.saturating_add(1)..)
            .unwrap_or(&[])
            .iter()
            .collect::<String>();
        let cursor_ch = if cursor.ch.is_empty() {
            " ".to_string()
        } else {
            cursor.ch.clone()
        };
        let suggestion = span_looks_like_suggestion(span);
        let (cursor_classes, cursor_style_attr) =
            cursor_cell_style(&classes, &style, cursor_style, suggestion);

        return rsx! {
            if !before.is_empty() {
                span {
                    class: "relative z-[1] {classes}",
                    style: "{style}",
                    "{before}"
                }
            }
            span {
                class: "relative z-[1] {cursor_classes}",
                style: "{cursor_style_attr}",
                "{cursor_ch}"
            }
            if !after.is_empty() {
                span {
                    class: "relative z-[1] {classes}",
                    style: "{style}",
                    "{after}"
                }
            }
        };
    }

    rsx! {
        span {
            key: "{span_idx}",
            class: "relative z-[1] {classes}",
            style: "{style}",
            "{span.text}"
        }
    }
}

fn span_contains_col(span: &TermSpan, col: u16) -> bool {
    let end_col = if span.grid_cols > 0 {
        span.col + span.grid_cols
    } else {
        span.col + span.text.chars().count() as u16
    };
    col >= span.col && col < end_col
}

fn span_char_offset_for_col(span: &TermSpan, col: u16) -> usize {
    let target_grid_col = col.saturating_sub(span.col);
    let mut offset = 0usize;
    let mut grid_col_acc = 0u16;
    for (i, ch) in span.text.chars().enumerate() {
        if grid_col_acc >= target_grid_col {
            return i;
        }
        grid_col_acc += ch.width().unwrap_or(1) as u16;
        offset = i + 1;
    }
    offset
}

/// Compute the selected column range for a given row, if any.
/// Returns Some((start_col, end_col_exclusive)) or None.
///
/// Normalizes the selection so it works regardless of drag direction
/// (start may be after end in either axis).
fn row_selection_cols(
    selection: &Option<TermSelectionRange>,
    row_idx: usize,
    total_cols: u16,
) -> Option<(usize, usize)> {
    let sel = selection.as_ref()?;
    let row = row_idx as u16;
    let lo_row = sel.start_row.min(sel.end_row);
    let hi_row = sel.start_row.max(sel.end_row);
    if row < lo_row || row > hi_row {
        return None;
    }
    // Normalize cols: for block selections per-axis; for linear selections
    // by row-major (start_row, start_col) order so start always comes first.
    let (sr, sc, er, ec) = if sel.is_block {
        (
            lo_row,
            sel.start_col.min(sel.end_col),
            hi_row,
            sel.start_col.max(sel.end_col),
        )
    } else if (sel.start_row, sel.start_col) <= (sel.end_row, sel.end_col) {
        (sel.start_row, sel.start_col, sel.end_row, sel.end_col)
    } else {
        (sel.end_row, sel.end_col, sel.start_row, sel.start_col)
    };

    let (start, end_exclusive) = if sel.is_block || sr == er {
        (sc as usize, ec as usize + 1)
    } else if row == sr {
        (sc as usize, total_cols as usize)
    } else if row == er {
        (0, ec as usize + 1)
    } else {
        (0, total_cols as usize)
    };

    if end_exclusive <= start {
        None
    } else {
        Some((start, end_exclusive))
    }
}
