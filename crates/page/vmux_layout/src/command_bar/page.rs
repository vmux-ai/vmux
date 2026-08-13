#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use crate::command_bar::palette::{CommandPalette, PaletteVariant};
use crate::command_bar::size::CommandBarSizeEmissionState;
use crate::command_bar::style::{command_bar_root_class, command_bar_shell_class};
use dioxus::prelude::*;
use vmux_command::event::{
    COMMAND_BAR_OPEN_EVENT, CommandBarActionEvent, CommandBarOpenEvent, CommandBarReadyEvent,
    CommandBarRenderedEvent, CommandBarSizeEvent, OpenId,
};
use vmux_ui::dom_listener::DocumentListener;
use vmux_ui::hooks::{send, use_listener, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

std::thread_local! {
    static COMMAND_BAR_SIZE_EMISSION: RefCell<CommandBarSizeEmissionState> = RefCell::new(CommandBarSizeEmissionState::default());
}

/// The Cmd+K command-bar modal page: renders [`CommandPalette`] in a modal shell and
/// owns the open/ack/reveal handshake, native sizing, and outside-pointer dismiss.
#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(CommandBarOpenEvent::default);
    let mut is_open = use_signal(|| false);
    let mut current_open_id = use_signal(|| OpenId::NONE);
    let mut last_rendered_open_id = use_signal(|| OpenId::NONE);
    let mut render_ack_scheduled_open_id = use_signal(|| OpenId::NONE);
    let mut ready_sent = use_signal(|| false);
    let mut observed_size_open_id = use_signal(|| None::<OpenId>);

    let open_listener =
        use_listener::<CommandBarOpenEvent, _>(COMMAND_BAR_OPEN_EVENT, move |data| {
            let open_id = data.open_id;
            let should_reset_input = open_id.should_reset_input(current_open_id());
            if !should_reset_input {
                return;
            }
            current_open_id.set(open_id);
            state.set(data);
            is_open.set(true);
            if open_id.is_open() {
                last_rendered_open_id.set(OpenId::NONE);
                render_ack_scheduled_open_id.set(OpenId::NONE);
            }
        });

    use_effect(move || {
        if !(open_listener.is_loading)() && !ready_sent() && send(&CommandBarReadyEvent).is_ok() {
            ready_sent.set(true);
        }
    });

    use_effect(move || {
        let open = is_open();
        let open_id = current_open_id();
        if open
            && open_id.is_open()
            && last_rendered_open_id() != open_id
            && render_ack_scheduled_open_id() != open_id
        {
            render_ack_scheduled_open_id.set(open_id);
            if !schedule_command_bar_rendered_emit(
                open_id,
                2,
                last_rendered_open_id,
                render_ack_scheduled_open_id,
            ) {
                render_ack_scheduled_open_id.set(OpenId::NONE);
            }
        }
    });

    // `Rc` because `use_hook` clones its value out on every render and a listener must have one
    // owner — two would each try to remove it, and the second removal is the one that silently
    // does nothing.
    use_hook(|| Rc::new(command_bar_outside_pointer(is_open)));

    let mut size_observer = use_signal(|| Option::<SizeObserver>::None);
    use_effect(move || {
        if !is_open() {
            return;
        }
        let open_id = current_open_id();
        if observed_size_open_id() == Some(open_id) {
            return;
        }
        if let Some(observer) = SizeObserver::on_command_bar_shell(current_open_id) {
            size_observer.set(Some(observer));
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
                    on_close: move |_| {},
                    on_dismiss: move |_| { dismiss_command_bar(is_open); },
                    on_activity: move |_| {
                        schedule_command_bar_size_emit(current_open_id());
                    },
                }
            }
        }
    }
}

fn dismiss_command_bar(is_open: Signal<bool>) {
    if !is_open() {
        return;
    }
    let _ = send(&CommandBarActionEvent::Dismiss);
}

/// Dismiss the command bar when a pointer goes down anywhere outside its shell.
fn command_bar_outside_pointer(is_open: Signal<bool>) -> Option<DocumentListener> {
    DocumentListener::capture("pointerdown", move |event| {
        if !is_open() {
            return;
        }
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
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
    })
}

fn emit_command_bar_size(open_id: OpenId) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id("command-bar-shell") else {
        return;
    };
    let shell: web_sys::HtmlElement = el.unchecked_into();
    let shell_rect = shell.get_bounding_client_rect();
    let shell_left = shell_rect.left().round() as i32;
    let shell_top = shell_rect.top().round() as i32;
    let shell_width = shell_rect.width().round().max(1.0) as u32;
    let shell_height = shell_rect.height().round().max(1.0) as u32;
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
    let should_emit = COMMAND_BAR_SIZE_EMISSION.with(|state| {
        state.borrow().should_emit(
            open_id,
            width,
            height,
            shell_left,
            shell_top,
            shell_width,
            shell_height,
        )
    });
    if !should_emit {
        return;
    }
    if send(&CommandBarSizeEvent {
        width,
        height,
        shell_left,
        shell_top,
        shell_width,
        shell_height,
    })
    .is_ok()
    {
        COMMAND_BAR_SIZE_EMISSION.with(|state| {
            state.borrow_mut().mark_emitted(
                open_id,
                width,
                height,
                shell_left,
                shell_top,
                shell_width,
                shell_height,
            )
        });
    }
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

fn schedule_command_bar_size_emit(open_id: OpenId) {
    emit_command_bar_size(open_id);
    let should_schedule = COMMAND_BAR_SIZE_EMISSION.with(|state| state.borrow_mut().schedule());
    if !should_schedule {
        return;
    }
    let Some(window) = web_sys::window() else {
        COMMAND_BAR_SIZE_EMISSION.with(|state| state.borrow_mut().finish_schedule());
        return;
    };
    let callback = Closure::once_into_js(move || {
        COMMAND_BAR_SIZE_EMISSION.with(|state| state.borrow_mut().finish_schedule());
        emit_command_bar_size(open_id);
    })
    .unchecked_into::<js_sys::Function>();
    if window.request_animation_frame(&callback).is_err() {
        let _ = callback.call0(&JsValue::NULL);
    }
}

fn schedule_command_bar_rendered_emit(
    open_id: OpenId,
    frames_left: u8,
    mut last_rendered_open_id: Signal<OpenId>,
    mut scheduled_open_id: Signal<OpenId>,
) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let callback = Closure::once(move || {
        if frames_left > 1 {
            if !schedule_command_bar_rendered_emit(
                open_id,
                frames_left - 1,
                last_rendered_open_id,
                scheduled_open_id,
            ) {
                scheduled_open_id.set(OpenId::NONE);
            }
        } else if send(&CommandBarRenderedEvent { open_id }).is_ok() {
            last_rendered_open_id.set(open_id);
        } else {
            scheduled_open_id.set(OpenId::NONE);
        }
    });
    match window.request_animation_frame(callback.as_ref().unchecked_ref()) {
        Ok(_) => {
            callback.forget();
            true
        }
        Err(_) => false,
    }
}

/// A `ResizeObserver` on the command-bar shell, disconnected when this value is dropped.
///
/// Leaking it is worse here than for a document listener: nothing latches the install, so every
/// remount left another live observer behind, each one still holding the previous component's
/// `current_open_id` and reading it on the next resize.
struct SizeObserver {
    observer: web_sys::ResizeObserver,
    _callback: Closure<dyn FnMut(JsValue)>,
}

impl Drop for SizeObserver {
    fn drop(&mut self) {
        self.observer.disconnect();
    }
}

impl SizeObserver {
    /// `None` while the shell is not in the document yet, which is why the caller retries.
    fn on_command_bar_shell(current_open_id: Signal<OpenId>) -> Option<Self> {
        let document = web_sys::window()?.document()?;
        let shell = document.get_element_by_id("command-bar-shell")?;
        schedule_command_bar_size_emit(current_open_id());
        let callback = Closure::wrap(Box::new(move |_entries: JsValue| {
            schedule_command_bar_size_emit(current_open_id());
        }) as Box<dyn FnMut(JsValue)>);
        let observer = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()).ok()?;
        observer.observe(&shell);
        Some(Self {
            observer,
            _callback: callback,
        })
    }
}
