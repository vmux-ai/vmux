use super::*;

#[test]
fn parses_mv3_action() {
    let m = parse(
            r#"{
            "manifest_version": 3,
            "name": "uBlock", "version": "1.6",
            "action": { "default_popup": "popup.html", "default_icon": { "16": "i16.png", "32": "i32.png" } }
        }"#,
        )
        .unwrap();
    assert_eq!(m.name, "uBlock");
    assert_eq!(m.version, "1.6");
    assert_eq!(m.popup.as_deref(), Some("popup.html"));
    assert_eq!(m.icon.as_deref(), Some("i32.png"));
}

#[test]
fn rejects_unsafe_version_and_resource_paths() {
    for manifest in [
        r#"{"manifest_version":3,"name":"x","version":"../1"}"#,
        r#"{"manifest_version":3,"name":"x","version":"01"}"#,
        r#"{"manifest_version":3,"name":"x","version":"1.2.3.4.5"}"#,
        r#"{"manifest_version":3,"name":"x","version":"1","action":{"default_icon":"../secret"}}"#,
        r#"{"manifest_version":3,"name":"x","version":"1","action":{"default_popup":"/tmp/page.html"}}"#,
    ] {
        assert!(parse(manifest).is_err(), "accepted {manifest}");
    }
}

#[test]
fn parses_api_and_host_permissions() {
    let m = parse(
        r#"{
                "name": "x",
                "version": "1",
                "manifest_version": 3,
                "permissions": ["storage"],
                "optional_permissions": ["history"],
                "host_permissions": ["https://example.com/*"],
                "optional_host_permissions": ["https://optional.example/*"]
            }"#,
    )
    .unwrap();

    assert_eq!(m.permissions, ["storage"]);
    assert_eq!(m.optional_permissions, ["history"]);
    assert_eq!(m.host_permissions, ["https://example.com/*"]);
    assert_eq!(m.optional_host_permissions, ["https://optional.example/*"]);
}

#[test]
fn parses_mv2_host_permissions_from_permissions() {
    let m = parse(
        r#"{
                "manifest_version": 2,
                "name": "x",
                "version": "1",
                "permissions": ["storage", "https://legacy.example/*"],
                "optional_permissions": ["history", "https://legacy-optional.example/*"]
            }"#,
    )
    .unwrap();

    assert_eq!(m.permissions, ["storage"]);
    assert_eq!(m.optional_permissions, ["history"]);
    assert_eq!(m.host_permissions, ["https://legacy.example/*"]);
    assert_eq!(
        m.optional_host_permissions,
        ["https://legacy-optional.example/*"]
    );
}

#[test]
fn parses_mv2_browser_action_and_string_icon() {
    let m = parse(
        r#"{
            "manifest_version": 2,
            "name": "x", "version": "2",
            "browser_action": { "default_popup": "p.html", "default_icon": "icon.png" }
        }"#,
    )
    .unwrap();
    assert_eq!(m.popup.as_deref(), Some("p.html"));
    assert_eq!(m.icon.as_deref(), Some("icon.png"));
}

#[test]
fn no_action_means_no_icon() {
    let m = parse(
        r#"{ "manifest_version": 3, "name": "bg", "version": "1", "icons": { "48": "x.png" } }"#,
    )
    .unwrap();
    assert!(m.popup.is_none());
    assert!(m.icon.is_none());
}

#[test]
fn picks_largest_within_48() {
    let m = parse(
            r#"{ "manifest_version": 3, "name": "x", "version": "1", "action": { "default_icon": { "16": "a.png", "48": "b.png", "128": "c.png" } } }"#,
        )
        .unwrap();
    assert_eq!(m.icon.as_deref(), Some("b.png"));
}

#[test]
fn falls_back_to_largest_above_48() {
    let m = parse(
            r#"{ "manifest_version": 3, "name": "x", "version": "1", "action": { "default_icon": { "64": "a.png", "128": "b.png" } } }"#,
        )
        .unwrap();
    assert_eq!(m.icon.as_deref(), Some("b.png"));
}
