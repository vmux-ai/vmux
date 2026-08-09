use super::*;

#[test]
fn pending_key_window_releases_keys_when_winit_becomes_responder() {
    let mut pending = false;

    assert!(!should_release_keys(
        ReclaimOutcome::PendingKeyWindow,
        &mut pending
    ));
    assert!(pending);
    assert!(should_release_keys(
        ReclaimOutcome::AlreadyWinit,
        &mut pending
    ));
    assert!(!pending);
}
