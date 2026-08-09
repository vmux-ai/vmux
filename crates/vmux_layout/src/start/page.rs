#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command::event::CommandBarOpenEvent;
use vmux_ui::components::prompt_composer::{PROMPT_INPUT_ID, prompt_textarea};
use vmux_ui::components::start_hero::{START_BACKDROP_STYLE, StartBackdrop, StartHero};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_event, use_listener, use_theme};
use vmux_ui::i18n::translate;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::command_bar::palette::{CommandPalette, PaletteVariant, StartInlineTransition};
use crate::start::event::{
    START_COMMAND_BAR_OPEN_EVENT, START_FOCUS_INPUT_EVENT, StartDataRequest, StartFocusInput,
};

const START_FOCUS_PENDING: &str = "_startFocusPending";
const START_TRANSITIONED: &str = "_startTransitioned";

/// The `vmux://start/` launcher page: a cinematic centered hero that requests its
/// entries on mount and renders [`CommandPalette`] in [`PaletteVariant::Start`].
#[component]
pub fn Page(
    #[props(default)] on_inline_transition: Option<EventHandler<StartInlineTransition>>,
) -> Element {
    let locale = use_theme();
    let state = use_event::<CommandBarOpenEvent>(
        START_COMMAND_BAR_OPEN_EVENT,
        CommandBarOpenEvent::default,
    );
    let mut mounted = use_signal(|| false);

    let _focus_listener = use_listener::<StartFocusInput, _>(START_FOCUS_INPUT_EVENT, move |_| {
        focus_start_input();
    });

    use_effect(move || {
        locale();
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title(&translate("start-title"));
        }
        let _ = try_cef_bin_emit_rkyv(&StartDataRequest);
        set_start_transitioned(false);
        mounted.set(true);
    });

    use_effect(|| {
        install_window_focus_refocus();
        install_keep_input_focused_on_click();
    });

    rsx! {
        main {
            class: "relative isolate flex h-screen items-center justify-center overflow-hidden bg-background px-4 text-foreground sm:px-6",
            style: START_BACKDROP_STYLE,
            StartBackdrop {}
            StartHero { revealed: mounted(),
                div { class: "relative w-full",
                    CommandPalette {
                        state,
                        variant: PaletteVariant::Start,
                        on_close: move |_| {},
                        on_dismiss: move |_| {},
                        on_activity: move |_| {},
                        on_start_inline_transition: on_inline_transition,
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
    if start_transitioned(&window) {
        return;
    }
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

fn start_transitioned(window: &web_sys::Window) -> bool {
    js_sys::Reflect::get(window, &JsValue::from_str(START_TRANSITIONED))
        .map(|value| value.is_truthy())
        .unwrap_or(false)
}

fn set_start_transitioned(transitioned: bool) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let _ = js_sys::Reflect::set(
        &window,
        &JsValue::from_str(START_TRANSITIONED),
        &JsValue::from_bool(transitioned),
    );
}

/// Disable launcher-only focus capture before switching this document to an agent page.
pub fn begin_agent_transition() {
    set_start_transitioned(true);
}

/// Focus the input if it is not already the active element; returns true once the document holds
/// focus and the input is active (caret visible), so the retry loop can stop.
fn try_focus_command_input_once() -> bool {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return true;
    };
    let Some(input) = prompt_textarea(PROMPT_INPUT_ID) else {
        return false;
    };
    let active_is_input = doc
        .active_element()
        .map(|a| a.id() == PROMPT_INPUT_ID)
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
        let Some(window) = web_sys::window() else {
            return;
        };
        if start_transitioned(&window) {
            return;
        }
        let Some(input) = prompt_textarea(PROMPT_INPUT_ID) else {
            return;
        };
        if let Some(el) = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        {
            let on_input = el
                .closest(&format!("#{PROMPT_INPUT_ID}"))
                .ok()
                .flatten()
                .is_some();
            let on_results = el.closest("#command-bar-results").ok().flatten().is_some();
            if on_input || on_results {
                return;
            }
        }
        e.prevent_default();
        let _ = input.focus();
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
