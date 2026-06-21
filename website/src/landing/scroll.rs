use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::HtmlElement;

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
        for_each(&doc, ".reveal", |el| add_class(el, "in"));
        for_each_html(&doc, "[data-scene]", |el| set_var(el, "--p", "0.66"));
        return;
    }

    let root = doc
        .document_element()
        .and_then(|e| e.dyn_into::<HtmlElement>().ok());

    {
        let w = win.clone();
        let d = doc.clone();
        let root = root.clone();
        let update = move || {
            let vh = w
                .inner_height()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let sy = w.scroll_y().unwrap_or(0.0);
            if let Some(r) = &root {
                set_var(r, "--sy", &sy.to_string());
            }
            for_each(&d, ".reveal", |el| {
                if el.get_bounding_client_rect().top() < vh * 0.88 {
                    add_class(el, "in");
                }
            });
            for_each_html(&d, "[data-scene]", |el| {
                let scrollable = (el.offset_height() as f64 - vh).max(1.0);
                let p = (-el.get_bounding_client_rect().top() / scrollable).clamp(0.0, 1.0);
                set_var(el, "--p", &p.to_string());
            });
        };
        update();
        let cb = Closure::<dyn FnMut()>::new(update);
        let _ = win.add_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    {
        let d = doc.clone();
        let root = root.clone();
        let on_move = move |e: web_sys::MouseEvent| {
            let (Some(r), Some(tilt)) = (&root, d.query_selector("[data-tilt]").ok().flatten())
            else {
                return;
            };
            let rect = tilt.get_bounding_client_rect();
            let hw = (rect.width() / 2.0).max(1.0);
            let hh = (rect.height() / 2.0).max(1.0);
            let ry = ((e.client_x() as f64 - (rect.left() + hw)) / hw).clamp(-1.0, 1.0);
            let rx = ((e.client_y() as f64 - (rect.top() + hh)) / hh).clamp(-1.0, 1.0);
            set_var(r, "--ry", &ry.to_string());
            set_var(r, "--rx", &rx.to_string());
        };
        let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(on_move);
        let _ = win.add_event_listener_with_callback("mousemove", cb.as_ref().unchecked_ref());
        cb.forget();
    }
}

fn set_var(el: &HtmlElement, name: &str, val: &str) {
    let _ = el.style().set_property(name, val);
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

fn for_each_html(doc: &web_sys::Document, sel: &str, mut f: impl FnMut(&HtmlElement)) {
    if let Ok(list) = doc.query_selector_all(sel) {
        for i in 0..list.length() {
            if let Some(el) = list.item(i).and_then(|n| n.dyn_into::<HtmlElement>().ok()) {
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
