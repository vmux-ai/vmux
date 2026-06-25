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
    static HINTS: RefCell<Option<crate::hints::Hints>> = const { RefCell::new(None) };
    static FIND: RefCell<Option<crate::find::Find>> = const { RefCell::new(None) };
    static OPENBAR: RefCell<Option<crate::openbar::OpenBar>> = const { RefCell::new(None) };
    static PENDING_AT: RefCell<f64> = const { RefCell::new(0.0) };
}

const PENDING_TIMEOUT_MS: f64 = 1000.0;

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
    doc.add_event_listener_with_callback_and_bool(
        "keydown",
        handler.as_ref().unchecked_ref(),
        true,
    )
    .unwrap();
    handler.forget();
}

fn on_keydown(ev: KeyboardEvent) {
    if ev.ctrl_key() || ev.meta_key() || ev.alt_key() {
        return;
    }
    let doc = document();
    let key = ev.key();

    let hints_open = HINTS.with(|h| h.borrow().is_some());
    if hints_open {
        ev.prevent_default();
        ev.stop_propagation();
        if key == "Escape" {
            HINTS.with(|h| {
                if let Some(hl) = h.borrow().as_ref() {
                    hl.cancel(&doc);
                }
                *h.borrow_mut() = None;
            });
            return;
        }
        if key.chars().count() == 1 {
            let keep = HINTS.with(|h| h.borrow_mut().as_mut().unwrap().feed(&doc, &key));
            if !keep {
                HINTS.with(|h| *h.borrow_mut() = None);
            }
        }
        return;
    }

    let find_input_open =
        FIND.with(|f| f.borrow().as_ref().map(|x| x.input_open()).unwrap_or(false));
    if find_input_open {
        match key.as_str() {
            "Escape" => {
                ev.prevent_default();
                ev.stop_propagation();
                FIND.with(|f| {
                    if let Some(fd) = f.borrow().as_ref() {
                        fd.close(&doc);
                    }
                    *f.borrow_mut() = None;
                });
            }
            "Enter" => {
                ev.prevent_default();
                FIND.with(|f| {
                    if let Some(fd) = f.borrow_mut().as_mut() {
                        fd.search(&doc);
                    }
                });
            }
            _ => {}
        }
        return;
    }

    let open_active = OPENBAR.with(|o| o.borrow().is_some());
    if open_active {
        match key.as_str() {
            "Escape" => {
                ev.prevent_default();
                ev.stop_propagation();
                OPENBAR.with(|o| {
                    if let Some(b) = o.borrow().as_ref() {
                        b.close(&doc);
                    }
                    *o.borrow_mut() = None;
                });
            }
            "Enter" => {
                ev.prevent_default();
                OPENBAR.with(|o| {
                    if let Some(b) = o.borrow().as_ref() {
                        b.submit(&doc);
                    }
                    *o.borrow_mut() = None;
                });
            }
            _ => {}
        }
        return;
    }

    if current_mode(&doc) == Mode::Insert {
        if key == "Escape" {
            FORCE_INSERT.with(|f| *f.borrow_mut() = false);
            if let Some(el) = doc.active_element()
                && let Some(h) = el.dyn_ref::<web_sys::HtmlElement>()
            {
                let _ = h.blur();
            }
        }
        return;
    }

    let now = js_sys::Date::now();
    let stale = MATCHER.with(|m| m.borrow().has_pending())
        && PENDING_AT.with(|t| now - *t.borrow() > PENDING_TIMEOUT_MS);
    if stale {
        MATCHER.with(|m| m.borrow_mut().clear_pending());
    }
    let result = MATCHER.with(|m| m.borrow_mut().feed(&key));
    match result {
        MatchResult::Action(action) => {
            ev.prevent_default();
            ev.stop_propagation();
            dispatch(action, &doc);
        }
        MatchResult::Pending => {
            PENDING_AT.with(|t| *t.borrow_mut() = now);
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
        Action::Hints => {
            if let Some(h) = crate::hints::Hints::show(doc) {
                HINTS.with(|c| *c.borrow_mut() = Some(h));
            }
        }
        Action::OpenFind => {
            FIND.with(|f| {
                if let Some(fd) = f.borrow().as_ref() {
                    fd.close(doc);
                }
                *f.borrow_mut() = Some(crate::find::Find::open(doc));
            });
        }
        Action::FindNext => FIND.with(|f| {
            if let Some(fd) = f.borrow_mut().as_mut() {
                fd.next(doc, true);
            }
        }),
        Action::FindPrev => FIND.with(|f| {
            if let Some(fd) = f.borrow_mut().as_mut() {
                fd.next(doc, false);
            }
        }),
        Action::Escape => {
            FIND.with(|f| {
                if let Some(fd) = f.borrow().as_ref() {
                    fd.close(doc);
                }
                *f.borrow_mut() = None;
            });
        }
        Action::OpenBar => {
            OPENBAR.with(|o| *o.borrow_mut() = Some(crate::openbar::OpenBar::open(doc)));
        }
    }
}

fn scroll_by(win: &web_sys::Window, dy: f64) {
    win.scroll_by_with_x_and_y(0.0, dy);
}
