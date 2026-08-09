use super::*;

#[test]
fn acp_agent_config_parses() {
    let cfg: AcpAgentConfig = ron::from_str(
        r#"(id: "vibe-acp", name: "Vibe", command: "uv", args: ["run", "vibe-acp"])"#,
    )
    .unwrap();
    assert_eq!(cfg.id, "vibe-acp");
    assert_eq!(cfg.command, "uv");
    assert!(cfg.env.is_empty());
    assert_eq!(cfg.cwd, None);
}

#[test]
fn agent_settings_default_seeds_acp_agents() {
    let agent = AgentSettings::default();
    for id in ["claude", "codex", "gemini"] {
        assert!(
            agent.acp.iter().any(|c| c.id == id),
            "missing acp agent {id}"
        );
    }
}

#[test]
fn agent_defaults_enable_tidy() {
    let s = default_agent_settings();
    assert!(s.tidy_files);
    assert_eq!(s.tidy_files_max, 5);
    assert!(!s.tidy_files_auto);
}

#[test]
fn agent_defaults_disable_run_placement_override() {
    assert!(!AgentSettings::default().allow_run_placement_override);
}

#[test]
fn legacy_agent_settings_default_run_placement_override_to_disabled() {
    let settings = parse_settings("(agent: (follow_files: true))").unwrap();
    assert!(!settings.agent.allow_run_placement_override);
}

#[test]
fn apply_update_enables_run_placement_override() {
    let mut settings = base_settings();
    let ron = apply_settings_update(
        &mut settings,
        "agent.allow_run_placement_override",
        serde_json::json!(true),
    )
    .expect("update ok");
    assert!(settings.agent.allow_run_placement_override);
    let reparsed: AppSettings = ron::de::from_str(&ron).expect("RON parses");
    assert!(reparsed.agent.allow_run_placement_override);
}

#[test]
fn apply_update_sets_tidy_auto_without_clobbering_siblings() {
    let mut s = base_settings();
    assert!(s.agent.follow_files);
    let ron = apply_settings_update(&mut s, "agent.tidy_files_auto", serde_json::json!(true))
        .expect("update ok");
    assert!(s.agent.tidy_files_auto);
    assert!(s.agent.follow_files, "sibling preserved");
    assert!(ron.contains("tidy_files_auto"));
}

fn base_settings() -> AppSettings {
    AppSettings {
        browser: BrowserSettings {
            startup_url: default_browser_startup_url(),
            search_engine: SearchEngine::default(),
        },
        layout: LayoutSettings {
            radius: 0.0,
            window: WindowSettings { padding: 0.0 },
            pane: PaneSettings { gap: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        },
        shortcuts: ShortcutSettings::default(),
        terminal: None,
        auto_update: false,
        agent: crate::plugin::runtime::AgentSettings::default(),
        spaces: Default::default(),
        recording: Default::default(),
        editor: Default::default(),
        appearance: Default::default(),
    }
}

#[test]
fn explorer_settings_default_when_absent() {
    let s = base_settings();
    assert!(!s.editor.explorer.visible());
    assert_eq!(s.editor.explorer.width(), EXPLORER_DEFAULT_WIDTH);
}

#[test]
fn editor_wrap_defaults_match_enabled_vscode_settings() {
    let settings = EditorSettings::default();

    assert_eq!(settings.word_wrap, vmux_core::editor::WordWrap::On);
    assert_eq!(settings.word_wrap_column, 80);
}

#[test]
fn editor_wrap_uses_vscode_setting_values() {
    let settings =
        parse_settings("(editor: (word_wrap: wordWrapColumn, word_wrap_column: 100))").unwrap();

    assert_eq!(
        settings.editor.word_wrap,
        vmux_core::editor::WordWrap::WordWrapColumn
    );
    assert_eq!(settings.editor.word_wrap_column, 100);
}

#[test]
fn explorer_settings_present_overrides() {
    let e = ExplorerSettings {
        visible: Some(false),
        width: Some(320),
    };
    assert!(!e.visible());
    assert_eq!(e.width(), 320);
}

#[test]
fn explorer_width_clamps_out_of_range() {
    let huge = ExplorerSettings {
        visible: None,
        width: Some(9000),
    };
    assert_eq!(huge.width(), EXPLORER_MAX_WIDTH);
    let tiny = ExplorerSettings {
        visible: None,
        width: Some(1),
    };
    assert_eq!(tiny.width(), EXPLORER_MIN_WIDTH);
}

#[test]
fn resolve_startup_url_returns_browser_override() {
    let mut s = base_settings();
    s.browser.startup_url = "vmux://services/".into();
    assert_eq!(resolve_startup_url(&s, "space-1"), "vmux://services/");
}

#[test]
fn resolve_startup_url_defaults_to_start() {
    let s = base_settings();
    assert_eq!(resolve_startup_url(&s, "space-1"), "vmux://start/");
}

#[test]
fn resolve_startup_url_uses_start_for_empty_browser_url() {
    let mut s = base_settings();
    s.browser.startup_url.clear();
    assert_eq!(resolve_startup_url(&s, "space-1"), "vmux://start/");
}

#[test]
fn resolve_startup_url_treats_legacy_agent_default_as_start() {
    let mut s = base_settings();
    s.browser.startup_url = "vmux://agent/".into();
    assert_eq!(resolve_startup_url(&s, "space-1"), "vmux://start/");
}

#[test]
fn resolve_startup_dir_matches_slug_variant_key() {
    let dir = std::env::temp_dir();
    let mut s = base_settings();
    s.spaces.insert(
        "mistralai-dashboard".to_string(),
        SpaceOverrides {
            startup_url: None,
            startup_dir: Some(dir.to_string_lossy().to_string()),
        },
    );
    assert_eq!(resolve_startup_dir(&s, "mistralai/dashboard"), Some(dir));
}

#[test]
fn embedded_settings_default_to_start() {
    let s = load_embedded_settings();
    assert_eq!(resolve_startup_url(&s, "space-1"), "vmux://start/");
}

#[test]
fn resolve_startup_url_prefers_per_space_override() {
    let mut s = base_settings();
    s.browser.startup_url = "https://global.example".into();
    s.spaces.insert(
        "work".into(),
        SpaceOverrides {
            startup_url: Some("https://work.example".into()),
            startup_dir: None,
        },
    );
    assert_eq!(resolve_startup_url(&s, "work"), "https://work.example");
    assert_eq!(resolve_startup_url(&s, "other"), "https://global.example");
}

#[test]
fn resolve_startup_url_blank_per_space_falls_to_global() {
    let mut s = base_settings();
    s.browser.startup_url = "https://global.example".into();
    s.spaces.insert(
        "work".into(),
        SpaceOverrides {
            startup_url: Some("   ".into()),
            startup_dir: None,
        },
    );
    assert_eq!(resolve_startup_url(&s, "work"), "https://global.example");
}

#[test]
fn app_settings_roundtrips_through_json() {
    let original = base_settings();
    let value = serde_json::to_value(&original).expect("serialize");
    let recovered: AppSettings = serde_json::from_value(value).expect("deserialize");
    assert_eq!(
        recovered.layout.window.padding,
        original.layout.window.padding
    );
    assert_eq!(recovered.layout.pane.gap, original.layout.pane.gap);
    assert_eq!(
        recovered.shortcuts.chord_timeout_ms,
        original.shortcuts.chord_timeout_ms
    );
    assert_eq!(recovered.auto_update, original.auto_update);
}

#[test]
fn set_at_path_replaces_nested_object_value() {
    let mut root = serde_json::json!({"layout": {"pane": {"gap": 8.0}}});
    set_at_path(&mut root, "layout.pane.gap", serde_json::json!(12.0)).unwrap();
    assert_eq!(root["layout"]["pane"]["gap"], serde_json::json!(12.0));
}

#[test]
fn set_at_path_replaces_array_element_field() {
    let mut root = serde_json::json!({
        "terminal": {"themes": [{"name": "default", "font_size": 14.0}]}
    });
    set_at_path(
        &mut root,
        "terminal.themes[0].font_size",
        serde_json::json!(16.0),
    )
    .unwrap();
    assert_eq!(
        root["terminal"]["themes"][0]["font_size"],
        serde_json::json!(16.0)
    );
}

#[test]
fn set_at_path_top_level_leaf() {
    let mut root = serde_json::json!({"auto_update": true});
    set_at_path(&mut root, "auto_update", serde_json::json!(false)).unwrap();
    assert_eq!(root["auto_update"], serde_json::json!(false));
}

#[test]
fn set_at_path_unknown_key_errors() {
    let mut root = serde_json::json!({"layout": {}});
    let err = set_at_path(&mut root, "layout.nope", serde_json::json!(1)).unwrap_err();
    assert!(
        err.contains("layout.nope"),
        "error must mention path: {err}"
    );
}

#[test]
fn set_at_path_array_out_of_bounds_errors() {
    let mut root = serde_json::json!({"themes": [{"font_size": 14.0}]});
    let err = set_at_path(&mut root, "themes[5].font_size", serde_json::json!(16.0)).unwrap_err();
    assert!(err.contains("themes[5]"), "error must mention path: {err}");
}

#[test]
fn set_at_path_empty_path_errors() {
    let mut root = serde_json::json!({});
    assert!(set_at_path(&mut root, "", serde_json::json!(1)).is_err());
}

#[test]
fn apply_settings_update_changes_pane_gap_and_returns_ron() {
    let mut settings = base_settings();
    let ron_bytes =
        apply_settings_update(&mut settings, "layout.pane.gap", serde_json::json!(16.0))
            .expect("apply ok");
    assert_eq!(settings.layout.pane.gap, 16.0);
    assert!(ron_bytes.contains("gap"));
    assert!(ron_bytes.contains("16"));
    let reparsed: AppSettings = ron::de::from_str(&ron_bytes).expect("RON parses");
    assert_eq!(reparsed.layout.pane.gap, 16.0);
}

#[test]
fn apply_settings_update_changes_top_level_bool() {
    let mut settings = base_settings();
    apply_settings_update(&mut settings, "auto_update", serde_json::json!(true)).unwrap();
    assert!(settings.auto_update);
}

#[test]
fn apply_settings_update_unknown_path_errors_without_mutating() {
    let mut settings = base_settings();
    let original_gap = settings.layout.pane.gap;
    let err =
        apply_settings_update(&mut settings, "layout.nope", serde_json::json!(1)).unwrap_err();
    assert!(err.contains("layout.nope"));
    assert_eq!(settings.layout.pane.gap, original_gap);
}

#[test]
fn apply_settings_update_type_mismatch_errors_without_mutating() {
    let mut settings = base_settings();
    let original_auto = settings.auto_update;
    let err =
        apply_settings_update(&mut settings, "auto_update", serde_json::json!("yes")).unwrap_err();
    assert!(!err.is_empty());
    assert_eq!(settings.auto_update, original_auto);
}

#[test]
fn acp_agent_config_allows_version_only_entry() {
    let cfg: AcpAgentConfig = serde_json::from_value(serde_json::json!({
        "id": "claude",
        "name": "Claude Code",
        "version": "0.11.0",
    }))
    .expect("a version-only acp entry (no command) must parse");
    assert_eq!(cfg.command, "");
    assert_eq!(cfg.version.as_deref(), Some("0.11.0"));
    assert!(cfg.args.is_empty());
}

#[test]
fn content_hash_is_deterministic() {
    let h1 = settings_content_hash(b"hello");
    let h2 = settings_content_hash(b"hello");
    let h3 = settings_content_hash(b"world");
    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
}

#[test]
fn app_settings_spaces_roundtrip_through_ron() {
    let mut s = base_settings();
    s.spaces.insert(
        "work".into(),
        SpaceOverrides {
            startup_url: Some("https://work.example".into()),
            startup_dir: Some("/tmp/work".into()),
        },
    );
    let ron = ron::ser::to_string_pretty(&s, ron::ser::PrettyConfig::default()).unwrap();
    let back: AppSettings = ron::de::from_str(&ron).unwrap();
    assert_eq!(
        back.spaces["work"].startup_url.as_deref(),
        Some("https://work.example")
    );
    assert_eq!(
        back.spaces["work"].startup_dir.as_deref(),
        Some("/tmp/work")
    );
}

#[test]
fn embedded_settings_have_empty_spaces_and_no_global_startup_dir() {
    let s = load_embedded_settings();
    assert!(s.spaces.is_empty());
    assert!(
        s.terminal
            .as_ref()
            .and_then(|t| t.startup_dir.as_ref())
            .is_none()
    );
}

#[test]
fn embedded_default_theme_shell_is_portable() {
    let s = load_embedded_settings();
    let terminal = s.terminal.expect("embedded settings define terminal");
    let shell = terminal.resolve_theme(&terminal.default_theme).shell;
    assert_eq!(shell, default_shell());
}

#[test]
fn resolve_startup_dir_prefers_per_space_then_global() {
    let per = tempfile::tempdir().unwrap();
    let glob = tempfile::tempdir().unwrap();
    let mut s = base_settings();
    s.terminal = Some(TerminalSettings {
        startup_dir: Some(glob.path().to_string_lossy().into()),
        ..Default::default()
    });
    s.spaces.insert(
        "work".into(),
        SpaceOverrides {
            startup_url: None,
            startup_dir: Some(per.path().to_string_lossy().into()),
        },
    );
    assert_eq!(resolve_startup_dir(&s, "work").as_deref(), Some(per.path()));
    assert_eq!(
        resolve_startup_dir(&s, "other").as_deref(),
        Some(glob.path())
    );
    s.terminal = None;
    assert_eq!(resolve_startup_dir(&s, "space-1"), None);
}

#[test]
fn resolve_startup_dir_invalid_per_space_cascades_to_valid_global() {
    let glob = tempfile::tempdir().unwrap();
    let mut s = base_settings();
    s.terminal = Some(TerminalSettings {
        startup_dir: Some(glob.path().to_string_lossy().into()),
        ..Default::default()
    });
    s.spaces.insert(
        "work".into(),
        SpaceOverrides {
            startup_url: None,
            startup_dir: Some("/no/such/dir/xyz-vmux".into()),
        },
    );
    assert_eq!(
        resolve_startup_dir(&s, "work").as_deref(),
        Some(glob.path())
    );
}

#[test]
fn resolve_startup_dir_all_invalid_returns_none() {
    let mut s = base_settings();
    s.terminal = Some(TerminalSettings {
        startup_dir: Some("/no/such/global/xyz-vmux".into()),
        ..Default::default()
    });
    s.spaces.insert(
        "work".into(),
        SpaceOverrides {
            startup_url: None,
            startup_dir: Some("/no/such/dir/xyz-vmux".into()),
        },
    );
    assert_eq!(resolve_startup_dir(&s, "work"), None);
}

#[test]
fn resolve_startup_dir_for_tab_prefers_tab_then_space() {
    let tab = tempfile::tempdir().unwrap();
    let per = tempfile::tempdir().unwrap();
    let glob = tempfile::tempdir().unwrap();
    let mut s = base_settings();
    s.terminal = Some(TerminalSettings {
        startup_dir: Some(glob.path().to_string_lossy().into()),
        ..Default::default()
    });
    s.spaces.insert(
        "work".into(),
        SpaceOverrides {
            startup_url: None,
            startup_dir: Some(per.path().to_string_lossy().into()),
        },
    );
    let tab_dir = tab.path().to_string_lossy().into_owned();
    assert_eq!(
        resolve_startup_dir_for_tab(&s, "work", Some(&tab_dir)),
        Some(tab.path().to_path_buf())
    );
    assert_eq!(
        resolve_startup_dir_for_tab(&s, "work", None),
        Some(per.path().to_path_buf())
    );
}

#[test]
fn resolve_startup_dir_for_tab_invalid_tab_cascades_to_space() {
    let per = tempfile::tempdir().unwrap();
    let mut s = base_settings();
    s.spaces.insert(
        "work".into(),
        SpaceOverrides {
            startup_url: None,
            startup_dir: Some(per.path().to_string_lossy().into()),
        },
    );
    assert_eq!(
        resolve_startup_dir_for_tab(&s, "work", Some("/no/such/tab/xyz-vmux")),
        Some(per.path().to_path_buf())
    );
}

#[test]
fn resolve_tab_workspace_dir_rejects_invalid_stored_path_without_fallback() {
    let per = tempfile::tempdir().unwrap();
    let mut s = base_settings();
    s.spaces.insert(
        "work".into(),
        SpaceOverrides {
            startup_url: None,
            startup_dir: Some(per.path().to_string_lossy().into()),
        },
    );

    assert!(resolve_tab_workspace_dir(&s, "work", Some("/no/such/tab/xyz-vmux")).is_err());
    assert_eq!(
        resolve_tab_workspace_dir(&s, "work", None).unwrap(),
        Some(per.path().to_path_buf())
    );
}

#[test]
fn validate_tab_workspace_dir_rejects_relative_path() {
    assert!(validate_tab_workspace_dir(".").is_err());
}

#[test]
fn resolve_startup_dir_for_tab_with_source_reports_level() {
    let tab = tempfile::tempdir().unwrap();
    let per = tempfile::tempdir().unwrap();
    let glob = tempfile::tempdir().unwrap();
    let mut s = base_settings();
    s.terminal = Some(TerminalSettings {
        startup_dir: Some(glob.path().to_string_lossy().into()),
        ..Default::default()
    });
    s.spaces.insert(
        "work".into(),
        SpaceOverrides {
            startup_url: None,
            startup_dir: Some(per.path().to_string_lossy().into()),
        },
    );
    let tab_dir = tab.path().to_string_lossy().into_owned();
    assert_eq!(
        resolve_startup_dir_for_tab_with_source(&s, "work", Some(&tab_dir))
            .unwrap()
            .1,
        DirSource::Tab
    );
    assert_eq!(
        resolve_startup_dir_for_tab_with_source(&s, "work", None)
            .unwrap()
            .1,
        DirSource::Space
    );
    assert_eq!(
        resolve_startup_dir_for_tab_with_source(&s, "other", None)
            .unwrap()
            .1,
        DirSource::Global
    );
    s.terminal = None;
    assert_eq!(
        resolve_startup_dir_for_tab_with_source(&s, "nospace", None),
        None
    );
}

#[test]
fn parse_settings_merges_sparse_over_embedded() {
    let s = parse_settings(r#"(browser: (startup_url: "https://x.example"))"#).unwrap();
    assert_eq!(s.browser.startup_url, "https://x.example");
    // omitted sections come from the embedded defaults, NOT the plainer serde
    // field defaults (embedded leader is "b"; serde default would be "g").
    assert_eq!(s.shortcuts.leader.key, "b");
    assert_eq!(s.layout.radius, 8.0);
}

#[test]
fn parse_settings_empty_uses_embedded_defaults() {
    let s = parse_settings("()").unwrap();
    assert_eq!(s.shortcuts.leader.key, "b");
    assert_eq!(s.browser.startup_url, "vmux://start/");
    assert_eq!(s.browser.search_engine, SearchEngine::Google);
}

#[test]
fn parse_settings_selects_search_engine() {
    let s = parse_settings(r#"(browser: (search_engine: duckduckgo))"#).unwrap();
    assert_eq!(s.browser.search_engine, SearchEngine::DuckDuckGo);
}

#[test]
fn apply_settings_update_writes_only_changed_section() {
    let mut settings = parse_settings("()").unwrap();
    let ron = apply_settings_update(
        &mut settings,
        "browser.startup_url",
        serde_json::json!("https://x.example"),
    )
    .unwrap();
    assert!(ron.contains("browser"));
    assert!(ron.contains("https://x.example"));
    // untouched heavy sections stay out of the file
    assert!(!ron.contains("shortcuts"));
    assert!(!ron.contains("themes"));
    // and reload merges them back from the embedded defaults
    let reloaded = parse_settings(&ron).unwrap();
    assert_eq!(reloaded.browser.startup_url, "https://x.example");
    assert_eq!(reloaded.shortcuts.leader.key, "b");
}

#[test]
fn request_settings_save_sets_due() {
    use bevy::ecs::message::Messages;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SettingsSaveDebounce>()
        .add_message::<SettingsSaveRequest>()
        .add_systems(Update, request_settings_save);
    app.world_mut()
        .resource_mut::<Messages<SettingsSaveRequest>>()
        .write(SettingsSaveRequest);
    app.update();
    assert!(app.world().resource::<SettingsSaveDebounce>().due.is_some());
}

#[test]
fn flush_writes_after_due_elapses() {
    use bevy::ecs::message::Messages;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(base_settings())
        .insert_resource(SettingsSaveDebounce {
            due: Some(Instant::now() - Duration::from_secs(1)),
        })
        .add_message::<SettingsWriteRequest>()
        .add_systems(Update, flush_settings_save);
    app.update();
    let writes = app
        .world_mut()
        .resource_mut::<Messages<SettingsWriteRequest>>()
        .drain()
        .count();
    assert_eq!(writes, 1);
    assert!(app.world().resource::<SettingsSaveDebounce>().due.is_none());
}

#[test]
fn flush_skips_before_due() {
    use bevy::ecs::message::Messages;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(base_settings())
        .insert_resource(SettingsSaveDebounce {
            due: Some(Instant::now() + Duration::from_secs(60)),
        })
        .add_message::<SettingsWriteRequest>()
        .add_systems(Update, flush_settings_save);
    app.update();
    let writes = app
        .world_mut()
        .resource_mut::<Messages<SettingsWriteRequest>>()
        .drain()
        .count();
    assert_eq!(writes, 0);
    assert!(app.world().resource::<SettingsSaveDebounce>().due.is_some());
}

#[test]
fn sparse_save_omits_terminal_when_unchanged() {
    let s = load_embedded_settings();
    let ron = sparse_settings_ron(&s).unwrap();
    assert!(
        !ron.contains("terminal"),
        "unchanged terminal must be omitted: {ron}"
    );
}

#[test]
fn sparse_save_persists_vim_keymap() {
    let mut settings = load_embedded_settings();
    settings.editor.keymap = vmux_core::KeymapKind::Vim;

    let ron = sparse_settings_ron(&settings).unwrap();
    assert!(ron.contains("editor: (keymap: vim)"), "{ron}");
    assert!(!ron.contains("word_wrap"), "{ron}");
    assert_eq!(
        parse_settings(&ron).unwrap().editor.keymap,
        vmux_core::KeymapKind::Vim
    );
}

#[test]
fn embedded_settings_bind_tab_nav_to_leader() {
    let s = load_embedded_settings();
    let leader_key = |cmd: &str| -> Option<String> {
        s.shortcuts.bindings.iter().find_map(|e| match &e.binding {
            ShortcutDef::Leader(combo) if e.command == cmd => Some(combo.key.clone()),
            _ => None,
        })
    };

    assert_eq!(
        leader_key("open_in_new_tab").as_deref(),
        Some("c"),
        "leader c must create a new tab"
    );
    assert_eq!(
        leader_key("next_tab").as_deref(),
        Some("n"),
        "leader n must select the next tab"
    );
    assert_eq!(
        leader_key("prev_tab").as_deref(),
        Some("p"),
        "leader p must select the previous tab"
    );
    assert_eq!(
        leader_key("open_in_new_stack"),
        None,
        "leader c is rebound from new stack to new tab"
    );
}

#[test]
fn sparse_save_omits_default_equal_theme_fields() {
    let mut s = load_embedded_settings();
    s.terminal
        .as_mut()
        .unwrap()
        .themes
        .iter_mut()
        .find(|t| t.name == "default")
        .unwrap()
        .font_size = 12.0;

    let ron = sparse_settings_ron(&s).unwrap();
    assert!(ron.contains("font_size"), "changed field persisted: {ron}");
    assert!(ron.contains("12"), "changed value persisted: {ron}");
    assert!(
        !ron.contains("font_family"),
        "default-equal font_family must be omitted: {ron}"
    );
    assert!(
        !ron.contains("color_scheme"),
        "default-equal color_scheme must be omitted: {ron}"
    );
    assert!(
        !ron.contains("cursor_style"),
        "default-equal cursor_style must be omitted: {ron}"
    );

    let reloaded = parse_settings(&ron).unwrap();
    let theme = reloaded.terminal.unwrap().resolve_theme("default");
    assert_eq!(theme.font_size, 12.0);
    assert_eq!(theme.font_family, default_terminal_font_family());
}

#[test]
fn sparse_save_keeps_genuinely_overridden_field() {
    let mut s = load_embedded_settings();
    s.terminal
        .as_mut()
        .unwrap()
        .themes
        .iter_mut()
        .find(|t| t.name == "default")
        .unwrap()
        .font_family = "Menlo".to_string();

    let ron = sparse_settings_ron(&s).unwrap();
    assert!(
        ron.contains("Menlo"),
        "explicit override must be persisted: {ron}"
    );
    let reloaded = parse_settings(&ron).unwrap();
    assert_eq!(
        reloaded
            .terminal
            .unwrap()
            .resolve_theme("default")
            .font_family,
        "Menlo"
    );
}

#[test]
fn color_scheme_defaults_to_device() {
    assert_eq!(ColorScheme::default(), ColorScheme::Device);
}

#[test]
fn appearance_absent_falls_back_to_device() {
    let s = parse_settings("()").expect("parse empty");
    assert_eq!(s.appearance.mode, ColorScheme::Device);
    assert_eq!(s.appearance.locale, "system");
}

#[test]
fn appearance_round_trips_through_ron() {
    let s = parse_settings("(appearance: (mode: light))").expect("parse light");
    assert_eq!(s.appearance.mode, ColorScheme::Light);
    let s = parse_settings("(appearance: (mode: dark, locale: \"ja\"))").expect("parse dark");
    assert_eq!(s.appearance.mode, ColorScheme::Dark);
    assert_eq!(s.appearance.locale, "ja");
}

#[test]
fn sparse_omits_default_appearance_and_emits_changed() {
    let s = load_embedded_settings();
    assert!(!sparse_settings_ron(&s).unwrap().contains("appearance"));
    let mut s = s;
    s.appearance.mode = ColorScheme::Dark;
    s.appearance.locale = "ja".to_string();
    let out = sparse_settings_ron(&s).unwrap();
    assert!(
        out.contains("appearance"),
        "changed appearance persisted: {out}"
    );
    assert!(out.contains("dark"), "mode value persisted: {out}");
    assert!(out.contains("ja"), "locale value persisted: {out}");
    assert_eq!(
        parse_settings(&out).unwrap().appearance.mode,
        ColorScheme::Dark
    );
    assert_eq!(parse_settings(&out).unwrap().appearance.locale, "ja");
}
