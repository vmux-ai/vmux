#[cfg(not(web))]
#[test]
fn debug_manifest_and_url_are_consistent() {
    assert_eq!(super::DEBUG_PAGE_MANIFEST.host, "debug");
    assert_eq!(crate::debug::DEBUG_PAGE_URL, "vmux://debug/");
}
