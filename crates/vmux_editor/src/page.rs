#![allow(non_snake_case)]

use crate::page_model::{gutter_width, span_style};
use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

const CONTAINER_ID: &str = "file-container";
const MEASURE_ID: &str = "file-measure";

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut path = use_signal(String::new);
    let mut language = use_signal(String::new);
    let mut total_lines = use_signal(|| 0u32);
    let mut first_line = use_signal(|| 0u32);
    let mut lines = use_signal(Vec::<FileLine>::new);
    let mut error = use_signal(String::new);
    let mut dir_entries = use_signal(Vec::<FileDirEntry>::new);
    let mut is_dir = use_signal(|| false);
    let mut theme_style = use_signal(String::new);
    let cell_dims = use_signal(|| (0.0f64, 0.0f64));

    let _meta = use_bin_event_listener::<FileMetaEvent, _>(FILE_META_EVENT, move |m| {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let name = m.path.rsplit('/').next().unwrap_or(&m.path).to_string();
            doc.set_title(&name);
        }
        path.set(m.path);
        language.set(m.language);
        total_lines.set(m.total_lines);
    });

    let _vp = use_bin_event_listener::<FileViewportPatch, _>(FILE_VIEWPORT_EVENT, move |p| {
        first_line.set(p.first_line);
        total_lines.set(p.total_lines);
        lines.set(p.lines);
    });

    let _err = use_bin_event_listener::<FileErrorEvent, _>(FILE_ERROR_EVENT, move |e| {
        error.set(e.message);
    });

    let _dir = use_bin_event_listener::<FileDirEvent, _>(FILE_DIR_EVENT, move |d| {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let name = d
                .path
                .rsplit('/')
                .find(|s| !s.is_empty())
                .unwrap_or(&d.path)
                .to_string();
            doc.set_title(&name);
        }
        path.set(d.path);
        dir_entries.set(d.entries);
        is_dir.set(true);
    });

    let _theme = use_bin_event_listener::<FileThemeEvent, _>(FILE_THEME_EVENT, move |t| {
        let mut s = String::new();
        if !t.font_family.is_empty() {
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
        theme_style.set(s);
    });

    use_effect(move || {
        setup_measurement(cell_dims);
    });

    let gw = gutter_width(total_lines());

    rsx! {
        div {
            id: CONTAINER_ID,
            tabindex: "0",
            class: "relative flex h-full w-full flex-col overflow-hidden bg-term-bg text-term-fg font-mono text-sm leading-normal",
            style: "outline:none;{theme_style}",

            onmousedown: move |_| focus_container(),

            onwheel: move |e: Event<WheelData>| {
                e.prevent_default();
                let (_, ch) = cell_dims();
                let line_px = if ch > 0.0 { ch } else { 16.0 };
                let data = e.data();
                let Some(raw) = data.downcast::<web_sys::WheelEvent>() else {
                    return;
                };
                let notches = (raw.delta_y() / line_px).round() as i64;
                if notches == 0 {
                    return;
                }
                let next = (first_line() as i64 + notches).max(0) as u32;
                let _ = try_cef_bin_emit_rkyv(&FileScrollEvent { top_line: next });
            },

            onkeydown: move |e: Event<KeyboardData>| {
                let data = e.data();
                let Some(raw) = data.downcast::<web_sys::KeyboardEvent>() else {
                    return;
                };
                let cur = first_line() as i64;
                let next = match raw.key().as_str() {
                    "ArrowDown" => cur + 1,
                    "ArrowUp" => cur - 1,
                    "PageDown" => cur + 20,
                    "PageUp" => cur - 20,
                    "Home" => 0,
                    _ => return,
                };
                e.prevent_default();
                let _ = try_cef_bin_emit_rkyv(&FileScrollEvent {
                    top_line: next.max(0) as u32,
                });
            },

            div {
                class: "flex h-9 shrink-0 items-center gap-2 border-b border-white/[0.07] bg-black/20 px-4 font-sans text-xs text-muted-foreground",
                span { class: "truncate", "{path}" }
                if !language().is_empty() {
                    span {
                        class: "ml-auto shrink-0 rounded bg-white/[0.06] px-1.5 py-0.5 text-[10px] uppercase tracking-wide opacity-80",
                        "{language}"
                    }
                }
            }

            {
                let msg = error.read().clone();
                (!msg.is_empty()).then(|| rsx! {
                    div {
                        class: "absolute inset-0 z-50 flex items-center justify-center",
                        style: "background:rgba(0,0,0,0.6);",
                        div {
                            class: "rounded-md border border-ansi-1 bg-term-bg px-4 py-2 text-sm text-ansi-1",
                            "{msg}"
                        }
                    }
                })
            }

            if is_dir() {
                div { class: "min-h-0 flex-1 overflow-y-auto p-3",
                    div { class: "flex flex-wrap content-start gap-1",
                        for entry in dir_entries().iter() {
                            div {
                                key: "{entry.path}",
                                class: "flex w-28 cursor-default flex-col items-center gap-1 rounded-md p-2 text-center hover:bg-white/5",
                                title: "{entry.path}",
                                if entry.is_dir {
                                    Icon { class: "h-10 w-10 text-blue-300/80",
                                        path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                                    }
                                } else {
                                    Icon { class: "h-10 w-10 text-muted-foreground",
                                        path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                                        path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
                                    }
                                }
                                span { class: "w-full truncate text-xs text-foreground", "{entry.name}" }
                            }
                        }
                    }
                }
            } else {
                div { class: "min-h-0 flex-1 overflow-auto",
                    div { class: "min-w-max py-2",
                        for line in lines().iter() {
                            div { key: "{line.line_no}", class: "group flex hover:bg-white/[0.035]",
                                span {
                                    class: "sticky left-0 z-[1] shrink-0 select-none bg-term-bg pl-4 pr-5 text-right tabular-nums opacity-40 group-hover:opacity-90",
                                    style: "min-width:calc(var(--cw, 1ch) * {gw} + 2.25rem);",
                                    "{line.line_no + 1}"
                                }
                                span { class: "whitespace-pre pr-8",
                                    for (i, s) in line.spans.iter().enumerate() {
                                        span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
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

fn focus_container() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(CONTAINER_ID))
        && let Ok(html) = el.dyn_into::<web_sys::HtmlElement>()
    {
        let _ = html.focus();
    }
}

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

    do_measure(cell_dims);

    let callback = Closure::wrap(Box::new(move |_entries: JsValue| {
        do_measure(cell_dims);
    }) as Box<dyn FnMut(JsValue)>);

    if let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
        observer.observe(&container);
        observer.observe(&measure);
        std::mem::forget(observer);
    }
    callback.forget();
}

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

    let html: &web_sys::HtmlElement = container.unchecked_ref();
    let _ = html.style().set_property("--cw", &format!("{cw}px"));
    let _ = html.style().set_property("--ch", &format!("{ch}px"));

    let vh = container.client_height() as f64;

    let _ = try_cef_bin_emit_rkyv(&FileResizeEvent {
        char_height: ch as f32,
        viewport_height: vh as f32,
    });
}
