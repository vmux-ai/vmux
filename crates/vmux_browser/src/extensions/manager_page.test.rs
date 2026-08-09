use super::*;

#[test]
fn web_store_injector_renders_without_page_globals() {
    let source = super::super::template::render(
        INJECTOR_JS,
        &[
            ("__VMUX_WEBSTORE_INSTALLED__", "[]".into()),
            ("__VMUX_WEBSTORE_NONCE__", "\"nonce\"".into()),
        ],
    )
    .unwrap();

    assert!(!source.contains("__VMUX_"));
    assert!(!source.contains("window.__VMUX_NONCE__"));
    assert!(!source.contains("window.__VMUX_INSTALLED__"));
}

#[test]
fn web_store_injector_reuses_nonce_for_same_extension() {
    let first = webstore_injector(None, "a".repeat(32));
    let same = webstore_injector(Some(&first), "a".repeat(32));
    let different = webstore_injector(Some(&first), "b".repeat(32));

    assert_eq!(same.nonce, first.nonce);
    assert_ne!(different.nonce, first.nonce);
}
