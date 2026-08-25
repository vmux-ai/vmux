use std::cell::Cell;
use std::rc::Rc;

use crate::transport::Host;
use dioxus::prelude::*;

pub fn use_selector(selected: Signal<usize>, item_id: impl Fn(usize) -> String + 'static) {
    let mounted = use_hook(|| Rc::new(Cell::new(false)));
    use_effect(move || {
        let id = item_id(selected());
        if !mounted.replace(true) {
            return;
        }
        Host::scroll_item_into_view(&id);
    });
}
