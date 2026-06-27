#![allow(non_snake_case)]

use crate::command_bar::palette::{CommandPalette, PaletteVariant, emit_action};
use crate::command_bar::style::{command_bar_root_class, command_bar_shell_class};
use dioxus::prelude::*;
use vmux_command::event::{
    COMMAND_BAR_OPEN_EVENT, CommandBarOpenEvent, CommandBarReadyEvent, CommandBarRenderedEvent,
    CommandBarSizeEvent, command_bar_open_should_ack, command_bar_open_should_reset_input,
};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// The Cmd+K command-bar modal page: renders [`CommandPalette`] in a modal shell and
/// owns the open/ack/reveal handshake, native sizing, and outside-pointer dismiss.
#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(CommandBarOpenEvent::default);
    let mut is_open = use_signal(|| false);
    let mut current_open_id = use_signal(|| 0u64);
    let mut last_rendered_open_id = use_signal(|| 0u64);
    let mut ready_sent = use_signal(|| false);
    let mut observed_size_open_id = use_signal(|| None::<u64>);
    let mut outside_pointer_listener_installed = use_signal(|| false);

    let open_listener =
        use_bin_event_listener::<CommandBarOpenEvent, _>(COMMAND_BAR_OPEN_EVENT, move |data| {
            let open_id = data.open_id;
            let should_reset_input =
                command_bar_open_should_reset_input(current_open_id(), open_id);
            if !should_reset_input {
                if command_bar_open_should_ack(open_id) {
                    let _ = try_cef_bin_emit_rkyv(&CommandBarRenderedEvent { open_id });
                }
                return;
            }
            current_open_id.set(open_id);
            state.set(data);
            is_open.set(true);
            if command_bar_open_should_ack(open_id) {
                last_rendered_open_id.set(0);
            }
        });

    use_effect(move || {
        if !(open_listener.is_loading)()
            && !ready_sent()
            && try_cef_bin_emit_rkyv(&CommandBarReadyEvent).is_ok()
        {
            ready_sent.set(true);
        }
    });

    use_effect(move || {
        let open = is_open();
        let open_id = current_open_id();
        if open
            && open_id != 0
            && last_rendered_open_id() != open_id
            && try_cef_bin_emit_rkyv(&CommandBarRenderedEvent { open_id }).is_ok()
        {
            last_rendered_open_id.set(open_id);
        }
    });

    use_effect(move || {
        if outside_pointer_listener_installed() {
            return;
        }
        if install_command_bar_outside_pointer_listener(is_open) {
            outside_pointer_listener_installed.set(true);
        }
    });

    use_effect(move || {
        if !is_open() || !state().native_windowed {
            return;
        }
        let open_id = current_open_id();
        if observed_size_open_id() == Some(open_id) {
            return;
        }
        if install_command_bar_size_observer() {
            observed_size_open_id.set(Some(open_id));
        }
    });

    if !is_open() {
        return rsx! { div { class: "h-full w-full" } };
    }

    let native_windowed = state().native_windowed;

    rsx! {
        div {
            class: command_bar_root_class(native_windowed),
            onclick: move |_| { dismiss_command_bar(is_open); },
            div {
                id: "command-bar-shell",
                class: command_bar_shell_class(native_windowed),
                onclick: move |e| { e.stop_propagation(); },
                div { class: "pointer-events-none absolute inset-0 rounded-2xl bg-gradient-to-br from-white/20 to-transparent" }
                CommandPalette {
                    state,
                    variant: PaletteVariant::Modal,
                    on_close: move |_| { is_open.set(false); },
                    on_dismiss: move |_| { dismiss_command_bar(is_open); },
                    on_activity: move |_| {
                        if state().native_windowed {
                            schedule_command_bar_size_emit();
                        }
                    },
                }
            }
        }
    }
}

fn dismiss_command_bar(mut is_open: Signal<bool>) {
    if !is_open() {
        return;
    }
    is_open.set(false);
    emit_action("dismiss", "");
}

fn install_command_bar_outside_pointer_listener(is_open: Signal<bool>) -> bool {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return false;
    };
    if js_sys::Reflect::get(
        &document,
        &JsValue::from_str("_commandBarOutsidePointerBound"),
    )
    .map(|v| v.is_truthy())
    .unwrap_or(false)
    {
        return true;
    }

    let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
        if !is_open() {
            return;
        }
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let Some(shell) = document.get_element_by_id("command-bar-shell") else {
            return;
        };
        let Some(target) = event.target() else {
            return;
        };
        let inside_shell = target
            .dyn_ref::<web_sys::Node>()
            .is_some_and(|node| shell.contains(Some(node)));
        if inside_shell {
            return;
        }
        dismiss_command_bar(is_open);
    }) as Box<dyn FnMut(web_sys::Event)>);

    let options = web_sys::AddEventListenerOptions::new();
    options.set_capture(true);
    if document
        .add_event_listener_with_callback_and_add_event_listener_options(
            "pointerdown",
            closure.as_ref().unchecked_ref(),
            &options,
        )
        .is_err()
    {
        return false;
    }
    let _ = js_sys::Reflect::set(
        &document,
        &JsValue::from_str("_commandBarOutsidePointerBound"),
        &JsValue::TRUE,
    );
    closure.forget();
    true
}

fn emit_command_bar_size() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id("command-bar-shell") else {
        return;
    };
    let shell: web_sys::HtmlElement = el.unchecked_into();
    let document_width = document
        .document_element()
        .map(|el| el.scroll_width())
        .unwrap_or(0);
    let body_width = document.body().map(|body| body.scroll_width()).unwrap_or(0);
    let result_list_extra_height = command_bar_results_extra_height(&document);
    let width = shell
        .offset_width()
        .max(shell.scroll_width())
        .max(document_width)
        .max(body_width)
        .max(1) as u32;
    let height = shell
        .offset_height()
        .max(shell.scroll_height() + result_list_extra_height)
        .max(1) as u32;
    let _ = try_cef_bin_emit_rkyv(&CommandBarSizeEvent { width, height });
}

fn command_bar_results_extra_height(document: &web_sys::Document) -> i32 {
    let Some(el) = document.get_element_by_id("command-bar-results") else {
        return 0;
    };
    let list: web_sys::HtmlElement = el.clone().unchecked_into();
    let max_outer_height = web_sys::window()
        .and_then(|window| window.get_computed_style(&el).ok().flatten())
        .and_then(|style| style.get_property_value("max-height").ok())
        .and_then(|value| css_px_value(&value))
        .map(|height| height.ceil() as i32);
    let border_height = (list.offset_height() - list.client_height()).max(0);
    let natural_outer_height = list.scroll_height() + border_height;
    let ideal_outer_height = max_outer_height
        .map(|height| natural_outer_height.min(height))
        .unwrap_or(natural_outer_height);
    (ideal_outer_height - list.offset_height()).max(0)
}

fn css_px_value(value: &str) -> Option<f64> {
    let value = value.trim().strip_suffix("px")?.parse::<f64>().ok()?;
    value.is_finite().then_some(value.max(0.0))
}

fn schedule_command_bar_size_emit() {
    emit_command_bar_size();
    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::wrap(Box::new(move || {
        emit_command_bar_size();
    }) as Box<dyn FnMut()>);
    let _ = window.request_animation_frame(callback.as_ref().unchecked_ref());
    callback.forget();
}

fn install_command_bar_size_observer() -> bool {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return false;
    };
    let Some(el) = document.get_element_by_id("command-bar-shell") else {
        return false;
    };
    schedule_command_bar_size_emit();
    let callback = Closure::wrap(Box::new(move |_entries: JsValue| {
        schedule_command_bar_size_emit();
    }) as Box<dyn FnMut(JsValue)>);
    let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) else {
        return false;
    };
    observer.observe(&el);
    std::mem::forget(observer);
    callback.forget();
    true
}
