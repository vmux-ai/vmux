use super::*;
use bevy_cef_core::prelude::WebviewCefStateEvent;
use vmux_core::PageMetadata;

fn vmux_meta() -> PageMetadata {
    PageMetadata {
        url: "vmux://history/".into(),
        title: "History".into(),
        icon: vmux_core::PageIcon::None,
        bg_color: None,
    }
}

fn external_meta() -> PageMetadata {
    PageMetadata {
        url: "https://example.com".into(),
        title: "old".into(),
        icon: vmux_core::PageIcon::None,
        bg_color: None,
    }
}

fn ev(title: Option<&str>, favicon: Option<&str>, url: Option<&str>) -> WebviewCefStateEvent {
    WebviewCefStateEvent {
        webview: Entity::PLACEHOLDER,
        url: url.map(str::to_string),
        title: title.map(str::to_string),
        favicon_url: favicon.map(str::to_string),
    }
}

#[test]
fn vmux_url_preserves_title_against_cef_update() {
    let mut meta = vmux_meta();
    apply_cef_state_to_meta(&mut meta, ev(Some("vmux history POC"), None, None));
    assert_eq!(meta.title, "History");
}

#[test]
fn vmux_agent_url_accepts_dynamic_title_only() {
    let mut meta = PageMetadata {
        url: "vmux://agent/codex".into(),
        title: "Codex".into(),
        icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Sparkles),
        bg_color: None,
    };
    apply_cef_state_to_meta(
        &mut meta,
        ev(
            Some("● Codex"),
            Some("https://example.com/favicon.ico"),
            None,
        ),
    );
    assert_eq!(meta.title, "● Codex");
    assert_eq!(meta.url, "vmux://agent/codex");
    assert_eq!(
        meta.icon,
        vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Sparkles)
    );
}

#[test]
fn vmux_url_preserves_favicon_against_cef_update() {
    let mut meta = vmux_meta();
    apply_cef_state_to_meta(&mut meta, ev(None, Some("https://x/fav.ico"), None));
    assert_eq!(meta.icon, vmux_core::PageIcon::None);
}

#[test]
fn vmux_url_preserves_url_when_cef_reports_same_vmux_url() {
    let mut meta = vmux_meta();
    apply_cef_state_to_meta(&mut meta, ev(None, None, Some("vmux://history/")));
    assert_eq!(meta.url, "vmux://history/");
    assert_eq!(meta.title, "History");
}

#[test]
fn vmux_url_updates_when_cef_navigates_to_external_url() {
    let mut meta = vmux_meta();
    apply_cef_state_to_meta(&mut meta, ev(None, None, Some("https://anthropic.com")));
    assert_eq!(meta.url, "https://anthropic.com");
}

#[test]
fn after_navigation_away_subsequent_title_updates_apply() {
    let mut meta = vmux_meta();
    apply_cef_state_to_meta(&mut meta, ev(None, None, Some("https://anthropic.com")));
    apply_cef_state_to_meta(&mut meta, ev(Some("Frontier AI"), None, None));
    assert_eq!(meta.title, "Frontier AI");
}

#[test]
fn external_url_accepts_title_update() {
    let mut meta = external_meta();
    apply_cef_state_to_meta(&mut meta, ev(Some("New Title"), None, None));
    assert_eq!(meta.title, "New Title");
}

#[test]
fn external_url_accepts_favicon_update() {
    let mut meta = external_meta();
    apply_cef_state_to_meta(&mut meta, ev(None, Some("https://x/fav.ico"), None));
    assert_eq!(
        meta.icon,
        vmux_core::PageIcon::Favicon("https://x/fav.ico".into())
    );
}

#[test]
fn external_url_url_change_clears_favicon() {
    let mut meta = PageMetadata {
        url: "https://example.com".into(),
        title: "Old".into(),
        icon: vmux_core::PageIcon::Favicon("https://example.com/fav.ico".into()),
        bg_color: None,
    };
    apply_cef_state_to_meta(&mut meta, ev(None, None, Some("https://other.com")));
    assert_eq!(meta.url, "https://other.com");
    assert_eq!(meta.icon, vmux_core::PageIcon::None);
}
