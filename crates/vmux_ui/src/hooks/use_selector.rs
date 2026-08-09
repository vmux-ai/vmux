//! Keeping the selected row on screen.

use crate::hooks::Host;
use dioxus::prelude::*;

/// Keep the selected row on screen as the selection moves.
///
/// `item_id` names the element for the current index. Scrolling needs the DOM, so off CEF this
/// does nothing — the affordance follows keyboard navigation, which a touch host does not have.
///
/// A row in such a list owes two things, and forgetting either is invisible until someone tries
/// it: the `id` this returns, or the scroll has nothing to find; and
/// `onmouseenter: move |_| selected.set(index)`, or the pointer and the arrow keys disagree about
/// which row is highlighted.
pub fn use_selector(selected: Signal<usize>, item_id: impl Fn(usize) -> String + 'static) {
    use_effect(move || {
        Host::scroll_item_into_view(&item_id(selected()));
    });
}
