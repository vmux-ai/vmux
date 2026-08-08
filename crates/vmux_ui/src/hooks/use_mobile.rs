//! Whether the viewport is narrow enough to treat as mobile.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

const MOBILE_BREAKPOINT: u32 = 768;

pub fn use_mobile() -> Signal<bool> {
    let mut is_mobile = use_signal(|| false);

    use_effect(move || {
        let check = || -> bool {
            web_sys::window()
                .and_then(|w| w.inner_width().ok())
                .and_then(|v| v.as_f64())
                .map(|w| (w as u32) < MOBILE_BREAKPOINT)
                .unwrap_or(false)
        };

        is_mobile.set(check());

        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |_: web_sys::Event| {
            is_mobile.set(
                web_sys::window()
                    .and_then(|w| w.inner_width().ok())
                    .and_then(|v| v.as_f64())
                    .map(|w| (w as u32) < MOBILE_BREAKPOINT)
                    .unwrap_or(false),
            );
        })
            as Box<dyn FnMut(web_sys::Event)>);

        if let Some(win) = web_sys::window() {
            let _ =
                win.add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref());
        }
        closure.forget();
    });

    is_mobile
}
