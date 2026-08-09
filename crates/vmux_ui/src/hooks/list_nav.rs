//! Moving a selection through a list.
//!
//! Every surface with a highlighted row — the command bar, the manager pages, a context menu, the
//! chat page's `/` and `@` selectors — answers the same three questions: did that key move the
//! selection, did it choose a row, and is the chosen row still on screen. The answers were being
//! written out again per surface, and the key mapping in particular is a convention users expect
//! to hold everywhere, so it belongs in one place.

use dioxus::prelude::Modifiers;

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

#[cfg(test)]
#[path = "list_nav.test.rs"]
mod tests;
