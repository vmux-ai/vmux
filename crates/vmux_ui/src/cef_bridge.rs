//! Shared `window.cef` listen / emit helpers for Dioxus WASM UIs in CEF.

use js_sys::Function;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::window;

/// Register `cef.listen(channel, …)` when `window.cef` is ready. Returns `false` while CEF is still booting.
pub fn try_cef_listen<F>(channel: &str, on_event: F) -> bool
where
    F: FnMut(JsValue) + 'static,
{
    let Some(win) = window() else {
        return false;
    };
    let Ok(cef) = js_sys::Reflect::get(&win, &JsValue::from_str("cef")) else {
        return false;
    };
    if cef.is_null() || cef.is_undefined() {
        return false;
    }
    let Ok(listen) = js_sys::Reflect::get(&cef, &JsValue::from_str("listen")) else {
        return false;
    };
    let Ok(listen_fn) = listen.dyn_into::<Function>() else {
        return false;
    };

    let mut on_event = on_event;
    let closure = Closure::wrap(Box::new(move |e: JsValue| {
        on_event(e);
    }) as Box<dyn FnMut(JsValue)>);

    let cb = closure.as_ref().unchecked_ref();
    let _ = listen_fn.call2(&cef, &JsValue::from_str(channel), cb);
    closure.forget();
    true
}

/// Call `cef.emit` with a plain object built from string keys and [`JsValue`] values.
pub fn try_cef_emit_keyed(pairs: &[(&str, JsValue)]) -> bool {
    let Some(win) = window() else {
        return false;
    };
    let Ok(cef) = js_sys::Reflect::get(&win, &JsValue::from_str("cef")) else {
        return false;
    };
    if cef.is_null() || cef.is_undefined() {
        return false;
    }
    let Ok(emit) = js_sys::Reflect::get(&cef, &JsValue::from_str("emit")) else {
        return false;
    };
    let Ok(emit_fn) = emit.dyn_into::<Function>() else {
        return false;
    };
    let obj = js_sys::Object::new();
    for (k, v) in pairs {
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(k), v);
    }
    let obj: JsValue = obj.into();
    let _ = emit_fn.call1(&cef, &obj);
    true
}
