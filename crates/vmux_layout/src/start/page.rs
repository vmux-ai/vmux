#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command::event::{COMMAND_BAR_OPEN_EVENT, CommandBarOpenEvent};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_event, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::command_bar::palette::{CommandPalette, PaletteVariant};
use crate::start::event::{START_FOCUS_INPUT_EVENT, StartDataRequest, StartFocusInput};

const START_FOCUS_PENDING: &str = "_startFocusPending";

/// The `vmux://start/` launcher page: a cinematic centered hero that requests its
/// entries on mount and renders [`CommandPalette`] in [`PaletteVariant::Start`].
#[component]
pub fn Page() -> Element {
    use_theme();
    let state =
        use_event::<CommandBarOpenEvent>(COMMAND_BAR_OPEN_EVENT, CommandBarOpenEvent::default);
    let mut mounted = use_signal(|| false);

    let _focus_listener =
        use_bin_event_listener::<StartFocusInput, _>(START_FOCUS_INPUT_EVENT, move |_| {
            focus_start_input();
        });

    use_effect(move || {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title("Start");
        }
        let _ = try_cef_bin_emit_rkyv(&StartDataRequest);
        mounted.set(true);
    });

    use_effect(|| {
        install_window_focus_refocus();
        install_keep_input_focused_on_click();
    });

    let reveal = if mounted() {
        "opacity-100 blur-0 translate-y-0"
    } else {
        "opacity-0 blur-sm translate-y-4"
    };

    rsx! {
        main {
            class: "relative isolate flex min-h-screen flex-col items-center justify-center overflow-hidden bg-background px-6 text-foreground",
            style: "background-image:radial-gradient(140% 100% at 50% -12%, rgba(129,140,248,0.05), transparent 55%);",
            div { class: "pointer-events-none absolute inset-0 -z-10 overflow-hidden",
                div { class: "absolute left-1/2 top-[16%] h-[36rem] w-[36rem] -translate-x-1/2 rounded-full blur-[150px] dark:bg-indigo-500/15" }
                div { class: "absolute left-[12%] top-1/3 h-80 w-80 rounded-full blur-[130px] dark:bg-cyan-400/10" }
                div { class: "absolute right-[12%] top-1/4 h-80 w-80 rounded-full blur-[130px] dark:bg-violet-500/12" }
                div { class: "absolute inset-x-0 bottom-0 h-1/3 bg-gradient-to-t from-transparent to-transparent dark:from-black/40" }
            }
            div {
                class: "relative flex w-full max-w-2xl flex-col items-center gap-8 transition-all duration-700 ease-out motion-reduce:transition-none {reveal}",
                div { class: "flex flex-col items-center gap-2",
                    h1 { class: "bg-gradient-to-b from-foreground to-foreground/55 bg-clip-text text-6xl font-semibold leading-none tracking-tight text-transparent",
                        "vmux"
                    }
                    p { class: "text-base text-muted-foreground", "One prompt. Anything, done." }
                }
                div { class: "w-full overflow-hidden rounded-2xl bg-foreground/[0.05] ring-1 ring-inset ring-foreground/10 shadow-2xl dark:shadow-[0_40px_120px_-32px_rgba(0,0,0,0.85)] backdrop-blur-2xl",
                    CommandPalette {
                        state,
                        variant: PaletteVariant::Start,
                        on_close: move |_| {},
                        on_dismiss: move |_| {},
                        on_activity: move |_| {},
                    }
                }
            }
        }
    }
}

/// Focus the launcher input after the host reveals the page, re-asserting focus once per animation
/// frame until the document actually holds focus. Concurrent requests share one bounded retry.
fn focus_start_input() {
    let Some(window) = web_sys::window() else {
        return;
    };
    if start_focus_pending(&window) {
        return;
    }
    set_start_focus_pending(&window, true);
    focus_start_input_retry(window, 90);
}

fn focus_start_input_retry(window: web_sys::Window, frames_left: u32) {
    let retry_window = window.clone();
    let cb = Closure::once(move || {
        if !try_focus_command_input_once() && frames_left > 1 {
            focus_start_input_retry(retry_window, frames_left - 1);
        } else {
            set_start_focus_pending(&retry_window, false);
        }
    });
    match window.request_animation_frame(cb.as_ref().unchecked_ref()) {
        Ok(_) => cb.forget(),
        Err(_) => set_start_focus_pending(&window, false),
    }
}

fn start_focus_pending(window: &web_sys::Window) -> bool {
    js_sys::Reflect::get(window, &JsValue::from_str(START_FOCUS_PENDING))
        .map(|v| v.is_truthy())
        .unwrap_or(false)
}

fn set_start_focus_pending(window: &web_sys::Window, pending: bool) {
    let _ = js_sys::Reflect::set(
        window,
        &JsValue::from_str(START_FOCUS_PENDING),
        &JsValue::from_bool(pending),
    );
}

/// Focus the input if it is not already the active element; returns true once the document holds
/// focus and the input is active (caret visible), so the retry loop can stop.
fn try_focus_command_input_once() -> bool {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return true;
    };
    let Some(el) = doc.get_element_by_id("command-bar-input") else {
        return false;
    };
    let input: web_sys::HtmlInputElement = el.unchecked_into();
    let active_is_input = doc
        .active_element()
        .map(|a| a.id() == "command-bar-input")
        .unwrap_or(false);
    if !active_is_input {
        let _ = input.focus();
        let len = input.value().len() as u32;
        let _ = input.set_selection_range(len, len);
    }
    let has_focus = doc.has_focus().unwrap_or(false);
    has_focus && active_is_input
}

/// Refocus the launcher input whenever this page's window (re)gains native focus. CEF grants an
/// OSR browser keyboard focus a frame or more after the page mounts — after the `autofocus`
/// attribute was already ignored (the document was not focused at parse time) — so without this
/// the caret never lands in the input until the user clicks. Installed once; also refocuses when
/// switching back to an already-open start page.
fn install_window_focus_refocus() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let already_bound = js_sys::Reflect::get(&window, &JsValue::from_str("_startFocusBound"))
        .map(|v| v.is_truthy())
        .unwrap_or(false);
    if already_bound {
        return;
    }
    let _ = js_sys::Reflect::set(
        &window,
        &JsValue::from_str("_startFocusBound"),
        &JsValue::TRUE,
    );

    let closure = Closure::wrap(Box::new(|| {
        focus_start_input();
    }) as Box<dyn FnMut()>);
    let target: &web_sys::EventTarget = window.as_ref();
    let _ = target.add_event_listener_with_callback("focus", closure.as_ref().unchecked_ref());
    closure.forget();
}

/// Keep the caret in the launcher input no matter where the user clicks. The start page has
/// nothing to interact with but the input and the result rows, so a click on the hero
/// background (or the card padding) should never blur the input. A capture-phase `mousedown`
/// listener cancels the default focus shift everywhere except the input itself and the results
/// list — result clicks still fire (`preventDefault` on `mousedown` does not cancel the click),
/// so selecting a result keeps working. Installed once.
fn install_keep_input_focused_on_click() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let already_bound = js_sys::Reflect::get(&window, &JsValue::from_str("_startClickBound"))
        .map(|v| v.is_truthy())
        .unwrap_or(false);
    if already_bound {
        return;
    }
    let _ = js_sys::Reflect::set(
        &window,
        &JsValue::from_str("_startClickBound"),
        &JsValue::TRUE,
    );
    let Some(document) = window.document() else {
        return;
    };

    let closure = Closure::wrap(Box::new(move |e: web_sys::Event| {
        if let Some(el) = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        {
            let on_input = el.closest("#command-bar-input").ok().flatten().is_some();
            let on_results = el.closest("#command-bar-results").ok().flatten().is_some();
            if on_input || on_results {
                return;
            }
        }
        e.prevent_default();
        if let Some(input) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("command-bar-input"))
        {
            let input: web_sys::HtmlInputElement = input.unchecked_into();
            let _ = input.focus();
        }
    }) as Box<dyn FnMut(web_sys::Event)>);
    let target: &web_sys::EventTarget = document.as_ref();
    let opts = web_sys::AddEventListenerOptions::new();
    opts.set_capture(true);
    let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
        "mousedown",
        closure.as_ref().unchecked_ref(),
        &opts,
    );
    closure.forget();
}
