use super::*;

#[test]
fn for_shell_maps_known_shells() {
    assert_eq!(
        BuiltinIcon::for_shell("/opt/homebrew/bin/nu"),
        Some(BuiltinIcon::Nushell)
    );
    assert_eq!(BuiltinIcon::for_shell("/bin/bash"), Some(BuiltinIcon::Bash));
    assert_eq!(BuiltinIcon::for_shell("/bin/zsh"), Some(BuiltinIcon::Zsh));
    assert_eq!(BuiltinIcon::for_shell("nu"), Some(BuiltinIcon::Nushell));
    assert_eq!(BuiltinIcon::for_shell("/usr/bin/fish"), None);
}

#[test]
fn favicon_constructor_collapses_empty_to_none() {
    assert_eq!(PageIcon::favicon(""), PageIcon::None);
    assert_eq!(
        PageIcon::favicon("https://x/fav.ico"),
        PageIcon::Favicon("https://x/fav.ico".to_string())
    );
}

#[test]
fn accessors() {
    assert_eq!(PageIcon::Favicon("u".into()).favicon_url(), "u");
    assert_eq!(PageIcon::Builtin(BuiltinIcon::Users).favicon_url(), "");
    assert_eq!(
        PageIcon::Builtin(BuiltinIcon::Users).builtin(),
        Some(BuiltinIcon::Users)
    );
    assert!(PageIcon::None.is_none());
    assert_eq!(PageIcon::default(), PageIcon::None);
}

#[test]
fn persisted_tool_and_vault_icons_deserialize() {
    for (json, expected) in [
        (r#"{"Builtin":"Hammer"}"#, BuiltinIcon::Hammer),
        (r#"{"Builtin":"Vault"}"#, BuiltinIcon::Vault),
    ] {
        assert_eq!(
            serde_json::from_str::<PageIcon>(json).unwrap(),
            PageIcon::Builtin(expected)
        );
    }
}
