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
