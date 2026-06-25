use std::cell::RefCell;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Document, KeyboardEvent};

use crate::keymap::{Action, MatchResult, Matcher};
use crate::mode::{Mode, resolve_mode};
use crate::scroll::{ScrollKind, scroll_delta};

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

fn dispatch(action: Action, doc: &Document) {
    let win = web_sys::window().unwrap();
    let vh = win
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0);
    match action {
        Action::EnterInsert => FORCE_INSERT.with(|f| *f.borrow_mut() = true),
        Action::ScrollDownLine => scroll_by(&win, scroll_delta(ScrollKind::Line, true, vh)),
        Action::ScrollUpLine => scroll_by(&win, scroll_delta(ScrollKind::Line, false, vh)),
        Action::ScrollDownHalf => scroll_by(&win, scroll_delta(ScrollKind::Half, true, vh)),
        Action::ScrollUpHalf => scroll_by(&win, scroll_delta(ScrollKind::Half, false, vh)),
        Action::ScrollTop => win.scroll_to_with_x_and_y(0.0, 0.0),
        Action::ScrollBottom => {
            let h = doc
                .document_element()
                .map(|e| e.scroll_height() as f64)
                .unwrap_or(0.0);
            win.scroll_to_with_x_and_y(0.0, h);
        }
        Action::HistoryBack => {
            let _ = win.history().and_then(|h| h.back());
        }
        Action::HistoryForward => {
            let _ = win.history().and_then(|h| h.forward());
        }
        Action::Reload => {
            let _ = win.location().reload();
        }
        other => web_sys::console::log_1(&format!("[vmux-vimium] {other:?}").into()),
    }
}

fn scroll_by(win: &web_sys::Window, dy: f64) {
    win.scroll_by_with_x_and_y(0.0, dy);
}
