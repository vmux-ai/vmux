use super::*;

#[test]
fn extracts_id_from_new_store_url() {
    let id = extension_id(
        "https://chromewebstore.google.com/detail/ublock-origin/cjpalhdlnbpafiamejdnhcphjbkeiagm",
    )
    .unwrap();
    assert_eq!(id, "cjpalhdlnbpafiamejdnhcphjbkeiagm");
}

#[test]
fn extracts_id_from_legacy_url() {
    let id = extension_id(
        "https://chrome.google.com/webstore/detail/foo/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
    .unwrap();
    assert_eq!(id, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
}

#[test]
fn accepts_bare_id() {
    let id = extension_id("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
    assert_eq!(id, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
}

#[test]
fn rejects_junk() {
    assert!(extension_id("not an extension").is_none());
    assert!(extension_id("https://example.com").is_none());
}

#[test]
fn rejects_wrong_length_or_chars() {
    assert!(extension_id("zzzz").is_none());
    assert!(extension_id("qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq").is_none());
}

#[test]
fn builds_crx_url() {
    let url = crx_url("cjpalhdlnbpafiamejdnhcphjbkeiagm", "120.0.0.0");
    assert!(url.contains("id%3Dcjpalhdlnbpafiamejdnhcphjbkeiagm"));
    assert!(url.contains("prodversion=120.0.0.0"));
    assert!(url.contains("acceptformat=crx2,crx3"));
}
