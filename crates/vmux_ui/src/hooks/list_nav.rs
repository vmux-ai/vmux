//! Moving a selection through a list.
//!
//! Every surface with a highlighted row — the command bar, the manager pages, a context menu, the
//! chat page's `/` and `@` selectors — answers the same three questions: did that key move the
//! selection, did it choose a row, and is the chosen row still on screen. The answers were being
//! written out again per surface, and the key mapping in particular is a convention users expect
//! to hold everywhere, so it belongs in one place.

use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuDirection {
    Next,
    Previous,
}

/// What a keystroke means to an open list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKey {
    Move(MenuDirection),
    /// Enter on the current row, or a number key naming one.
    Choose(Option<usize>),
    Dismiss,
}

/// Arrow keys, or the emacs bindings people reach for without thinking.
pub fn menu_direction(key: &str, ctrl: bool) -> Option<MenuDirection> {
    match key {
        "ArrowDown" if !ctrl => Some(MenuDirection::Next),
        "ArrowUp" if !ctrl => Some(MenuDirection::Previous),
        "n" | "N" if ctrl => Some(MenuDirection::Next),
        "p" | "P" if ctrl => Some(MenuDirection::Previous),
        _ => None,
    }
}

/// A 1-based number key naming one of `len` rows.
pub fn choice_number_index(key: &str, len: usize) -> Option<usize> {
    let number = key.parse::<usize>().ok()?;
    if number == 0 || number > len {
        None
    } else {
        Some(number - 1)
    }
}

/// Wraps at both ends, so holding Down cycles rather than sticking at the bottom.
pub fn move_selection(current: usize, len: usize, direction: MenuDirection) -> usize {
    if len == 0 {
        return 0;
    }
    match direction {
        MenuDirection::Next => (current + 1) % len,
        MenuDirection::Previous => (current + len - 1) % len,
    }
}

/// Read a keystroke as list navigation, or `None` if the list should not claim it.
///
/// Ctrl is meaningful — it carries the emacs bindings — so only Meta and Alt disqualify a key
/// outright. The caller still decides whether a list is open at all.
pub fn list_key(key: &str, modifiers: Modifiers, len: usize) -> Option<ListKey> {
    if modifiers.meta() || modifiers.alt() {
        return None;
    }
    if let Some(direction) = menu_direction(key, modifiers.ctrl()) {
        return Some(ListKey::Move(direction));
    }
    if modifiers.ctrl() {
        return None;
    }
    match key {
        "Enter" => Some(ListKey::Choose(None)),
        "Escape" => Some(ListKey::Dismiss),
        _ => choice_number_index(key, len).map(|index| ListKey::Choose(Some(index))),
    }
}

/// Keep the selected row on screen as the selection moves.
///
/// `item_id` names the element for the current index. Scrolling needs the DOM, so off CEF this
/// does nothing — the affordance follows keyboard navigation, which a touch host does not have.
///
/// A row in such a list owes two things, and forgetting either is invisible until someone tries
/// it: the `id` this returns, or the scroll has nothing to find; and
/// `onmouseenter: move |_| selected.set(index)`, or the pointer and the arrow keys disagree about
/// which row is highlighted.
pub fn use_selection_visible(selected: Signal<usize>, item_id: impl Fn(usize) -> String + 'static) {
    use_effect(move || {
        scroll_item_into_view(&item_id(selected()));
    });
}

#[cfg(target_arch = "wasm32")]
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

#[cfg(not(target_arch = "wasm32"))]
fn scroll_item_into_view(_item_id: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_wraps_at_both_ends() {
        assert_eq!(move_selection(2, 3, MenuDirection::Next), 0);
        assert_eq!(move_selection(0, 3, MenuDirection::Previous), 2);
        assert_eq!(move_selection(0, 0, MenuDirection::Next), 0);
    }

    /// Ctrl carries the emacs bindings, so it must not disqualify a key the way Meta and Alt do.
    #[test]
    fn ctrl_navigates_where_meta_and_alt_decline() {
        let ctrl = Modifiers::CONTROL;
        assert_eq!(
            list_key("n", ctrl, 3),
            Some(ListKey::Move(MenuDirection::Next))
        );
        assert_eq!(list_key("ArrowDown", Modifiers::META, 3), None);
        assert_eq!(list_key("ArrowDown", Modifiers::ALT, 3), None);
        // Ctrl still declines everything that is not a binding.
        assert_eq!(list_key("Enter", ctrl, 3), None);
    }

    #[test]
    fn a_number_key_names_a_row_only_when_the_row_exists() {
        let none = Modifiers::empty();
        assert_eq!(list_key("2", none, 3), Some(ListKey::Choose(Some(1))));
        assert_eq!(list_key("4", none, 3), None);
        assert_eq!(list_key("0", none, 3), None);
        assert_eq!(list_key("Enter", none, 3), Some(ListKey::Choose(None)));
        assert_eq!(list_key("Escape", none, 3), Some(ListKey::Dismiss));
    }
}
