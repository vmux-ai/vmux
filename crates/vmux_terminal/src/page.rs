#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use crate::event::*;
use crate::matrix_rain::MatrixRain;
use crate::render_model::{
    cursor_cell_style, span_background_overlay, span_classes, span_inline_style,
    span_looks_like_suggestion,
};
use dioxus::html::Modifiers;
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use unicode_width::UnicodeWidthChar;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// ID for the outermost terminal container div.
const CONTAINER_ID: &str = "term-container";
/// ID for the hidden measurement span used to compute character dimensions.
const MEASURE_ID: &str = "term-measure";

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut rows = use_signal(std::collections::BTreeMap::<u32, Signal<TermLine>>::new);
    let mut first_row = use_signal(|| 0u32);
    let mut total_rows = use_signal(|| 0u32);
    let mut alt = use_signal(|| false);
    let mut mouse = use_signal(|| false);
    let mut following = use_signal(|| true);
    let mut last_scroll_req = use_signal(|| u32::MAX);
    let mut cols = use_signal(|| 0u16);
    let mut cursor = use_signal(|| None::<TermCursor>);
    let mut selection = use_signal(|| None::<TermSelectionRange>);
    let mut copy_mode = use_signal(|| false);
    let mut theme = use_signal(|| None::<TermThemeEvent>);
    let mut service_error = use_signal(String::new);
    let mut loading = use_signal(|| None::<(String, String)>);
    let mut prompt_draft = use_signal(|| (String::new(), false));
    let client_h = use_signal(|| 0.0f64);

    let _err_listener = use_bin_event_listener::<ServiceUnavailableEvent, _>(
        SERVICE_UNAVAILABLE_EVENT,
        move |evt| service_error.set(evt.message),
    );

    let _listener =
        use_bin_event_listener::<TermViewportPatch, _>(TERM_VIEWPORT_EVENT, move |patch| {
            let first = patch.first_row;
            first_row.set(first);
            total_rows.set(patch.total_rows);
            if *alt.peek() != patch.alt {
                alt.set(patch.alt);
            }
            if *mouse.peek() != patch.mouse {
                mouse.set(patch.mouse);
            }
            if *cols.peek() != patch.cols {
                cols.set(patch.cols);
            }

            let overscan = vmux_core::scroll::overscan_for(
                patch.rows,
                vmux_core::scroll::TERMINAL_OVERSCAN_K,
                vmux_core::scroll::OVERSCAN_FLOOR,
                vmux_core::scroll::OVERSCAN_CAP,
            );
            let keep_hi = first + patch.rows as u32 + overscan * 2 + 2;
            rows.with_mut(|map| {
                if patch.full {
                    map.clear();
                }
                for (doc_row, line) in &patch.changed_lines {
                    if let Some(mut existing) = map.get(doc_row).copied() {
                        if *existing.peek() != *line {
                            existing.set(line.clone());
                        }
                    } else {
                        map.insert(*doc_row, Signal::new(line.clone()));
                    }
                }
                map.retain(|k, _| *k >= first && *k <= keep_hi);
            });

            if *selection.peek() != patch.selection {
                selection.set(patch.selection);
            }
            if *copy_mode.peek() != patch.copy_mode {
                copy_mode.set(patch.copy_mode);
            }
            if cursor.peek().as_ref() != Some(&patch.cursor) {
                cursor.set(Some(patch.cursor.clone()));
            }
        });

    use_effect(move || {
        let _ = total_rows();
        let _ = client_h();
        if following() {
            pin_scroll_to_bottom();
        }
    });

    let _theme_listener =
        use_bin_event_listener::<TermThemeEvent, _>(TERM_THEME_EVENT, move |data| {
            theme.set(Some(data));
        });

    let _title_listener =
        use_bin_event_listener::<TermTitleEvent, _>(TERM_TITLE_EVENT, move |evt| {
            if let Some(window) = web_sys::window()
                && let Some(doc) = window.document()
            {
                doc.set_title(&evt.title);
            }
        });

    let _loading_listener =
        use_bin_event_listener::<TermLoadingEvent, _>(TERM_LOADING_EVENT, move |evt| {
            loading.set(if evt.loading {
                Some((evt.label, evt.segment))
            } else {
                prompt_draft.set((String::new(), false));
                None
            });
        });

    let _prompt_draft_listener =
        use_bin_event_listener::<AgentPromptDraftEvent, _>(AGENT_PROMPT_DRAFT_EVENT, move |evt| {
            prompt_draft.set((evt.draft, evt.skipped));
        });

    // Cell dimensions (char_width, char_height), updated by resize observer.
    let cell_dims = use_signal(|| (0.0f64, 0.0f64));
    // Last emitted mouse cell position for move-event throttling.
    let mut last_mouse_cell = use_signal(|| (-1i32, -1i32));
    // Accumulated wheel delta (pixels) not yet converted into scroll notches.
    let mut wheel_accum = use_signal(|| 0.0f64);
    // Set up character measurement span and ResizeObserver (runs once after mount).
    use_effect(move || {
        setup_measurement(cell_dims, client_h);
    });

    let theme_style = {
        let t = theme();
        match t {
            Some(t) => {
                let [fr, fg, fb] = t.foreground;
                let [cr, cg, cb] = t.cursor;
                let mut s = format!(
                    "--term-fg:rgb({fr},{fg},{fb});--term-bg:var(--background);--term-cursor:rgb({cr},{cg},{cb});"
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

    let passthrough = alt() || copy_mode() || mouse();
    let overflow_class = if passthrough {
        "overflow-hidden"
    } else {
        "overflow-auto"
    };
    let content_h = total_rows() as f64 * ch;
    let bottom_pad = if ch > 0.0 && content_h + 2.0 * padding > client_h() {
        vmux_core::scroll::follow_bottom_pad(client_h() as f32, padding as f32, ch as f32) as f64
    } else {
        0.0
    };
    let spacer_h = content_h + bottom_pad;

    rsx! {
        div {
            id: CONTAINER_ID,
            tabindex: "0",
            class: "relative h-full w-full {overflow_class} bg-term-bg text-term-fg font-mono text-sm leading-tight select-none",
            style: "{theme_style}{cell_style}outline:none;",

            onmousedown: move |e: Event<MouseData>| {
                e.prevent_default();
                focus_terminal_container();
                let dims = cell_dims();
                if let Some((col, row)) = mouse_to_cell(&e, padding, dims) {
                    emit_mouse(trigger_button_id(&e), col, row, modifier_bits(&e), true, false);
                }
            },

            onkeydown: move |e: Event<KeyboardData>| {
                e.prevent_default();
                emit_key(&e);
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

            onwheel: move |e: Event<WheelData>| {
                if !(alt() || copy_mode() || mouse()) {
                    return;
                }
                e.prevent_default();
                let dims = cell_dims();
                let (_, ch) = dims;
                let data = e.data();
                let Some(raw) = data.downcast::<web_sys::WheelEvent>() else {
                    return;
                };
                let line_px = if ch > 0.0 { ch } else { 16.0 };
                let px = match raw.delta_mode() {
                    1 => raw.delta_y() * line_px,
                    2 => raw.delta_y() * line_px * 3.0,
                    _ => raw.delta_y(),
                };
                let total = wheel_accum() + px;
                let notches = (total / line_px).trunc();
                wheel_accum.set(total - notches * line_px);
                let count = (notches as i32).clamp(-10, 10);
                if count == 0 {
                    return;
                }
                if let Some((col, row)) =
                    client_to_cell(raw.client_x() as f64, raw.client_y() as f64, padding, dims)
                {
                    let button = if count < 0 { 64 } else { 65 };
                    let modifiers = wheel_modifier_bits(raw);
                    for _ in 0..count.unsigned_abs() {
                        emit_mouse(button, col, row, modifiers, true, false);
                    }
                }
            },

            onscroll: move |_| {
                if alt() || copy_mode() || mouse() {
                    return;
                }
                let (_, ch) = cell_dims();
                if ch <= 0.0 {
                    return;
                }
                let Some(el) = scroll_el() else {
                    return;
                };
                let vis_first = (((el.scroll_top() as f64 - padding) / ch).floor()).max(0.0) as u32;
                let vis_rows = (el.client_height() as f64 / ch).ceil() as u32 + 1;
                let follow = is_following(ch);
                if follow != *following.peek() {
                    following.set(follow);
                    last_scroll_req.set(if follow { u32::MAX } else { vis_first });
                    let _ = try_cef_bin_emit_rkyv(&TermScrollEvent {
                        top_row: vis_first,
                        follow,
                    });
                    if follow {
                        return;
                    }
                }
                if follow {
                    return;
                }
                let trigger = (vis_rows as f32 * vmux_core::scroll::EDGE_TRIGGER_K).ceil() as u32;
                let loaded_first = first_row();
                let loaded_len = rows.read().len() as u32;
                if vmux_core::scroll::needs_refetch(vis_first, vis_rows, loaded_first, loaded_len, trigger)
                    && last_scroll_req() != vis_first
                {
                    last_scroll_req.set(vis_first);
                    let _ = try_cef_bin_emit_rkyv(&TermScrollEvent {
                        top_row: vis_first,
                        follow: false,
                    });
                }
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

            {
                let msg = service_error.read().clone();
                (!msg.is_empty()).then(|| rsx! {
                    div {
                        class: "absolute inset-0 z-50 flex items-center justify-center",
                        style: "background: rgba(0,0,0,0.6);",
                        div {
                            class: "rounded-md border border-ansi-1 bg-term-bg px-4 py-2 text-sm text-ansi-1",
                            "{msg}"
                        }
                    }
                })
            }

            {
                let waiting = rows.read().is_empty()
                    && service_error.read().is_empty()
                    && loading.read().is_none();
                waiting.then(|| rsx! {
                    div {
                        class: "absolute inset-0 z-40 flex items-center justify-center text-sm",
                        style: "color:#888;",
                        "Loading…"
                    }
                })
            }

            {
                let state = loading.read().clone();
                state.map(|(label, segment)| {
                    let accent = agent_accent(&segment);
                    let favicon_url = format!("vmux://agent/{segment}/cli/");
                    let words = vec![label.to_uppercase()];
                    let (draft_text, draft_skipped) = prompt_draft.read().clone();
                    let composing = !draft_skipped && !draft_text.is_empty();
                    rsx! {
                        div {
                            class: "pointer-events-none absolute inset-0 z-40 overflow-hidden bg-term-bg",
                            MatrixRain { accent_rgb: accent.rain_rgb.to_string(), words }
                            div {
                                class: "relative z-10 flex h-full w-full items-center justify-center",
                                div {
                                    class: "flex items-center gap-3 rounded-2xl bg-white/70 px-5 py-4 ring-1 ring-inset ring-black/10 backdrop-blur-md dark:bg-black/40 dark:ring-white/10",
                                    div {
                                        class: "flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-foreground/[0.06] ring-1 ring-inset ring-foreground/10",
                                        Favicon {
                                            favicon_url: "".to_string(),
                                            url: favicon_url.clone(),
                                            class: "h-5 w-5 shrink-0 rounded object-contain".to_string(),
                                            globe_class: "h-5 w-5 text-muted-foreground".to_string(),
                                        }
                                    }
                                    div {
                                        div { class: "text-sm font-semibold {accent.accent_text}", "{label}" }
                                        if composing {
                                            div {
                                                class: "mt-0.5 w-80 whitespace-pre-wrap break-words font-mono text-sm text-foreground",
                                                "{draft_text}"
                                                span { class: "ml-px inline-block h-3.5 w-1.5 align-middle animate-pulse {accent.accent_bg}" }
                                            }
                                            div {
                                                class: "mt-1 text-[10px] text-muted-foreground/70",
                                                "runs when ready · Ctrl+C clears · Esc skips"
                                            }
                                        } else if draft_skipped {
                                            div {
                                                class: "flex items-center gap-1.5 text-xs text-muted-foreground",
                                                span { class: "font-mono", "> booting" }
                                                span { class: "inline-block h-3.5 w-2 animate-pulse {accent.accent_bg}" }
                                            }
                                        } else {
                                            div {
                                                class: "mt-0.5",
                                                PromptGhost { accent_bg: accent.accent_bg.to_string() }
                                            }
                                            div {
                                                class: "mt-1 text-[10px] text-muted-foreground/70",
                                                "type a prompt · runs when ready · Esc skips"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                })
            }

            div {
                style: "padding:{padding}px;",
                div {
                    class: "relative",
                    style: "height:{spacer_h}px;",
                    {
                        let base_rows = rows();
                        rsx! {
                            for (doc_row, line) in base_rows.iter() {
                                {
                                    let top = *doc_row as f64 * ch;
                                    rsx! {
                                        div {
                                            key: "{doc_row}",
                                            style: "position:absolute;left:0;right:0;top:{top}px;",
                                            TerminalRow {
                                                row_idx: *doc_row as usize,
                                                line: *line,
                                                selection,
                                                cols,
                                            }
                                        }
                                    }
                                }
                            }
                            {
                                cursor().filter(|c| c.visible).map(|c| {
                                    let cstyle = theme()
                                        .map(|t| t.cursor_style.clone())
                                        .unwrap_or_else(|| "block".to_string());
                                    let top = c.row as f64 * ch;
                                    let left = c.col as f64 * cw;
                                    let (w, h, oy, show_ch) = match cstyle.as_str() {
                                        "beam" | "bar" => (2.0, ch, 0.0, false),
                                        "underline" => (cw, 2.0, ch - 2.0, false),
                                        _ => (cw, ch, 0.0, true),
                                    };
                                    let ctop = top + oy;
                                    rsx! {
                                        div {
                                            class: "pointer-events-none absolute whitespace-pre",
                                            style: "left:{left}px;top:{ctop}px;width:{w}px;height:{h}px;background:var(--term-cursor);color:var(--term-bg);overflow:hidden;",
                                            if show_ch {
                                                "{c.ch}"
                                            }
                                        }
                                    }
                                })
                            }
                        }
                    }
                }
            }
        }
    }
}

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
    "border-ansi-1",
];

#[component]
fn TerminalRow(
    row_idx: usize,
    line: Signal<TermLine>,
    selection: Signal<Option<TermSelectionRange>>,
    cols: Signal<u16>,
) -> Element {
    let line = line();
    let selected_cols = row_selection_cols(&selection(), row_idx, cols());

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
                {render_span(span, span_idx, None, "block")}
            }
            if let Some((sel_start, sel_end)) = selected_cols {
                div {
                    class: "absolute top-0 bottom-0 pointer-events-none",
                    style: "left:calc(var(--cw, 1ch) * {sel_start});width:calc(var(--cw, 1ch) * {sel_end - sel_start});background:rgba(255,255,255,0.25);",
                }
            }
            for link in line.links.iter() {
                {
                    let url = link.url.clone();
                    let start = link.start_col;
                    let width = link.end_col - link.start_col + 1;
                    rsx! {
                        div {
                            key: "lnk-{start}",
                            class: "vmux-link absolute top-0 bottom-0",
                            style: "left:calc(var(--cw, 1ch) * {start});width:calc(var(--cw, 1ch) * {width});z-index:2;cursor:pointer;",
                            onmousedown: move |e: Event<MouseData>| {
                                e.stop_propagation();
                                e.prevent_default();
                            },
                            onclick: move |e: Event<MouseData>| {
                                e.stop_propagation();
                                e.prevent_default();
                                let _ = try_cef_bin_emit_rkyv(&TermLinkOpenRequest { url: url.clone() });
                            },
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
fn setup_measurement(cell_dims: Signal<(f64, f64)>, client_h: Signal<f64>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(container) = document.get_element_by_id(CONTAINER_ID) else {
        return;
    };

    if document.get_element_by_id("vmux-link-style").is_none()
        && let Ok(style_el) = document.create_element("style")
    {
        let _ = style_el.set_attribute("id", "vmux-link-style");
        let style_node: &web_sys::Node = style_el.as_ref();
        style_node.set_text_content(Some(
            ".vmux-link:hover{border-bottom:2px solid var(--primary)}",
        ));
        let _ = container.append_child(&style_el);
    }

    // Create hidden measurement span (80 monospace characters).
    let measure: web_sys::Element = document.create_element("span").unwrap();
    measure
        .set_attribute(
            "style",
            "position:absolute;top:0;left:0;visibility:hidden;white-space:pre;font:inherit",
        )
        .unwrap();
    measure.set_attribute("id", MEASURE_ID).unwrap();
    let measure_node: &web_sys::Node = measure.as_ref();
    measure_node.set_text_content(Some(&"X".repeat(80)));
    container.append_child(&measure).unwrap();

    // Run initial measurement.
    do_measure(cell_dims, client_h);

    // Install ResizeObserver on container + measure span to catch both
    // viewport resizes and font-load-triggered reflows.
    let callback = Closure::wrap(Box::new(move |_entries: JsValue| {
        do_measure(cell_dims, client_h);
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
fn do_measure(mut cell_dims: Signal<(f64, f64)>, mut client_h: Signal<f64>) {
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

    let viewport_client_h = container.client_height() as f64;
    let vw = container.client_width() as f64 - pad_x;
    let vh = viewport_client_h - pad_y;
    client_h.set(viewport_client_h);

    let _ = try_cef_bin_emit_rkyv(&TermResizeEvent {
        char_width: cw as f32,
        char_height: ch as f32,
        viewport_width: vw as f32,
        viewport_height: vh as f32,
    });
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
fn mouse_to_cell(e: &Event<MouseData>, padding: f64, dims: (f64, f64)) -> Option<(u16, u16)> {
    let client = e.client_coordinates();
    client_to_cell(client.x, client.y, padding, dims)
}

fn client_to_cell(
    client_x: f64,
    client_y: f64,
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
    let x = client_x - rect.left() - padding;
    let y = client_y - rect.top() - padding;
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

fn wheel_modifier_bits(e: &web_sys::WheelEvent) -> u8 {
    let mut m = 0u8;
    if e.ctrl_key() {
        m |= MOD_CTRL;
    }
    if e.alt_key() {
        m |= MOD_ALT;
    }
    if e.shift_key() {
        m |= MOD_SHIFT;
    }
    if e.meta_key() {
        m |= MOD_SUPER;
    }
    m
}

fn focus_terminal_container() {
    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(CONTAINER_ID))
    else {
        return;
    };
    let Ok(html) = el.dyn_into::<web_sys::HtmlElement>() else {
        return;
    };
    let _ = html.focus();
}

fn emit_key(e: &Event<KeyboardData>) {
    let data = e.data();
    let Some(raw) = data.downcast::<web_sys::KeyboardEvent>() else {
        return;
    };
    let key = raw.key();
    if is_modifier_key_name(&key) {
        return;
    }
    let text = (key.chars().count() == 1).then_some(key.clone());
    let _ = try_cef_bin_emit_rkyv(&TermKeyEvent {
        key,
        code: raw.code(),
        modifiers: key_modifier_bits(raw),
        text,
    });
}

fn is_modifier_key_name(key: &str) -> bool {
    matches!(
        key,
        "Shift" | "Control" | "Alt" | "Meta" | "OS" | "Fn" | "CapsLock"
    )
}

fn key_modifier_bits(e: &web_sys::KeyboardEvent) -> u8 {
    let mut m = 0;
    if e.ctrl_key() {
        m |= MOD_CTRL;
    }
    if e.alt_key() {
        m |= MOD_ALT;
    }
    if e.shift_key() {
        m |= MOD_SHIFT;
    }
    if e.meta_key() {
        m |= MOD_SUPER;
    }
    m
}

/// The native-scroll container element (also the measurement/mouse origin).
fn scroll_el() -> Option<web_sys::Element> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(CONTAINER_ID))
}

/// True when the viewport is within ~one row of the bottom (i.e. "following").
fn is_following(ch: f64) -> bool {
    let Some(el) = scroll_el() else {
        return true;
    };
    let dist = el.scroll_height() as f64 - el.scroll_top() as f64 - el.client_height() as f64;
    dist <= ch.max(2.0) + 1.0
}

/// Pin the viewport to the bottom (used while following as output grows).
fn pin_scroll_to_bottom() {
    if let Some(el) = scroll_el() {
        el.set_scroll_top(el.scroll_height());
    }
}

/// Emit a TermMouseEvent to the Bevy host via the CEF bridge.
fn emit_mouse(button: u8, col: u16, row: u16, modifiers: u8, pressed: bool, moving: bool) {
    let _ = try_cef_bin_emit_rkyv(&TermMouseEvent {
        button,
        col,
        row,
        modifiers,
        pressed,
        moving,
    });
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

/// Example prompts cycled by [`PromptGhost`] while the boot prompt is empty.
const PROMPT_EXAMPLES: &[&str] = &[
    "Find me a hotel with AC near Paris for this weekend",
    "Find the best flight from Paris to Tokyo next month",
    "Build a landing site for my new restaurant — make it themeable",
    "Open a PR for my staged changes",
];

/// Placeholder that types out [`PROMPT_EXAMPLES`] one character at a time with a
/// blinking caret while the agent boot prompt is empty. The live draft replaces
/// it the moment the user types; unmounting clears the interval.
#[component]
fn PromptGhost(accent_bg: String) -> Element {
    let ex_idx = use_signal(|| 0usize);
    let typed = use_signal(|| 0usize);
    let cb: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = use_hook(|| Rc::new(RefCell::new(None)));
    let timer: Rc<RefCell<Option<i32>>> = use_hook(|| Rc::new(RefCell::new(None)));
    use_effect({
        let cb = cb.clone();
        let timer = timer.clone();
        move || start_prompt_typewriter(ex_idx, typed, cb.clone(), timer.clone())
    });
    use_drop({
        let cb = cb.clone();
        let timer = timer.clone();
        move || {
            if let Some(id) = timer.borrow_mut().take()
                && let Some(win) = web_sys::window()
            {
                win.clear_interval_with_handle(id);
            }
            *cb.borrow_mut() = None;
        }
    });
    let example = PROMPT_EXAMPLES[ex_idx() % PROMPT_EXAMPLES.len()];
    let full = example.chars().count();
    let shown: String = example.chars().take(typed().min(full)).collect();
    rsx! {
        div {
            class: "w-80 whitespace-pre-wrap break-words font-mono text-sm text-muted-foreground/50",
            "{shown}"
            span { class: "ml-px inline-block h-3.5 w-1.5 align-middle animate-pulse {accent_bg}" }
        }
    }
}

fn start_prompt_typewriter(
    mut ex_idx: Signal<usize>,
    mut typed: Signal<usize>,
    cb_cell: Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
    timer_cell: Rc<RefCell<Option<i32>>>,
) {
    const PAUSE_TICKS: usize = 28;
    let cb = Closure::wrap(Box::new(move || {
        let idx = *ex_idx.peek();
        let full = PROMPT_EXAMPLES[idx % PROMPT_EXAMPLES.len()].chars().count();
        let t = *typed.peek();
        if t >= full + PAUSE_TICKS {
            typed.set(0);
            ex_idx.set((idx + 1) % PROMPT_EXAMPLES.len());
        } else {
            typed.set(t + 1);
        }
    }) as Box<dyn FnMut()>);
    if let Some(win) = web_sys::window()
        && let Ok(id) = win
            .set_interval_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), 60)
    {
        *timer_cell.borrow_mut() = Some(id);
    }
    *cb_cell.borrow_mut() = Some(cb);
}
