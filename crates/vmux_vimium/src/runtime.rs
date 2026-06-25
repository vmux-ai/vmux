use std::cell::RefCell;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Document, KeyboardEvent};

use crate::keymap::{Action, MatchResult, Matcher};
use crate::mode::{Mode, resolve_mode};

thread_local! {
    static MATCHER: RefCell<Matcher> = RefCell::new(Matcher::new());
    static FORCE_INSERT: RefCell<bool> = const { RefCell::new(false) };
}

fn document() -> Document {
    web_sys::window().unwrap().document().unwrap()
}

fn focused_tag_and_editable(doc: &Document) -> (Option<String>, bool) {
    match doc.active_element() {
        Some(el) => {
            let tag = el.tag_name();
            let editable = el
                .dyn_ref::<web_sys::HtmlElement>()
                .map(|h| h.is_content_editable())
                .unwrap_or(false);
            (Some(tag), editable)
        }
        None => (None, false),
    }
}

fn current_mode(doc: &Document) -> Mode {
    let (tag, editable) = focused_tag_and_editable(doc);
    let force_insert = FORCE_INSERT.with(|f| *f.borrow());
    resolve_mode(tag.as_deref(), editable, force_insert, false)
}

pub fn install() {
    let doc = document();
    let handler = Closure::<dyn FnMut(KeyboardEvent)>::new(move |ev: KeyboardEvent| {
        on_keydown(ev);
    });
    doc.add_event_listener_with_callback_and_bool("keydown", handler.as_ref().unchecked_ref(), true)
        .unwrap();
    handler.forget();
}

fn on_keydown(ev: KeyboardEvent) {
    if ev.ctrl_key() || ev.meta_key() || ev.alt_key() {
        return;
    }
    let doc = document();
    let key = ev.key();

    if current_mode(&doc) == Mode::Insert {
        if key == "Escape" {
            FORCE_INSERT.with(|f| *f.borrow_mut() = false);
            if let Some(el) = doc.active_element() {
                if let Some(h) = el.dyn_ref::<web_sys::HtmlElement>() {
                    let _ = h.blur();
                }
            }
        }
        return;
    }

    let result = MATCHER.with(|m| m.borrow_mut().feed(&key));
    match result {
        MatchResult::Action(action) => {
            ev.prevent_default();
            ev.stop_propagation();
            dispatch(action, &doc);
        }
        MatchResult::Pending => {
            ev.prevent_default();
            ev.stop_propagation();
        }
        MatchResult::None => {}
    }
}

fn dispatch(action: Action, _doc: &Document) {
    match action {
        Action::EnterInsert => FORCE_INSERT.with(|f| *f.borrow_mut() = true),
        other => web_sys::console::log_1(&format!("[vmux-vimium] {other:?}").into()),
    }
}
