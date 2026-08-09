use super::{error_page_source, percent_encode};

#[test]
fn percent_encode_escapes_reserved_keeps_unreserved() {
    assert_eq!(percent_encode("a b/&"), "a%20b%2F%26");
    assert_eq!(percent_encode("v0.0.1-rc~_"), "v0.0.1-rc~_");
}

#[test]
fn error_page_source_builds_query() {
    assert_eq!(
        error_page_source("Page not found", "", "vmux://debug/"),
        "vmux://error/?title=Page%20not%20found&message=&url=vmux%3A%2F%2Fdebug%2F"
    );
}
