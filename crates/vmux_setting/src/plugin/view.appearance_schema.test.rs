use super::*;

#[test]
fn schema_exposes_appearance_mode_select() {
    let schema = build_settings_schema();
    assert!(schema.sections.iter().any(|s| s.id == "appearance"));
    let mode = schema.field("appearance.mode").expect("mode field");
    assert_eq!(mode.widget, Some(WidgetKind::Select));
    let vals: Vec<_> = mode.options.iter().map(|o| o.value.as_str()).collect();
    assert_eq!(vals, vec!["device", "light", "dark"]);
}

#[test]
fn schema_exposes_standard_and_vim_keymaps() {
    let schema = build_settings_schema();
    let keymap = schema.field("editor.keymap").expect("keymap field");
    let options = keymap
        .options
        .iter()
        .map(|option| (option.value.as_str(), option.label.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(options, vec![("standard", "Standard"), ("vim", "Vim")]);
}

#[test]
fn schema_exposes_every_bundled_language() {
    let schema = build_settings_schema();
    let language = schema.field("appearance.locale").expect("language field");
    assert_eq!(language.widget, Some(WidgetKind::Select));
    let values = language
        .options
        .iter()
        .map(|option| option.value.as_str())
        .collect::<Vec<_>>();
    assert_eq!(values.first(), Some(&"system"));
    assert_eq!(&values[1..], available_locales());
    assert_eq!(
        language
            .options
            .iter()
            .find(|option| option.value == "ja")
            .map(|option| option.label.as_str()),
        Some("日本語")
    );
}

#[test]
fn schema_uses_requested_locale() {
    let schema = build_settings_schema_for("ja");
    let appearance = schema
        .sections
        .iter()
        .find(|section| section.id == "appearance")
        .unwrap();
    assert_eq!(appearance.title, "外観");
    assert_eq!(
        schema.field("appearance.locale").unwrap().label.as_deref(),
        Some("言語")
    );
    assert_eq!(appearance.root_path, "appearance");
    assert!(appearance.synthetic_keys.is_empty());
    assert_eq!(
        schema
            .field("agent.app_providers[0].provider")
            .unwrap()
            .label
            .as_deref(),
        Some("プロバイダー")
    );
    assert_eq!(
        schema
            .field("agent.acp[0].command")
            .unwrap()
            .label
            .as_deref(),
        Some("コマンド")
    );
    assert_eq!(
        schema
            .field("spaces.personal.startup_dir")
            .unwrap()
            .label
            .as_deref(),
        Some("起動ディレクトリ")
    );
}
