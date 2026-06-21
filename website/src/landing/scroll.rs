use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

pub fn init() {
    let Some(win) = web_sys::window() else {
        return;
    };
    let Some(doc) = win.document() else {
        return;
    };

    let reduce = win
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|m| m.matches());

    if reduce {
        reveal_all(&doc);
        for_each(&doc, "[data-scene]", |el| {
            let _ = el.set_attribute("style", "--p:1");
        });
        return;
    }

    let w = win.clone();
    let d = doc.clone();
    let update = move || {
        let vh = w
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let sy = w.scroll_y().unwrap_or(0.0);
        if let Some(root) = d.document_element() {
            let _ = root.set_attribute("style", &format!("--sy:{sy}"));
        }
        for_each(&d, ".reveal", |el| {
            if el.get_bounding_client_rect().top() < vh * 0.88 {
                add_class(el, "in");
            }
        });
        for_each_html(&d, "[data-scene]", |el| {
            let scrollable = (el.offset_height() as f64 - vh).max(1.0);
            let p = (-el.get_bounding_client_rect().top() / scrollable).clamp(0.0, 1.0);
            let _ = el.set_attribute("style", &format!("--p:{p}"));
        });
    };
    update();
    let cb = Closure::<dyn FnMut()>::new(update);
    let _ = win.add_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
    cb.forget();
}

fn reveal_all(doc: &web_sys::Document) {
    for_each(doc, ".reveal", |el| add_class(el, "in"));
}

fn for_each(doc: &web_sys::Document, sel: &str, mut f: impl FnMut(&web_sys::Element)) {
    if let Ok(list) = doc.query_selector_all(sel) {
        for i in 0..list.length() {
            if let Some(el) = list
                .item(i)
                .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
            {
                f(&el);
            }
        }
    }
}

fn for_each_html(doc: &web_sys::Document, sel: &str, mut f: impl FnMut(&web_sys::HtmlElement)) {
    if let Ok(list) = doc.query_selector_all(sel) {
        for i in 0..list.length() {
            if let Some(el) = list
                .item(i)
                .and_then(|n| n.dyn_into::<web_sys::HtmlElement>().ok())
            {
                f(&el);
            }
        }
    }
}

fn add_class(el: &web_sys::Element, c: &str) {
    let cur = el.class_name();
    if !cur.split_whitespace().any(|x| x == c) {
        el.set_class_name(&format!("{cur} {c}"));
    }
}
