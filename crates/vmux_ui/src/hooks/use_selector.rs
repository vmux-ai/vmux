//! Keeping the selected row on screen.

use std::cell::Cell;
use std::rc::Rc;

use crate::host::Host;
use dioxus::prelude::*;

/// Keep the selected row on screen as the selection moves.
///
/// `item_id` names the element for the current index, and naming one nothing answers to — the
/// empty string will do — is how a caller says "not now". Scrolling needs the DOM, so off CEF this
/// does nothing: the affordance follows keyboard navigation, which a touch host does not have.
///
/// Only a *move* scrolls. Mounting is not a move: a list that has just appeared is already where
/// the page put it, and scrolling it then would fight that. What counts as a move is whatever
/// `item_id` reads, since those are the signals this reacts to.
///
/// A row in such a list owes two things, and forgetting either is invisible until someone tries
/// it: the `id` this returns, or the scroll has nothing to find; and
/// `onmouseenter: move |_| selected.set(index)`, or the pointer and the arrow keys disagree about
/// which row is highlighted.
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
