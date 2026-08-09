use super::is_debug_url;

#[test]
fn matches_with_and_without_trailing_slash() {
    assert!(is_debug_url("vmux://debug/"));
    assert!(is_debug_url("vmux://debug"));
}

#[test]
fn rejects_other_hosts() {
    assert!(!is_debug_url("vmux://debugger"));
    assert!(!is_debug_url("vmux://spaces/"));
    assert!(!is_debug_url("vmux://debug/extra"));
}
