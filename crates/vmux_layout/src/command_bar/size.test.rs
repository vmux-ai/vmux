use super::*;

#[test]
fn repeated_size_is_suppressed_until_the_next_open() {
    let mut state = CommandBarSizeEmissionState::default();

    assert!(state.should_emit(1, 576, 320, 100, 80, 576, 320));
    state.mark_emitted(1, 576, 320, 100, 80, 576, 320);
    assert!(!state.should_emit(1, 576, 320, 100, 80, 576, 320));
    assert!(state.should_emit(1, 576, 400, 100, 80, 576, 400));
    assert!(state.should_emit(1, 576, 320, 110, 80, 576, 320));
    assert!(state.should_emit(2, 576, 320, 100, 80, 576, 320));
}

#[test]
fn animation_frame_requests_coalesce() {
    let mut state = CommandBarSizeEmissionState::default();

    assert!(state.schedule());
    assert!(!state.schedule());
    state.finish_schedule();
    assert!(state.schedule());
}
