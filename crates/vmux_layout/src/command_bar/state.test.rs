use super::*;

#[test]
fn missing_keyboard_target_is_closed() {
    assert_eq!(
        CommandBarState::from_modal(Display::Flex, Visibility::Inherited, false),
        CommandBarState::Closed
    );
}

#[test]
fn display_none_is_closed_even_with_keyboard_target() {
    assert_eq!(
        CommandBarState::from_modal(Display::None, Visibility::Inherited, true),
        CommandBarState::Closed
    );
}

#[test]
fn hidden_surface_with_keyboard_target_still_owns_input() {
    let state = CommandBarState::from_modal(Display::Flex, Visibility::Hidden, true);

    assert_eq!(state, CommandBarState::Revealing);
    assert!(state.owns_input());
    assert!(!state.is_shown());
}

#[test]
fn visible_surface_with_keyboard_target_is_shown() {
    let state = CommandBarState::from_modal(Display::Flex, Visibility::Inherited, true);

    assert_eq!(state, CommandBarState::Shown);
    assert!(state.owns_input());
    assert!(state.is_shown());
}

#[test]
fn closed_owns_nothing() {
    assert!(!CommandBarState::Closed.owns_input());
    assert!(!CommandBarState::Closed.is_shown());
    assert!(!CommandBarState::Closed.is_revealing());
}
