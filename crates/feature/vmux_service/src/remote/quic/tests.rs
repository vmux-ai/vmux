use super::*;

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
