use vmux_ui::i18n::Locale;

use crate::schema::{FieldSpec, SectionSpec, SettingsSchema, WidgetKind};
use crate::state::SettingsSelectOption;

impl SettingsSchema {
    pub(super) fn localized(locale: &Locale) -> Self {
        build_settings_schema_for(locale)
    }
}

#[cfg(test)]
fn build_settings_schema() -> SettingsSchema {
    SettingsSchema::localized(&Locale::from("en-US"))
}

fn build_settings_schema_for(locale: &Locale) -> SettingsSchema {
    let directory = vmux_core::profile::config_dir().join("locales");
    let tag = locale.as_str();
    if let Some(source) = [tag, tag.split('-').next().unwrap_or(tag)]
        .into_iter()
        .find_map(|tag| std::fs::read_to_string(directory.join(format!("{tag}.ftl"))).ok())
    {
        let _ = locale.register_catalog(&source);
    }
    let t = |id| locale.translate(id);
    let mut locale_options = vec![SettingsSelectOption {
        value: "system".to_string(),
        label: t("schema-system"),
    }];
    for available in Locale::available() {
        let label = available.name().to_string();
        locale_options.push(SettingsSelectOption {
            value: available.into_string(),
            label,
        });
    }
    SettingsSchema {
        sections: vec![
            SectionSpec {
                id: "general".to_string(),
                title: t("schema-general"),
                description: None,
                synthetic_keys: vec!["update_channel".to_string(), "auto_update".to_string()],
                root_path: String::new(),
            },
            SectionSpec {
                id: "appearance".to_string(),
                title: t("schema-appearance"),
                description: None,
                synthetic_keys: vec![],
                root_path: "appearance".to_string(),
            },
            SectionSpec {
                id: "layout".to_string(),
                title: t("schema-layout"),
                description: Some(t("schema-layout-detail")),
                synthetic_keys: vec![],
                root_path: "layout".to_string(),
            },
            SectionSpec {
                id: "agent".to_string(),
                title: t("schema-agent"),
                description: Some(t("schema-agent-detail")),
                synthetic_keys: vec![],
                root_path: "agent".to_string(),
            },
            SectionSpec {
                id: "shortcuts".to_string(),
                title: t("schema-shortcuts"),
                description: Some(t("schema-shortcuts-detail")),
                synthetic_keys: vec![],
                root_path: "shortcuts".to_string(),
            },
            SectionSpec {
                id: "terminal".to_string(),
                title: t("schema-terminal"),
                description: None,
                synthetic_keys: vec![],
                root_path: "terminal".to_string(),
            },
            SectionSpec {
                id: "browser".to_string(),
                title: t("schema-browser"),
                description: None,
                synthetic_keys: vec![],
                root_path: "browser".to_string(),
            },
            SectionSpec {
                id: "editor".to_string(),
                title: t("schema-editor"),
                description: None,
                synthetic_keys: vec![],
                root_path: "editor".to_string(),
            },
            SectionSpec {
                id: "recording".to_string(),
                title: t("schema-recording"),
                description: None,
                synthetic_keys: vec![],
                root_path: "recording".to_string(),
            },
            SectionSpec {
                id: "spaces".to_string(),
                title: t("spaces-title"),
                description: None,
                synthetic_keys: vec![],
                root_path: "spaces".to_string(),
            },
        ],
        fields: vec![
            field(
                "",
                FieldSpec {
                    order: vec!["update_channel".into(), "auto_update".into()],
                    ..Default::default()
                },
            ),
            field(
                "update_channel",
                FieldSpec {
                    label: Some(t("schema-update-channel")),
                    hint: Some(t("schema-update-channel-detail")),
                    widget: Some(WidgetKind::Select),
                    options: vec![
                        SettingsSelectOption {
                            value: "stable".into(),
                            label: t("schema-update-channel-stable"),
                        },
                        SettingsSelectOption {
                            value: "preview".into(),
                            label: t("schema-update-channel-preview"),
                        },
                    ],
                    ..Default::default()
                },
            ),
            field(
                "appearance",
                FieldSpec {
                    order: vec!["mode".into(), "locale".into()],
                    ..Default::default()
                },
            ),
            field(
                "appearance.mode",
                FieldSpec {
                    label: Some(t("schema-mode")),
                    hint: Some(t("schema-mode-detail")),
                    widget: Some(WidgetKind::Select),
                    options: vec![
                        SettingsSelectOption {
                            value: "device".into(),
                            label: t("schema-device"),
                        },
                        SettingsSelectOption {
                            value: "light".into(),
                            label: t("schema-light"),
                        },
                        SettingsSelectOption {
                            value: "dark".into(),
                            label: t("schema-dark"),
                        },
                    ],
                    ..Default::default()
                },
            ),
            field(
                "appearance.locale",
                FieldSpec {
                    label: Some(t("schema-language")),
                    hint: Some(t("schema-language-detail")),
                    widget: Some(WidgetKind::Select),
                    options: locale_options,
                    ..Default::default()
                },
            ),
            field(
                "auto_update",
                FieldSpec {
                    label: Some(t("schema-auto-update")),
                    hint: Some(t("schema-auto-update-detail")),
                    ..Default::default()
                },
            ),
            field(
                "browser",
                FieldSpec {
                    order: vec!["startup_url".into(), "search_engine".into()],
                    ..Default::default()
                },
            ),
            field(
                "browser.startup_url",
                FieldSpec {
                    label: Some(t("schema-startup-url")),
                    hint: Some(t("schema-startup-url-detail")),
                    placeholder: Some("https://example.com".into()),
                    ..Default::default()
                },
            ),
            field(
                "browser.search_engine",
                FieldSpec {
                    label: Some(t("schema-search-engine")),
                    hint: Some(t("schema-search-engine-detail")),
                    widget: Some(WidgetKind::Select),
                    options: vec![
                        SettingsSelectOption {
                            value: "google".into(),
                            label: "Google".into(),
                        },
                        SettingsSelectOption {
                            value: "bing".into(),
                            label: "Bing".into(),
                        },
                        SettingsSelectOption {
                            value: "duckduckgo".into(),
                            label: "DuckDuckGo".into(),
                        },
                        SettingsSelectOption {
                            value: "brave".into(),
                            label: "Brave Search".into(),
                        },
                        SettingsSelectOption {
                            value: "kagi".into(),
                            label: "Kagi".into(),
                        },
                    ],
                    ..Default::default()
                },
            ),
            field(
                "layout",
                FieldSpec {
                    order: vec![
                        "radius".into(),
                        "window".into(),
                        "pane".into(),
                        "side_sheet".into(),
                        "focus_ring".into(),
                    ],
                    ..Default::default()
                },
            ),
            labeled_field("layout.radius", t("schema-radius")),
            field(
                "layout.window",
                FieldSpec {
                    label: Some(t("schema-window")),
                    order: vec!["padding".into()],
                    ..Default::default()
                },
            ),
            labeled_field("layout.window.padding", t("schema-padding")),
            field(
                "layout.pane",
                FieldSpec {
                    label: Some(t("schema-pane")),
                    order: vec!["gap".into()],
                    ..Default::default()
                },
            ),
            labeled_field("layout.pane.gap", t("schema-gap")),
            field(
                "layout.side_sheet",
                FieldSpec {
                    label: Some(t("schema-side-sheet")),
                    order: vec!["width".into()],
                    ..Default::default()
                },
            ),
            labeled_field("layout.side_sheet.width", t("schema-width")),
            field(
                "layout.focus_ring",
                FieldSpec {
                    label: Some(t("schema-focus-ring")),
                    order: vec!["width".into(), "color".into()],
                    ..Default::default()
                },
            ),
            labeled_field("layout.focus_ring.width", t("schema-width")),
            field(
                "layout.focus_ring.color",
                FieldSpec {
                    label: Some(t("schema-color")),
                    order: vec!["r".into(), "g".into(), "b".into()],
                    ..Default::default()
                },
            ),
            labeled_field("layout.focus_ring.color.r", t("schema-red")),
            labeled_field("layout.focus_ring.color.g", t("schema-green")),
            labeled_field("layout.focus_ring.color.b", t("schema-blue")),
            field(
                "agent",
                FieldSpec {
                    order: vec![
                        "allow_run_placement_override".into(),
                        "follow_files".into(),
                        "tidy_files".into(),
                        "tidy_files_max".into(),
                        "tidy_files_auto".into(),
                        "app_providers".into(),
                        "acp".into(),
                    ],
                    ..Default::default()
                },
            ),
            field(
                "agent.allow_run_placement_override",
                FieldSpec {
                    label: Some(t("schema-run-placement")),
                    hint: Some(t("schema-run-placement-detail")),
                    ..Default::default()
                },
            ),
            labeled_field("agent.follow_files", t("schema-follow-files")),
            labeled_field("agent.tidy_files", t("schema-tidy-files")),
            labeled_field("agent.tidy_files_max", t("schema-tidy-files-max")),
            labeled_field("agent.tidy_files_auto", t("schema-tidy-files-auto")),
            labeled_field("agent.app_providers", t("schema-app-providers")),
            labeled_field("agent.app_providers[].provider", t("schema-provider")),
            labeled_field("agent.app_providers[].kind", t("schema-kind")),
            labeled_field("agent.app_providers[].models", t("schema-models")),
            labeled_field("agent.acp", t("schema-acp")),
            labeled_field("agent.acp[].id", t("schema-id")),
            labeled_field("agent.acp[].name", t("schema-name")),
            labeled_field("agent.acp[].command", t("schema-command")),
            labeled_field("agent.acp[].args", t("schema-arguments")),
            labeled_field("agent.acp[].env", t("schema-environment")),
            labeled_field("agent.acp[].cwd", t("schema-working-directory")),
            field(
                "shortcuts",
                FieldSpec {
                    order: vec![
                        "chord_timeout_ms".into(),
                        "leader".into(),
                        "bindings".into(),
                    ],
                    ..Default::default()
                },
            ),
            field(
                "shortcuts.leader",
                FieldSpec {
                    label: Some(t("schema-leader")),
                    hint: Some(t("schema-leader-detail")),
                    widget: Some(WidgetKind::LeaderKbd),
                    ..Default::default()
                },
            ),
            field(
                "shortcuts.chord_timeout_ms",
                FieldSpec {
                    label: Some(t("schema-chord-timeout")),
                    hint: Some(t("schema-chord-timeout-detail")),
                    ..Default::default()
                },
            ),
            field(
                "shortcuts.bindings",
                FieldSpec {
                    label: Some(t("schema-bindings")),
                    widget: Some(WidgetKind::BindingsList),
                    ..Default::default()
                },
            ),
            field(
                "terminal",
                FieldSpec {
                    order: vec![
                        "shell".into(),
                        "font_family".into(),
                        "startup_dir".into(),
                        "confirm_close".into(),
                        "default_theme".into(),
                        "themes".into(),
                        "custom_themes".into(),
                    ],
                    ..Default::default()
                },
            ),
            labeled_field("terminal.shell", t("schema-shell")),
            labeled_field("terminal.font_family", t("schema-font-family")),
            labeled_field("terminal.startup_dir", t("schema-startup-directory")),
            field(
                "terminal.confirm_close",
                FieldSpec {
                    label: Some(t("schema-confirm-close")),
                    hint: Some(t("schema-confirm-close-detail")),
                    ..Default::default()
                },
            ),
            field(
                "terminal.default_theme",
                FieldSpec {
                    label: Some(t("schema-default-theme")),
                    hint: Some(t("schema-default-theme-detail")),
                    placeholder: Some("default".into()),
                    ..Default::default()
                },
            ),
            labeled_field("terminal.themes", t("schema-themes")),
            labeled_field("terminal.themes[].name", t("schema-name")),
            labeled_field("terminal.themes[].color_scheme", t("schema-color-scheme")),
            labeled_field("terminal.themes[].font_family", t("schema-font-family")),
            labeled_field("terminal.themes[].font_size", t("schema-font-size")),
            labeled_field("terminal.themes[].line_height", t("schema-line-height")),
            labeled_field("terminal.themes[].padding", t("schema-padding")),
            labeled_field("terminal.themes[].cursor_style", t("schema-cursor-style")),
            labeled_field("terminal.themes[].cursor_blink", t("schema-cursor-blink")),
            labeled_field("terminal.themes[].shell", t("schema-shell")),
            labeled_field("terminal.custom_themes", t("schema-custom-themes")),
            labeled_field("terminal.custom_themes[].name", t("schema-name")),
            labeled_field(
                "terminal.custom_themes[].foreground",
                t("schema-foreground"),
            ),
            labeled_field(
                "terminal.custom_themes[].background",
                t("schema-background"),
            ),
            labeled_field("terminal.custom_themes[].cursor", t("schema-cursor")),
            labeled_field("terminal.custom_themes[].ansi", t("schema-ansi-colors")),
            field(
                "editor",
                FieldSpec {
                    order: vec!["keymap".into(), "explorer".into(), "lsp".into()],
                    ..Default::default()
                },
            ),
            field(
                "editor.keymap",
                FieldSpec {
                    label: Some(t("schema-keymap")),
                    widget: Some(WidgetKind::Select),
                    options: vec![
                        SettingsSelectOption {
                            value: "standard".into(),
                            label: t("editor-keymap-standard"),
                        },
                        SettingsSelectOption {
                            value: "vim".into(),
                            label: t("editor-keymap-vim"),
                        },
                    ],
                    ..Default::default()
                },
            ),
            field(
                "editor.explorer",
                FieldSpec {
                    label: Some(t("schema-explorer")),
                    order: vec!["visible".into(), "width".into()],
                    ..Default::default()
                },
            ),
            labeled_field("editor.explorer.visible", t("schema-visible")),
            labeled_field("editor.explorer.width", t("schema-width")),
            field(
                "editor.lsp",
                FieldSpec {
                    label: Some(t("schema-language-servers")),
                    order: vec!["servers".into()],
                    ..Default::default()
                },
            ),
            labeled_field("editor.lsp.servers", t("schema-servers")),
            labeled_field("editor.lsp.servers.*.command", t("schema-command")),
            labeled_field("editor.lsp.servers.*.args", t("schema-arguments")),
            labeled_field("editor.lsp.servers.*.language_id", t("schema-language-id")),
            labeled_field(
                "editor.lsp.servers.*.root_markers",
                t("schema-root-markers"),
            ),
            field(
                "recording",
                FieldSpec {
                    order: vec!["output_dir".into()],
                    ..Default::default()
                },
            ),
            labeled_field("recording.output_dir", t("schema-output-directory")),
            labeled_field("spaces.*.startup_url", t("schema-startup-url")),
            labeled_field("spaces.*.startup_dir", t("schema-startup-directory")),
        ],
    }
}

fn field(path: &str, spec: FieldSpec) -> (String, FieldSpec) {
    (path.to_string(), spec)
}

fn labeled_field(path: &str, label: String) -> (String, FieldSpec) {
    field(
        path,
        FieldSpec {
            label: Some(label),
            ..Default::default()
        },
    )
}

#[cfg(test)]
mod agent_schema_tests {
    use super::*;

    #[test]
    fn schema_exposes_run_placement_override_under_agent() {
        let schema = build_settings_schema();
        assert!(schema.sections.iter().any(|section| section.id == "agent"));
        let field = schema
            .field("agent.allow_run_placement_override")
            .expect("run placement override field");
        assert_eq!(field.label.as_deref(), Some("Allow run placement override"));
    }
}

#[cfg(test)]
mod appearance_schema_tests {
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
    fn schema_exposes_stable_and_preview_channels() {
        let schema = build_settings_schema();
        let channel = schema
            .field("update_channel")
            .expect("update channel field");
        let options = channel
            .options
            .iter()
            .map(|option| option.value.as_str())
            .collect::<Vec<_>>();

        assert_eq!(channel.widget, Some(WidgetKind::Select));
        assert_eq!(options, vec!["stable", "preview"]);
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
        let bundled = Locale::available()
            .map(Locale::into_string)
            .collect::<Vec<_>>();
        assert_eq!(bundled[..], values[1..]);
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
        let schema = SettingsSchema::localized(&Locale::from("ja"));
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
}

#[cfg(test)]
mod browser_schema_tests {
    use super::*;

    #[test]
    fn schema_exposes_search_engine_select() {
        let schema = build_settings_schema();
        let field = schema
            .field("browser.search_engine")
            .expect("search engine field");
        assert_eq!(field.widget, Some(WidgetKind::Select));
        let values: Vec<_> = field
            .options
            .iter()
            .map(|option| option.value.as_str())
            .collect();
        assert_eq!(
            values,
            vec!["google", "bing", "duckduckgo", "brave", "kagi"]
        );
    }
}
