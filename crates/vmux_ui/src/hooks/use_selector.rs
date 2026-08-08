//! Keeping the selected row on screen.

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
        scroll_item_into_view(&item_id(selected()));
    });
}

#[cfg(web)]
fn scroll_item_into_view(item_id: &str) {
    let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(item_id))
    else {
        return;
    };
    let options = web_sys::ScrollIntoViewOptions::new();
    options.set_block(web_sys::ScrollLogicalPosition::Nearest);
    element.scroll_into_view_with_scroll_into_view_options(&options);
}

#[cfg(not(web))]
fn scroll_item_into_view(_item_id: &str) {}
