#![allow(non_snake_case)]

use crate::event::{
    HardwareButton, SIMULATOR_READY_EVENT, SimulatorGesture, SimulatorKey, SimulatorReady,
};
use crate::url::{IosVersion, SimulatorRoute};
use dioxus::prelude::*;
use vmux_ui::hooks::{send, use_event, use_theme};

/// The simulator view.
///
/// The device image is an `<img>` on a loopback MJPEG stream rather than anything drawn here or
/// in Bevy: `axe stream-video` already emits `multipart/x-mixed-replace`, which Chromium renders
/// natively. Pointer positions go back to the host as fractions of the image, so this half never
/// needs to know the device's resolution.
#[component]
pub fn Page() -> Element {
    use_theme();
    let ready = use_event::<SimulatorReady>(SIMULATOR_READY_EVENT, SimulatorReady::default);
    let route = CurrentRoute::read();

    let announced = ready();
    use_effect(move || {
        let announced = ready();
        if !matches!(CurrentRoute::read(), Some(SimulatorRoute::Unpinned)) {
            return;
        }
        let Some(version) = IosVersion::parse(&announced.version) else {
            return;
        };
        CurrentRoute::replace(&version);
    });

    rsx! {
        div { class: "flex h-screen w-screen items-center justify-center overflow-hidden bg-background",
            if announced.port == 0 {
                Waiting { route }
            } else {
                Mirror { port: announced.port }
            }
        }
    }
}

/// The live device. `object-contain` letterboxes it, which is what makes a pointer fraction of
/// the rendered box a fraction of the device.
///
/// `tabindex` is what lets the mirror take keyboard focus at all — an `<img>` cannot — and
/// clicking it focuses it, so typing follows the tap that put the caret in a field.
#[component]
fn Mirror(port: u16) -> Element {
    let mut press = use_signal(|| None::<(f32, f32)>);

    rsx! {
        div {
            class: "flex h-full w-full items-center justify-center outline-none",
            tabindex: 0,
            onkeydown: move |event| {
                let Some(key) = Keystroke::of(&event) else {
                    return;
                };
                event.prevent_default();
                let _ = send(&key);
            },
            img {
                class: "h-full w-full object-contain select-none",
                draggable: false,
                src: "http://127.0.0.1:{port}/",
                onmousedown: move |event| press.set(Pointer::fraction(&event)),
                onmouseup: move |event| {
                    let Some(from) = press.take() else {
                        return;
                    };
                    let Some(to) = Pointer::fraction(&event) else {
                        return;
                    };
                    let _ = send(&SimulatorGesture {
                        from_x: from.0,
                        from_y: from.1,
                        to_x: to.0,
                        to_y: to.1,
                    });
                },
            }
        }
    }
}

/// What a key press on the focused mirror means to the device.
struct Keystroke;

impl Keystroke {
    fn of(event: &Event<KeyboardData>) -> Option<SimulatorKey> {
        let modifiers = event.modifiers();
        let key = event.key().to_string();
        // Cmd+Shift is the escape hatch for buttons that are not on the screen; there is nowhere
        // to put a button for them without inventing a toolbar.
        if modifiers.meta() && modifiers.shift() {
            return match key.to_ascii_lowercase().as_str() {
                "h" => Some(SimulatorKey::Button(HardwareButton::Home)),
                "l" => Some(SimulatorKey::Button(HardwareButton::Lock)),
                _ => None,
            };
        }
        // Anything else held with a host modifier is a vmux shortcut, not device input.
        if modifiers.meta() || modifiers.ctrl() || modifiers.alt() {
            return None;
        }
        SimulatorKey::of_browser_key(&key)
    }
}

#[component]
fn Waiting(route: Option<SimulatorRoute>) -> Element {
    let label = match route {
        Some(SimulatorRoute::Pinned(version)) => format!("iOS {version}"),
        _ => String::new(),
    };
    rsx! {
        div { class: "text-sm text-muted-foreground", "{label}" }
    }
}

/// Where a pointer event landed, as a fraction of the element it hit.
struct Pointer;

impl Pointer {
    fn fraction(event: &Event<MouseData>) -> Option<(f32, f32)> {
        let point = event.data().element_coordinates();
        let size = Self::target_size(event)?;
        if size.0 <= 0.0 || size.1 <= 0.0 {
            return None;
        }
        Some(((point.x as f32) / size.0, (point.y as f32) / size.1))
    }

    /// The rendered box of the image, which `object-contain` has already fitted to the device
    /// aspect — so the fraction needs no further correction.
    #[cfg(target_arch = "wasm32")]
    fn target_size(_event: &Event<MouseData>) -> Option<(f32, f32)> {
        use wasm_bindgen::JsCast;

        let document = web_sys::window()?.document()?;
        let element = document
            .query_selector("img[src^='http://127.0.0.1']")
            .ok()??;
        let element: web_sys::HtmlElement = element.dyn_into().ok()?;
        Some((
            element.client_width() as f32,
            element.client_height() as f32,
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn target_size(_event: &Event<MouseData>) -> Option<(f32, f32)> {
        None
    }
}

/// The page's own URL.
struct CurrentRoute;

impl CurrentRoute {
    #[cfg(target_arch = "wasm32")]
    fn read() -> Option<SimulatorRoute> {
        let pathname = web_sys::window()?.location().pathname().ok()?;
        SimulatorRoute::parse(&pathname)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn read() -> Option<SimulatorRoute> {
        None
    }

    /// Rewrites the address bar without a history entry: the bare URL is a shorthand that
    /// resolved, not a place worth going back to.
    #[cfg(target_arch = "wasm32")]
    fn replace(version: &IosVersion) {
        let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
            return;
        };
        let path = SimulatorRoute::path(version);
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn replace(_version: &IosVersion) {}
}
