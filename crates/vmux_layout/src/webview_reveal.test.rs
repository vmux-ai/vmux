use super::*;

#[test]
fn vmux_ui_webviews_reveal_after_frame_delay_even_without_page_ready() {
    assert!(!webview_reveal_ready(
        &WebviewSource::new("vmux://header/"),
        false,
        REVEAL_FRAMES - 1
    ));
    assert!(webview_reveal_ready(
        &WebviewSource::new("vmux://header/"),
        false,
        REVEAL_FRAMES
    ));
}

#[test]
fn tab_content_reveal_still_uses_frame_delay_only() {
    assert!(!webview_reveal_ready(
        &WebviewSource::new("https://example.com/"),
        false,
        REVEAL_FRAMES - 1
    ));
    assert!(webview_reveal_ready(
        &WebviewSource::new("https://example.com/"),
        false,
        REVEAL_FRAMES
    ));
}

#[test]
fn unknown_vmux_urls_are_treated_as_content() {
    assert!(webview_reveal_ready(
        &WebviewSource::new("vmux://unknown/"),
        false,
        REVEAL_FRAMES
    ));
}
