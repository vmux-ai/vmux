use super::*;

#[test]
fn a_matching_token_is_admitted() {
    assert_eq!(admit("secret", "secret", true), Ok(ServerHello {}));
}

#[test]
fn a_wrong_token_is_refused() {
    assert_eq!(admit("guess", "secret", true), Err(Rejection::Unauthorized));
}

/// The kill switch outranks the secret, so flipping Remote off refuses even a correctly
/// paired phone.
#[test]
fn remote_switched_off_outranks_a_valid_token() {
    assert_eq!(
        admit("secret", "secret", false),
        Err(Rejection::RemoteDisabled)
    );
}

#[test]
fn each_rejection_carries_a_distinct_close_code() {
    let codes = [
        Rejection::Unauthorized,
        Rejection::RemoteDisabled,
        Rejection::Malformed,
    ]
    .map(|rejection| rejection.close_code().as_u32());
    let mut unique = codes.to_vec();
    unique.sort_unstable();
    unique.dedup();

    assert_eq!(unique.len(), codes.len());
}
