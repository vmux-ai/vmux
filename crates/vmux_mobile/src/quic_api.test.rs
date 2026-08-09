use super::*;

/// The mapping is what turns a close code into something a user can act on, so each one has
/// to stay distinct — collapsing two would send someone to re-pair over a version mismatch.
#[test]
fn every_close_code_maps_to_its_own_error() {
    assert!(matches!(
        QuicError::from_close_code(CloseCode::Unauthorized.as_u32() as u64),
        QuicError::Unauthorized
    ));
    assert!(matches!(
        QuicError::from_close_code(CloseCode::RemoteDisabled.as_u32() as u64),
        QuicError::RemoteDisabled
    ));
    assert!(matches!(
        QuicError::from_close_code(9999),
        QuicError::Transport(_)
    ));
}

/// A refusal reaches the user as advice, not a status code. `NoDesktop` in particular must
/// not read as broken — it clears when a window opens.
#[test]
fn a_refusal_explains_what_to_do() {
    assert_eq!(
        QuicError::Refused(SharedFailure::NoDesktop).to_string(),
        "Open the Vmux window on your Mac."
    );
    assert_eq!(
        QuicError::Unauthorized.to_string(),
        "Pairing expired. Scan the QR on your Mac again."
    );
}
