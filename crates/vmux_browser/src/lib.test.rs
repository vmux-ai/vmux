use super::*;

#[test]
fn cef_disables_bfcache_for_extension_ports() {
    assert!(
        cef_command_line_config()
            .switch_values
            .contains(&("disable-features", "BackForwardCache"))
    );
}

#[test]
fn knowledge_paths_only_open_vault_markdown_and_directories() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("knowledge");
    let folder = vault.join("projects");
    std::fs::create_dir_all(&folder).unwrap();
    let note = folder.join("brief.md");
    let text = folder.join("brief.txt");
    let outside = temp.path().join("outside.md");
    std::fs::write(&note, "# Brief").unwrap();
    std::fs::write(&text, "Brief").unwrap();
    std::fs::write(&outside, "# Outside").unwrap();

    assert!(knowledge_path_url(&vault, &vault).is_some());
    assert!(knowledge_path_url(&vault, &folder).is_some());
    assert!(knowledge_path_url(&vault, &note).is_some());
    assert!(knowledge_path_url(&vault, &text).is_none());
    assert!(knowledge_path_url(&vault, &outside).is_none());
}

#[test]
fn stored_tab_dir_is_sidebar_source_of_truth() {
    let tab = Tab {
        name: "test".into(),
        startup_dir: Some("/tmp/agent-checkout".into()),
    };
    let settings = test_app_settings_with_radius(0.0);

    assert_eq!(
        tab_boundary_dir(&tab, &settings, None),
        Some((
            std::path::PathBuf::from("/tmp/agent-checkout"),
            vmux_setting::DirSource::Tab,
        ))
    );
}

#[test]
fn legacy_tab_boundary_uses_space_fallback_without_migration() {
    let dir = std::env::temp_dir();
    let record = vmux_space::model::bootstrap_space_record();
    let mut settings = test_app_settings_with_radius(0.0);
    settings.spaces.insert(
        record.id.clone(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(dir.to_string_lossy().into_owned()),
        },
    );
    let tab = Tab::default();

    let (path, source) = tab_boundary_dir(
        &tab,
        &settings,
        Some(&vmux_space::spaces::ActiveSpace { record }),
    )
    .unwrap();

    assert_eq!(path, dir);
    assert_eq!(source, vmux_setting::DirSource::Space);
    assert_eq!(tab.startup_dir, None);
}

#[test]
fn normalize_vmux_url_trims_and_adds_trailing_slash_to_bare_host() {
    assert_eq!(normalize_vmux_url("vmux://lsp"), "vmux://lsp/");
    assert_eq!(normalize_vmux_url("vmux://terminal"), "vmux://terminal/");
    assert_eq!(normalize_vmux_url("vmux://lsp/"), "vmux://lsp/");
    assert_eq!(
        normalize_vmux_url("vmux://agent/vibe/"),
        "vmux://agent/vibe/"
    );
    assert_eq!(
        normalize_vmux_url("vmux://error/?title=x"),
        "vmux://error/?title=x"
    );
    assert_eq!(
        normalize_vmux_url("file:///tmp/main.rs"),
        "file:///tmp/main.rs"
    );
    assert_eq!(
        normalize_vmux_url("  vmux://agent/codex/session-id  "),
        "vmux://agent/codex/session-id"
    );
}

#[test]
fn effective_title_prefers_nonempty_osc() {
    use vmux_core::OscTitle;
    assert_eq!(
        effective_title(Some(&OscTitle("osc".to_string())), "def"),
        "osc"
    );
    assert_eq!(
        effective_title(Some(&OscTitle(String::new())), "def"),
        "def"
    );
    assert_eq!(effective_title(None, "def"), "def");
}

#[test]
fn osr_webview_hides_when_window_is_hidden() {
    assert!(!should_show_osr_webview(true, true, true, false, false));
    assert!(!should_show_osr_webview(false, true, true, true, false));
    assert!(!should_show_osr_webview(false, false, true, false, false));
    assert!(should_show_osr_webview(true, true, true, true, false));
}

#[test]
fn auxiliary_osr_webviews_remain_visible_when_window_is_focused() {
    assert!(should_show_osr_webview(true, false, true, false, false));
    assert!(should_show_osr_webview(true, true, false, false, false));
    assert!(should_show_osr_webview(true, true, true, false, true));
}

fn layout_material_after_mode(
    mode: vmux_layout::scene::InteractionMode,
    initial_alpha: f32,
) -> WebviewExtendStandardMaterial {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .insert_resource(mode)
        .add_systems(Update, sync_layout_mesh_visibility);
    let mut material = WebviewExtendStandardMaterial::default();
    material.base.alpha_mode = AlphaMode::Blend;
    material.base.base_color.set_alpha(initial_alpha);
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WebviewExtendStandardMaterial>>()
        .add(material);
    app.world_mut()
        .spawn((LayoutCef, WebviewMaterialHandle(handle.clone())));

    app.update();

    app.world()
        .resource::<Assets<WebviewExtendStandardMaterial>>()
        .get(handle.id())
        .expect("layout material")
        .clone()
}

#[test]
fn user_mode_hides_layout_mesh_behind_native_overlay() {
    let mat = layout_material_after_mode(vmux_layout::scene::InteractionMode::User, 1.0);
    assert_eq!(
        mat.base.base_color.alpha(),
        0.0,
        "User mode presents layout chrome through the native accelerated overlay"
    );
    assert_eq!(mat.base.alpha_mode, AlphaMode::Blend);
}

#[test]
fn player_mode_makes_layout_mesh_visible_and_transparent() {
    let mat = layout_material_after_mode(vmux_layout::scene::InteractionMode::Player, 0.0);
    assert_eq!(
        mat.base.base_color.alpha(),
        1.0,
        "Player mode renders the layout via the mesh, so it must be visible"
    );
    assert_eq!(
        mat.base.alpha_mode,
        AlphaMode::Blend,
        "Player uses straight alpha so pages show through the layout's transparent areas"
    );
}

#[test]
fn agent_cli_url_redirects_tab_to_session_id() {
    let mut app = App::new();
    app.add_systems(Update, sync_page_metadata_to_tab);

    let stack = app
        .world_mut()
        .spawn((
            Stack::default(),
            PageMetadata {
                url: "vmux://agent/vibe/".to_string(),
                ..default()
            },
        ))
        .id();
    let child = app
        .world_mut()
        .spawn((
            Browser,
            PageMetadata {
                url: "vmux://agent/vibe/".to_string(),
                ..default()
            },
            ChildOf(stack),
        ))
        .id();

    app.update();

    app.world_mut().get_mut::<PageMetadata>(child).unwrap().url =
        "vmux://agent/vibe/abc-123".to_string();

    app.update();

    let stack_url = app.world().get::<PageMetadata>(stack).unwrap().url.clone();
    assert_eq!(stack_url, "vmux://agent/vibe/abc-123");
}

#[test]
fn side_sheet_close_stack_routes_through_stack_command() {
    let source = include_str!("lib.rs");
    let branch = source
        .split("\"close_stack\" => {")
        .nth(1)
        .and_then(|rest| rest.split("\"new_stack\" => {").next())
        .expect("close_stack branch");

    assert!(branch.contains("StackCommand::Close"));
    assert!(!branch.contains("window.visible = false"));
}

#[test]
fn cef_pointer_hit_rect_contains_edges() {
    let rect = CefPointerHitRect {
        center: Vec2::new(50.0, 20.0),
        size: Vec2::new(100.0, 40.0),
        interactive: true,
    };

    assert!(cef_pointer_hit_rect_contains(rect, Vec2::new(0.0, 0.0)));
    assert!(cef_pointer_hit_rect_contains(rect, Vec2::new(100.0, 40.0)));
    assert!(!cef_pointer_hit_rect_contains(rect, Vec2::new(100.1, 20.0)));
}

#[test]
fn cef_pointer_ignores_inactive_regions() {
    let rect = CefPointerHitRect {
        center: Vec2::new(50.0, 20.0),
        size: Vec2::new(100.0, 40.0),
        interactive: false,
    };

    assert!(!cef_pointer_hit_rect_contains(rect, Vec2::new(50.0, 20.0)));
}

#[test]
fn hidden_or_collapsed_webviews_do_not_render() {
    assert!(!webview_layout_is_renderable(
        Vec2::ZERO,
        Some(&Visibility::Inherited),
        false
    ));
    assert!(!webview_layout_is_renderable(
        Vec2::new(100.0, 0.0),
        Some(&Visibility::Inherited),
        false
    ));
    assert!(!webview_layout_is_renderable(
        Vec2::new(100.0, 20.0),
        Some(&Visibility::Hidden),
        false
    ));
    assert!(webview_layout_is_renderable(
        Vec2::new(100.0, 20.0),
        Some(&Visibility::Inherited),
        false
    ));
}

#[test]
fn hidden_pending_reveal_webviews_resize_before_reveal() {
    assert!(webview_layout_is_renderable(
        Vec2::new(100.0, 20.0),
        Some(&Visibility::Hidden),
        true
    ));
}

#[test]
fn inactive_tab_pages_keep_size_other_hidden_pages_collapse() {
    assert_eq!(
        hidden_webview_sizing(true, false),
        HiddenWebviewSizing::Render
    );
    assert_eq!(
        hidden_webview_sizing(true, true),
        HiddenWebviewSizing::Render
    );
    assert_eq!(
        hidden_webview_sizing(false, true),
        HiddenWebviewSizing::HideKeepSize
    );
    assert_eq!(
        hidden_webview_sizing(false, false),
        HiddenWebviewSizing::Collapse
    );
}

#[test]
fn layout_shell_osr_renders_above_player_page_osr() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<vmux_layout::NewStackContext>()
        .add_systems(Update, sync_children_to_ui);

    let glass = app
        .world_mut()
        .spawn((
            VmuxWindow,
            ComputedNode {
                size: Vec2::new(1200.0, 800.0),
                ..default()
            },
            UiGlobalTransform::default(),
        ))
        .id();
    let layout = app
        .world_mut()
        .spawn((
            Browser,
            LayoutCef,
            Transform::default(),
            ComputedNode {
                size: Vec2::new(1200.0, 800.0),
                ..default()
            },
            bevy::ui::ComputedStackIndex(0),
            UiGlobalTransform::default(),
            WebviewSize(Vec2::ONE),
            ChildOf(glass),
        ))
        .id();
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(1)))
        .id();
    let pane = app
        .world_mut()
        .spawn((
            Pane,
            ComputedNode {
                size: Vec2::new(1200.0, 740.0),
                ..default()
            },
            UiGlobalTransform::default(),
            ChildOf(tab),
        ))
        .id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
        .id();
    let page = app
        .world_mut()
        .spawn((
            Browser,
            Transform::default(),
            ComputedNode {
                size: Vec2::new(1200.0, 740.0),
                ..default()
            },
            bevy::ui::ComputedStackIndex(0),
            UiGlobalTransform::default(),
            WebviewSize(Vec2::ONE),
            ChildOf(stack),
        ))
        .id();

    app.update();

    let layout_z = app.world().get::<Transform>(layout).unwrap().translation.z;
    let page_z = app.world().get::<Transform>(page).unwrap().translation.z;

    assert!(layout_z > page_z);
}

#[test]
fn pending_reveal_webviews_keep_cef_running() {
    assert!(webview_osr_should_run(
        Vec2::ZERO,
        Some(&Visibility::Hidden),
        true
    ));
}

#[test]
fn hidden_osr_command_bar_stays_warm_for_reopen() {
    assert!(keep_hidden_osr_webview_warm(true, false, true));
    assert!(!keep_hidden_osr_webview_warm(false, false, true));
    assert!(!keep_hidden_osr_webview_warm(true, true, true));
    assert!(!keep_hidden_osr_webview_warm(true, false, false));
}

#[test]
fn command_bar_modal_wins_osr_focus_for_keyboard_input() {
    let pane = Entity::from_bits(1);
    let modal = Entity::from_bits(2);

    assert_eq!(
        choose_osr_active_webview(Some((modal, false)), Some(pane), pane),
        Some(modal)
    );
}

#[test]
fn windowed_command_bar_modal_suppresses_osr_focus_targets() {
    let pane = Entity::from_bits(1);
    let modal = Entity::from_bits(2);

    assert_eq!(
        choose_osr_active_webview(Some((modal, true)), Some(pane), pane),
        None
    );
}

#[test]
fn open_command_bar_is_exclusive_cef_keyboard_target() {
    let mut app = App::new();
    app.insert_resource(vmux_layout::scene::InteractionMode::User)
        .init_resource::<vmux_layout::stack::FocusedStack>()
        .insert_resource(CefSuppressKeyboardInput(true))
        .add_systems(Update, sync_keyboard_target);
    let page = app.world_mut().spawn((Browser, CefKeyboardTarget)).id();
    let modal = app
        .world_mut()
        .spawn((
            Browser,
            Modal,
            Node {
                display: Display::Flex,
                ..default()
            },
            CefKeyboardTarget,
        ))
        .id();

    app.update();

    assert!(app.world().get::<CefKeyboardTarget>(modal).is_some());
    assert!(app.world().get::<CefKeyboardTarget>(page).is_none());
    assert!(!app.world().resource::<CefSuppressKeyboardInput>().0);
}

#[test]
fn layout_shell_is_auxiliary_osr_focus_target() {
    let active = Entity::from_bits(1);
    let layout = Entity::from_bits(2);
    let sidecar = Entity::from_bits(3);

    assert_eq!(
        osr_focus_targets(&[active, layout, sidecar], Some(active), false, |e| e
            == layout),
        (Some(active), vec![layout, sidecar])
    );
}

#[test]
fn layout_shell_is_not_active_osr_focus_target() {
    let layout = Entity::from_bits(1);
    let sidecar = Entity::from_bits(2);

    assert_eq!(
        osr_focus_targets(&[layout, sidecar], Some(layout), false, |e| e == layout),
        (None, vec![layout, sidecar])
    );
}

#[test]
fn bookmark_text_input_can_make_layout_shell_active_osr_target() {
    let layout = Entity::from_bits(1);
    let sidecar = Entity::from_bits(2);

    assert_eq!(
        osr_focus_targets(&[layout, sidecar], Some(layout), true, |e| e == layout),
        (Some(layout), vec![sidecar])
    );
}

#[test]
fn windowed_layout_sync_raises_layout_above_bevy_view() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_layout")
        .nth(1)
        .and_then(|tail| tail.split("fn apply_repaint_nudge").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("browsers.raise_windowed_to_front"));
    assert!(!sync_fn.contains("browsers.lower_windowed_to_back"));
}

#[test]
fn native_layout_sync_runs_before_native_page_sync() {
    let source = include_str!("lib.rs");
    let post_update = source
        .split("PostUpdate,")
        .nth(1)
        .and_then(|tail| tail.split(".chain()").next())
        .unwrap_or_default();
    let layout_idx = post_update
        .find("sync_windowed_layout")
        .expect("windowed layout sync");
    let page_idx = post_update
        .find("sync_windowed_frames")
        .expect("windowed page sync");

    assert!(layout_idx < page_idx);
}

#[test]
fn windowed_page_sync_sends_pages_above_layout() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_frames")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_layout").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("browsers.raise_windowed_to_front"));
}

#[test]
fn windowed_page_sync_raises_visible_pages_and_hides_inactive() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_frames")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_layout").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("browsers.raise_windowed_to_front(&entity)"));
    assert!(sync_fn.contains("windowed_pages_to_hide("));
}

#[test]
fn webview_tab_visibility_uses_active_marker_not_global_recency() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_children_to_ui")
        .nth(1)
        .and_then(|tail| tail.split("fn webview_should_use_windowed").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("active_tab_q.contains(tab)"));
    assert!(!sync_fn.contains("max_by_key"));
}

#[test]
fn windowed_pages_hide_on_deactivate_and_first_show() {
    let just_deactivated = Entity::from_bits(1);
    let still_inactive = Entity::from_bits(2);
    let never_shown = Entity::from_bits(3);

    let hidden = [just_deactivated, still_inactive, never_shown];
    let prev_visible = [just_deactivated];
    let ever_shown = [just_deactivated, still_inactive];

    assert_eq!(
        windowed_pages_to_hide(&hidden, &prev_visible, &ever_shown, &[]),
        vec![just_deactivated, never_shown]
    );
}

#[test]
fn recreated_inactive_windowed_page_is_hidden() {
    let page = Entity::from_bits(1);

    assert_eq!(
        windowed_pages_to_hide(&[page], &[], &[page], &[page]),
        vec![page]
    );
}

#[test]
fn layout_fixed_offsets_use_computed_header_rect() {
    let computed = ComputedNode {
        size: Vec2::new(1_544.0, 168.0),
        inverse_scale_factor: 0.5,
        ..default()
    };
    let transform = UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(
        788.0, 84.0,
    )));

    let offsets =
        layout_fixed_offsets_from_computed(&computed, &transform, 1_600.0).expect("offsets");

    assert_eq!(offsets.left, 8.0);
    assert_eq!(offsets.top, 0.0);
    assert_eq!(offsets.right, 20.0);
    assert_eq!(offsets.height, 84.0);
}

#[test]
fn windowed_content_mesh_material_is_hidden() {
    let mut material = WebviewExtendStandardMaterial::default();

    set_windowed_content_mesh_material(&mut material, true);

    assert_eq!(material.base.base_color.alpha(), 0.0);
    assert_eq!(material.base.alpha_mode, AlphaMode::Blend);

    set_windowed_content_mesh_material(&mut material, false);

    assert_eq!(material.base.base_color.alpha(), 1.0);
    assert_eq!(material.base.alpha_mode, AlphaMode::Opaque);
}

fn test_app_settings_with_radius(radius: f32) -> AppSettings {
    AppSettings {
        browser: vmux_setting::BrowserSettings {
            startup_url: "about:blank".to_string(),
            ..Default::default()
        },
        layout: vmux_layout::settings::LayoutSettings {
            radius,
            window: vmux_layout::settings::WindowSettings { padding: 0.0 },
            pane: vmux_layout::settings::PaneSettings { gap: 0.0 },
            side_sheet: vmux_layout::settings::SideSheetSettings::default(),
            focus_ring: vmux_layout::settings::FocusRingSettings::default(),
        },
        shortcuts: vmux_setting::ShortcutSettings::default(),
        terminal: None,
        auto_update: false,
        agent: vmux_setting::AgentSettings::default(),
        spaces: Default::default(),
        recording: Default::default(),
        editor: Default::default(),
        appearance: Default::default(),
    }
}

#[test]
fn appearance_change_updates_cef_color_scheme() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_app_settings_with_radius(0.0))
        .init_resource::<CefColorScheme>()
        .add_systems(
            Update,
            sync_appearance_to_cef.run_if(resource_changed::<AppSettings>),
        );
    app.update();
    app.world_mut()
        .resource_mut::<AppSettings>()
        .appearance
        .mode = vmux_setting::ColorScheme::Light;
    app.update();
    assert_eq!(
        app.world().resource::<CefColorScheme>().0,
        CefColorMode::Light
    );
}

#[test]
fn player_osr_pane_clip_uses_alpha_to_coverage_for_rounded_corners() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_app_settings_with_radius(12.0))
        .insert_resource(vmux_layout::toggle::LayoutHidden(false))
        .insert_resource(vmux_layout::scene::InteractionMode::Player)
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, sync_webview_pane_corner_clip);

    let handle = app
        .world_mut()
        .resource_mut::<Assets<WebviewExtendStandardMaterial>>()
        .add(WebviewExtendStandardMaterial::default());
    let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
    let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), ChildOf(pane)))
        .id();
    app.world_mut().spawn((
        Browser,
        WebviewSize(Vec2::new(320.0, 240.0)),
        WebviewMaterialHandle(handle.clone()),
        ChildOf(stack),
    ));

    app.update();

    let material = app
        .world()
        .resource::<Assets<WebviewExtendStandardMaterial>>()
        .get(&handle)
        .expect("webview material");

    assert_eq!(
        material.extension.pane_corner_clip,
        Vec4::new(12.0, 320.0, 240.0, 0.0)
    );
    assert_eq!(material.base.alpha_mode, AlphaMode::AlphaToCoverage);
}

#[test]
fn layout_cef_shell_keeps_blend_material() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_app_settings_with_radius(12.0))
        .insert_resource(vmux_layout::toggle::LayoutHidden(false))
        .insert_resource(vmux_layout::scene::InteractionMode::Player)
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, sync_webview_pane_corner_clip);

    let mut material = WebviewExtendStandardMaterial::default();
    material.base.alpha_mode = AlphaMode::Blend;
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WebviewExtendStandardMaterial>>()
        .add(material);
    app.world_mut().spawn((
        Browser,
        LayoutCef,
        WebviewSize(Vec2::new(320.0, 240.0)),
        WebviewMaterialHandle(handle.clone()),
    ));

    app.update();

    let material = app
        .world()
        .resource::<Assets<WebviewExtendStandardMaterial>>()
        .get(&handle)
        .expect("webview material");

    assert_eq!(material.extension.pane_corner_clip, Vec4::ZERO);
    assert_eq!(material.base.alpha_mode, AlphaMode::Blend);
}

#[test]
fn windowed_page_sync_keeps_pages_visible_while_command_bar_is_open() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_frames")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_layout").next())
        .unwrap_or_default();

    assert!(!sync_fn.contains("is_command_bar_open"));
    assert!(!sync_fn.contains("return;"));
}

#[test]
fn windowed_hover_refresh_position_maps_physical_cursor_to_webview_space() {
    let frame = WindowedHoverRefreshFrame {
        left_px: 100.0,
        top_px: 50.0,
        width_px: 400.0,
        height_px: 300.0,
        scale: 2.0,
    };

    assert_eq!(
        windowed_hover_refresh_position(Vec2::new(300.0, 250.0), frame),
        Some(Vec2::new(100.0, 100.0))
    );
}

#[test]
fn windowed_hover_refresh_position_ignores_cursor_outside_frame() {
    let frame = WindowedHoverRefreshFrame {
        left_px: 100.0,
        top_px: 50.0,
        width_px: 400.0,
        height_px: 300.0,
        scale: 2.0,
    };

    assert_eq!(
        windowed_hover_refresh_position(Vec2::new(99.0, 250.0), frame),
        None
    );
}

#[test]
fn windowed_page_sync_applies_settings_radius_to_native_page() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_frames")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_layout").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("settings: Res<AppSettings>"));
    assert!(sync_fn.contains("settings.layout.radius"));
    assert!(sync_fn.contains("browsers.set_windowed_corner_radius"));
}

#[test]
fn windowed_page_sync_uses_native_corner_policy() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_frames")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_layout").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("visible_pane_count_for_windowed_sync"));
    assert!(sync_fn.contains("windowed_page_all_corners(layout_hidden.0, visible_pane_count)"));
}

#[test]
fn windowed_page_keeps_single_pane_top_edge_flat_under_header() {
    assert!(!windowed_page_all_corners(false, 1));
}

#[test]
fn windowed_page_rounds_when_layout_hidden_or_split() {
    assert!(windowed_page_all_corners(true, 1));
    assert!(windowed_page_all_corners(false, 2));
}

#[test]
fn windowed_page_sync_aligns_single_pane_frame_to_header() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_frames")
        .nth(1)
        .and_then(|tail| tail.split("fn visible_pane_count_for_windowed_sync").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("header_rect"));
    assert!(sync_fn.contains("windowed_page_frame_rect("));
}

#[test]
fn single_pane_windowed_frame_matches_header_edges_without_side_gaps() {
    let pane = WindowedFrameRect {
        left: 60.2,
        top: 84.0,
        width: 150.6,
        height: 300.0,
    };
    let header = WindowedFrameRect {
        left: 72.1,
        top: 0.0,
        width: 130.8,
        height: 84.2,
    };

    let frame = windowed_page_frame_rect(pane, Some(header), false, 1);

    assert_eq!(
        frame,
        WindowedFrameRect {
            left: 73.0,
            top: 85.0,
            width: 129.0,
            height: 299.0,
        }
    );
}

#[test]
fn split_pane_windowed_frame_starts_below_header_without_changing_width() {
    let pane = WindowedFrameRect {
        left: 610.2,
        top: 24.0,
        width: 560.6,
        height: 720.0,
    };
    let header = WindowedFrameRect {
        left: 150.0,
        top: 24.0,
        width: 1020.0,
        height: 72.2,
    };

    let frame = windowed_page_frame_rect(pane, Some(header), false, 2);

    assert_eq!(
        frame,
        WindowedFrameRect {
            left: 611.0,
            top: 97.0,
            width: 559.0,
            height: 647.0,
        }
    );
}

#[test]
fn windowed_frame_hit_test_uses_physical_page_bounds() {
    let frame = WindowedFrameRect {
        left: 100.0,
        top: 50.0,
        width: 400.0,
        height: 300.0,
    };

    assert!(windowed_frame_contains(frame, Vec2::new(100.0, 50.0)));
    assert!(windowed_frame_contains(frame, Vec2::new(500.0, 350.0)));
    assert!(!windowed_frame_contains(frame, Vec2::new(99.0, 200.0)));
    assert!(!windowed_frame_contains(frame, Vec2::new(300.0, 351.0)));
}

#[test]
fn windowed_page_sync_sets_focus_ring_on_active_split_page() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_frames")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_layout").next())
        .unwrap_or_default();

    let ring_fn = source
        .split("fn windowed_ring_for")
        .nth(1)
        .and_then(|tail| tail.split("fn agent_ring_rgb").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("focus: Res<vmux_layout::stack::FocusedStack>"));
    assert!(sync_fn.contains("browsers.set_windowed_focus_ring"));
    assert!(ring_fn.contains("focus.stack == Some(stack)"));
    assert!(ring_fn.contains("visible_pane_count > 1"));
}

#[test]
fn windowed_page_sync_covers_corners_over_remote_content() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_frames")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_layout").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("browsers.set_windowed_corner_cover"));
    assert!(sync_fn.contains("clear_color.0.to_srgba()"));
}

#[test]
fn windowed_page_sync_uses_native_focus_ring_for_terminals() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_frames")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_layout").next())
        .unwrap_or_default();

    assert!(!sync_fn.contains("!is_terminal"));
    assert!(sync_fn.contains("browsers.set_windowed_focus_ring"));
}

#[test]
fn windowed_page_sync_scales_native_radius_and_focus_ring_to_physical_pixels() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_frames")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_layout").next())
        .unwrap_or_default();

    let ring_fn = source
        .split("fn windowed_ring_for")
        .nth(1)
        .and_then(|tail| tail.split("fn agent_ring_rgb").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("settings.layout.radius * scale"));
    assert!(ring_fn.contains("settings.layout.focus_ring.width * scale"));
}

#[test]
fn windowed_command_bar_sync_keeps_modal_above_pages() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_command_bar")
        .nth(1)
        .and_then(|tail| tail.split("fn apply_repaint_nudge").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("browsers.raise_windowed_to_front(&entity);"));
    assert!(!sync_fn.contains("if !*was_open {\n        browsers.raise_windowed_to_front"));
}

#[test]
fn browser_mode_keeps_layout_and_command_bar_osr_for_native_overlays() {
    let source = include_str!("lib.rs");
    let backend_fn = source
        .split("fn sync_cef_backend_for_interaction_mode")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_frames").next())
        .unwrap_or_default();

    assert!(backend_fn.contains("Has<LayoutCef>"));
    assert!(backend_fn.contains("Has<Modal>"));
    assert!(backend_fn.contains("!is_layout && !is_modal"));
    assert!(backend_fn.contains("WebviewNativeOverlay"));
    assert!(backend_fn.contains("target_native_direct_overlay"));
}

#[test]
fn layout_overlay_mode_change_recreates_browser() {
    let source = include_str!("lib.rs");
    let backend_fn = source
        .split("fn sync_cef_backend_for_interaction_mode")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_frames").next())
        .unwrap_or_default();

    assert!(backend_fn.contains("actual_native_overlay != want_native_overlay"));
    assert!(backend_fn.contains("browsers.has_browser(entity)"));
}

#[test]
fn browser_mode_uses_windowed_webviews_on_macos() {
    assert_eq!(
        webview_should_use_windowed(vmux_layout::scene::InteractionMode::User),
        cfg!(target_os = "macos")
    );
}

#[test]
fn player_mode_uses_osr_webviews() {
    assert!(!webview_should_use_windowed(
        vmux_layout::scene::InteractionMode::Player
    ));
}

#[test]
fn browser_mode_keeps_layout_and_modal_osr_and_windows_pages_on_macos() {
    let mut app = App::new();
    app.world_mut().insert_non_send(Browsers::default());
    app.insert_resource(vmux_layout::scene::InteractionMode::User);

    let layout = app
        .world_mut()
        .spawn((Browser, LayoutCef, WebviewSource::new("vmux://layout/")))
        .id();
    let modal = app
        .world_mut()
        .spawn((Browser, Modal, WebviewSource::new("vmux://command-bar/")))
        .id();
    let page = app
        .world_mut()
        .spawn((Browser, WebviewSource::new("https://example.com/")))
        .id();
    let terminal = app
        .world_mut()
        .spawn((Browser, Terminal, WebviewSource::new("vmux://terminal/")))
        .id();

    sync_cef_backend_for_interaction_mode(app.world_mut());

    assert!(app.world().get::<WebviewWindowed>(layout).is_none());
    assert_eq!(
        app.world().get::<WebviewNativeOverlay>(layout).is_some(),
        cfg!(target_os = "macos")
    );
    assert_eq!(
        app.world()
            .get::<WebviewNativeDirectOverlay>(layout)
            .is_some(),
        cfg!(target_os = "macos")
    );
    assert!(
        app.world()
            .get::<WebviewNativeDirectOverlay>(modal)
            .is_none()
    );
    assert_eq!(
        app.world().get::<WebviewNativeOverlay>(modal).is_some(),
        cfg!(target_os = "macos")
    );
    assert_eq!(
        app.world().get::<WebviewWindowed>(terminal).is_some(),
        cfg!(target_os = "macos")
    );
    assert!(app.world().get::<WebviewWindowed>(modal).is_none());
    assert_eq!(
        app.world().get::<WebviewWindowed>(page).is_some(),
        cfg!(target_os = "macos")
    );
}

#[test]
fn user_player_user_backend_round_trip() {
    let mut app = App::new();
    app.world_mut().insert_non_send(Browsers::default());
    app.insert_resource(vmux_layout::scene::InteractionMode::User);
    let window = Window {
        resolution: (800, 600).into(),
        ..default()
    };
    let home = vmux_layout::scene::frame_main_camera_transform(&window, 800.0 / 600.0, 0.0);
    app.world_mut().spawn((window, PrimaryWindow));
    app.world_mut()
        .spawn((vmux_layout::scene::MainCamera, home));

    let layout = app
        .world_mut()
        .spawn((Browser, LayoutCef, WebviewSource::new("vmux://layout/")))
        .id();
    let modal = app
        .world_mut()
        .spawn((Browser, Modal, WebviewSource::new("vmux://command-bar/")))
        .id();
    let page = app
        .world_mut()
        .spawn((Browser, WebviewSource::new("https://example.com/")))
        .id();

    sync_cef_backend_for_interaction_mode(app.world_mut());
    app.insert_resource(vmux_layout::scene::InteractionMode::Player);
    sync_cef_backend_for_interaction_mode(app.world_mut());
    app.insert_resource(vmux_layout::scene::InteractionMode::User);
    sync_cef_backend_for_interaction_mode(app.world_mut());

    assert!(app.world().get::<WebviewWindowed>(layout).is_none());
    assert_eq!(
        app.world().get::<WebviewNativeOverlay>(layout).is_some(),
        cfg!(target_os = "macos")
    );
    assert_eq!(
        app.world()
            .get::<WebviewNativeDirectOverlay>(layout)
            .is_some(),
        cfg!(target_os = "macos")
    );
    assert!(app.world().get::<WebviewWindowed>(modal).is_none());
    assert!(
        app.world()
            .get::<WebviewNativeDirectOverlay>(modal)
            .is_none()
    );
    assert_eq!(
        app.world().get::<WebviewNativeOverlay>(modal).is_some(),
        cfg!(target_os = "macos")
    );
    assert_eq!(
        app.world().get::<WebviewWindowed>(page).is_some(),
        cfg!(target_os = "macos")
    );
}

#[test]
fn browser_mode_disables_windowed_pages_when_camera_is_off_axis() {
    let mut app = App::new();
    app.world_mut().insert_non_send(Browsers::default());
    app.insert_resource(vmux_layout::scene::InteractionMode::User);
    app.world_mut().spawn((
        Window {
            resolution: (800, 600).into(),
            ..default()
        },
        PrimaryWindow,
    ));
    app.world_mut().spawn((
        vmux_layout::scene::MainCamera,
        Transform::from_xyz(2.0, 1.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let page = app
        .world_mut()
        .spawn((
            Browser,
            WebviewWindowed,
            WebviewSource::new("https://example.com/"),
        ))
        .id();

    sync_cef_backend_for_interaction_mode(app.world_mut());
    sync_cef_backend_for_interaction_mode(app.world_mut());

    assert!(app.world().get::<WebviewWindowed>(page).is_none());
}

#[test]
fn browser_mode_keeps_windowed_pages_for_first_resize_camera_mismatch() {
    let mut app = App::new();
    app.world_mut().insert_non_send(Browsers::default());
    app.insert_resource(vmux_layout::scene::InteractionMode::User);
    let old_window = Window {
        resolution: (800, 600).into(),
        ..default()
    };
    let stale_home =
        vmux_layout::scene::frame_main_camera_transform(&old_window, 800.0 / 600.0, 0.0);
    app.world_mut().spawn((
        Window {
            resolution: (1200, 900).into(),
            ..default()
        },
        PrimaryWindow,
    ));
    app.world_mut()
        .spawn((vmux_layout::scene::MainCamera, stale_home));
    let page = app
        .world_mut()
        .spawn((
            Browser,
            WebviewWindowed,
            WebviewSource::new("https://example.com/"),
        ))
        .id();

    sync_cef_backend_for_interaction_mode(app.world_mut());

    assert_eq!(
        app.world().get::<WebviewWindowed>(page).is_some(),
        cfg!(target_os = "macos")
    );
}

#[test]
fn browser_mode_keeps_windowed_pages_when_camera_is_home() {
    let mut app = App::new();
    app.world_mut().insert_non_send(Browsers::default());
    app.insert_resource(vmux_layout::scene::InteractionMode::User);
    let window = Window {
        resolution: (800, 600).into(),
        ..default()
    };
    let home = vmux_layout::scene::frame_main_camera_transform(&window, 800.0 / 600.0, 0.0);
    app.world_mut().spawn((window, PrimaryWindow));
    app.world_mut()
        .spawn((vmux_layout::scene::MainCamera, home));
    let page = app
        .world_mut()
        .spawn((Browser, WebviewSource::new("https://example.com/")))
        .id();

    sync_cef_backend_for_interaction_mode(app.world_mut());

    assert_eq!(
        app.world().get::<WebviewWindowed>(page).is_some(),
        cfg!(target_os = "macos")
    );
}

#[test]
fn player_mode_marks_every_cef_surface_osr() {
    let mut app = App::new();
    app.world_mut().insert_non_send(Browsers::default());
    app.insert_resource(vmux_layout::scene::InteractionMode::Player);

    let layout = app
        .world_mut()
        .spawn((
            Browser,
            LayoutCef,
            WebviewWindowed,
            WebviewSource::new("vmux://layout/"),
        ))
        .id();
    let modal = app
        .world_mut()
        .spawn((
            Browser,
            Modal,
            WebviewWindowed,
            WebviewSource::new("vmux://command-bar/"),
        ))
        .id();
    let page = app
        .world_mut()
        .spawn((
            Browser,
            WebviewWindowed,
            WebviewSource::new("https://example.com/"),
        ))
        .id();

    sync_cef_backend_for_interaction_mode(app.world_mut());

    assert!(app.world().get::<WebviewWindowed>(layout).is_none());
    assert!(app.world().get::<WebviewWindowed>(modal).is_none());
    assert!(app.world().get::<WebviewWindowed>(page).is_none());
}

#[test]
fn backend_sync_runs_after_page_spawners_before_cef_create() {
    let source = include_str!("lib.rs");
    let backend_sync = source
        .split("fn configure_cef_backend_sync")
        .nth(1)
        .and_then(|tail| tail.split("impl Plugin for BrowserPlugin").next())
        .unwrap_or_default();

    assert!(backend_sync.contains(".after(PageOpenSet::Fallback)"));
    assert!(backend_sync.contains(".after(spawn_popup_stacks)"));
    assert!(backend_sync.contains(".before(CefSystems::CreateAndResize)"));
}

#[derive(Resource, Default)]
struct ObservedBackendMode(Option<vmux_layout::scene::InteractionMode>);

fn finish_exit_for_backend_sync_test(mut mode: ResMut<vmux_layout::scene::InteractionMode>) {
    *mode = vmux_layout::scene::InteractionMode::User;
}

fn observe_backend_sync_mode(
    mode: Res<vmux_layout::scene::InteractionMode>,
    mut observed: ResMut<ObservedBackendMode>,
) {
    observed.0 = Some(*mode);
}

#[test]
fn backend_sync_runs_after_exit_transition_completion() {
    let mut app = App::new();
    app.world_mut().insert_non_send(Browsers::default());
    configure_cef_backend_sync(&mut app)
        .insert_resource(vmux_layout::scene::InteractionMode::Player)
        .init_resource::<ObservedBackendMode>()
        .add_systems(
            Update,
            finish_exit_for_backend_sync_test
                .in_set(vmux_layout::scene::SceneSystems::CompleteModeTransition),
        )
        .add_systems(
            Update,
            observe_backend_sync_mode
                .in_set(BrowserSystems::SyncCefBackend)
                .before(sync_cef_backend_for_interaction_mode),
        );

    app.update();

    assert!(
        app.world().resource::<ObservedBackendMode>().0
            == Some(vmux_layout::scene::InteractionMode::User)
    );
}

#[test]
fn command_bar_windowed_frame_uses_measured_height() {
    let frame =
        command_bar_windowed_frame(1600.0, 1000.0, 2.0, Some(Vec2::new(500.0, 220.0)), None)
            .unwrap();

    assert!((frame.left_px - 224.0).abs() < 0.01);
    assert!((frame.top_px - 150.0).abs() < 0.01);
    assert!((frame.width_px - 1152.0).abs() < 0.01);
    assert!((frame.height_px - 440.0).abs() < 0.01);
}

#[test]
fn command_bar_windowed_frame_clamps_height_to_window() {
    let frame = command_bar_windowed_frame(800.0, 500.0, 1.0, Some(Vec2::new(500.0, 1000.0)), None)
        .unwrap();

    assert!((frame.top_px - 75.0).abs() < 0.01);
    assert!((frame.height_px - 409.0).abs() < 0.01);
}

#[test]
fn command_bar_windowed_frame_centers_in_page_workspace() {
    let frame = command_bar_windowed_frame(
        1600.0,
        1000.0,
        2.0,
        Some(Vec2::new(500.0, 220.0)),
        Some(WindowedFrameRect {
            left: 300.0,
            top: 100.0,
            width: 1200.0,
            height: 800.0,
        }),
    )
    .unwrap();

    assert!((frame.left_px - 332.0).abs() < 0.01);
    assert!((frame.top_px - 220.0).abs() < 0.01);
    assert!((frame.width_px - 1136.0).abs() < 0.01);
    assert!((frame.height_px - 440.0).abs() < 0.01);
}

#[test]
fn windowed_command_bar_outside_click_dismisses() {
    let frame = CommandBarWindowedFrame {
        left_px: 100.0,
        top_px: 50.0,
        width_px: 200.0,
        height_px: 100.0,
    };

    assert!(command_bar_windowed_click_should_dismiss(
        true,
        MouseButton::Left,
        ButtonState::Pressed,
        Some(Vec2::new(99.0, 80.0)),
        Some(frame),
    ));
    assert!(!command_bar_windowed_click_should_dismiss(
        true,
        MouseButton::Left,
        ButtonState::Pressed,
        Some(Vec2::new(150.0, 80.0)),
        Some(frame),
    ));
    assert!(!command_bar_windowed_click_should_dismiss(
        true,
        MouseButton::Right,
        ButtonState::Pressed,
        Some(Vec2::new(99.0, 80.0)),
        Some(frame),
    ));
    assert!(!command_bar_windowed_click_should_dismiss(
        false,
        MouseButton::Left,
        ButtonState::Pressed,
        Some(Vec2::new(99.0, 80.0)),
        Some(frame),
    ));
}

#[test]
fn browser_plugin_wires_command_bar_outside_click_dismiss() {
    let source = include_str!("lib.rs");
    let plugin_build = source
        .split("impl Plugin for BrowserPlugin")
        .nth(1)
        .and_then(|tail| tail.split("fn on_webview_ready_send_theme").next())
        .unwrap_or_default();

    assert!(plugin_build.contains("dismiss_command_bar_from_native_monitor"));
    assert!(plugin_build.contains("dismiss_windowed_command_bar_on_outside_click"));
    assert!(plugin_build.contains("run_if(on_message::<MouseButtonInput>)"));
}

#[test]
fn browser_plugin_wires_active_windowed_hover_refresh() {
    let source = include_str!("lib.rs");
    let plugin_build = source
        .split("impl Plugin for BrowserPlugin")
        .nth(1)
        .and_then(|tail| tail.split("fn on_webview_ready_send_theme").next())
        .unwrap_or_default();
    let refresh_fn = source
        .split("fn refresh_active_windowed_hover")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_layout").next())
        .unwrap_or_default();

    assert!(plugin_build.contains("refresh_layout_cef_hover"));
    assert!(plugin_build.contains("refresh_active_windowed_hover"));
    assert!(refresh_fn.contains("With<CefKeyboardTarget>"));
    assert!(refresh_fn.contains("With<WebviewWindowed>"));
    assert!(refresh_fn.contains("vmux_layout::pane::pane_hover_cursor_position"));
    assert!(refresh_fn.contains("browsers.send_mouse_move"));
    assert!(refresh_fn.contains("state.position == Some(position)"));
}

#[test]
fn browser_plugin_refreshes_layout_hover_from_native_cursor() {
    let source = include_str!("lib.rs");
    let refresh_fn = source
        .split("fn refresh_layout_cef_hover")
        .nth(1)
        .and_then(|tail| tail.split("fn refresh_active_windowed_hover").next())
        .unwrap_or_default();

    assert!(refresh_fn.contains("vmux_layout::native_pointer::snapshot()"));
    assert!(refresh_fn.contains("set_native_layout_pointer_regions"));
    assert!(refresh_fn.contains("physical_cef_pointer_hit_rect"));
    assert!(refresh_fn.contains("browsers.native_mouse_move_presenter"));
    assert!(refresh_fn.contains("queue_native_layout_pointer_move"));
    assert!(refresh_fn.contains("flush_native_layout_pointer_move"));
    assert!(refresh_fn.contains("window.resolution.scale_factor()"));
    assert!(refresh_fn.matches("reset_layout_cef_hover").count() >= 5);
}

#[test]
fn native_layout_pointer_queue_retains_only_latest_sample() {
    let source = include_str!("lib.rs");
    let sample = source
        .split("fn queue_native_layout_pointer_sample")
        .nth(1)
        .and_then(|tail| tail.split("pub fn queue_native_layout_pointer_move").next())
        .unwrap_or_default();
    let queue = source
        .split("pub fn queue_native_layout_pointer_move")
        .nth(1)
        .and_then(|tail| tail.split("pub fn flush_native_layout_pointer_move").next())
        .unwrap_or_default();
    let flush = source
        .split("pub fn flush_native_layout_pointer_move")
        .nth(1)
        .and_then(|tail| tail.split("pub fn native_layout_pointer_is_inside").next())
        .unwrap_or_default();

    assert!(sample.contains("state.position_px = Some(position)"));
    assert!(sample.contains("state.buttons = buttons"));
    assert!(source.contains("fn queue_native_layout_pointer_sample"));
    assert!(sample.contains("sample_changed"));
    assert!(sample.contains("state.pending = true"));
    assert!(queue.contains("queue_native_layout_pointer_sample"));
    assert!(flush.contains("state.pending = false"));
    assert!(flush.contains("presenter.send(position_px / state.scale"));
}

#[cfg(target_os = "macos")]
#[test]
fn native_layout_pointer_queue_skips_identical_sample() {
    let mut state = NativeLayoutPointerState {
        regions: vec![CefPointerHitRect {
            center: Vec2::new(50.0, 25.0),
            size: Vec2::new(20.0, 10.0),
            interactive: true,
        }],
        ..Default::default()
    };
    let buttons = NativeMouseButtons::default();

    let entered = queue_native_layout_pointer_sample(&mut state, Vec2::new(50.0, 25.0), buttons);
    assert!(entered.owns_pointer);
    assert!(entered.region_changed);
    assert!(entered.pending);
    state.pending = false;
    let duplicate = queue_native_layout_pointer_sample(&mut state, Vec2::new(50.0, 25.0), buttons);
    assert!(duplicate.owns_pointer);
    assert!(!duplicate.region_changed);
    assert!(!duplicate.pending);
    let moved = queue_native_layout_pointer_sample(&mut state, Vec2::new(51.0, 25.0), buttons);
    assert!(moved.owns_pointer);
    assert!(!moved.region_changed);
    assert!(moved.pending);
}

#[test]
fn macos_layout_mouse_move_has_one_forwarding_path() {
    let source = include_str!("lib.rs");
    let raw_forward = source
        .split("#[cfg(target_os = \"macos\")]\nfn forward_layout_cef_cursor_move")
        .nth(1)
        .and_then(|tail| tail.split("#[cfg(not(target_os = \"macos\"))]").next())
        .unwrap_or_default();

    assert!(!raw_forward.contains("browsers.send_mouse_move"));
    assert!(raw_forward.contains("events.read()"));
}

#[test]
fn macos_layout_click_uses_native_pointer_position() {
    let source = include_str!("lib.rs");
    let click_forward = source
        .split("fn forward_layout_cef_mouse_button")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn dismiss_windowed_command_bar_on_outside_click")
                .next()
        })
        .unwrap_or_default();
    let target_sync = source
        .split("fn sync_layout_cef_pointer_target")
        .nth(1)
        .and_then(|tail| tail.split("fn forward_layout_cef_cursor_move").next())
        .unwrap_or_default();

    assert!(click_forward.contains("vmux_layout::native_pointer::snapshot()"));
    assert!(target_sync.contains("vmux_layout::native_pointer::snapshot()"));
}

#[test]
fn layout_pointer_regions_match_layout_coordinates() {
    let rect = CefPointerHitRect {
        center: Vec2::new(50.0, 25.0),
        size: Vec2::new(20.0, 10.0),
        interactive: true,
    };

    assert!(cef_pointer_hit_rect_contains(rect, Vec2::new(50.0, 25.0)));
    assert!(!cef_pointer_hit_rect_contains(rect, Vec2::new(39.0, 25.0)));
}

#[test]
fn layout_frame_rate_bursts_after_input() {
    let now = std::time::Instant::now();
    assert_eq!(
        layout_frame_rate(now, None, false, false),
        LAYOUT_IDLE_FRAME_RATE
    );
    assert_eq!(
        layout_frame_rate(now, None, true, false),
        LAYOUT_ACTIVE_FRAME_RATE
    );
    assert_eq!(
        layout_frame_rate(now, Some(now), false, false),
        LAYOUT_ACTIVE_FRAME_RATE
    );
    assert_eq!(
        layout_frame_rate(now, None, true, true),
        LAYOUT_ACTIVE_FRAME_RATE
    );
}

#[test]
fn layout_host_emit_requests_frame_burst() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<LayoutFrameRateBurst>()
        .add_observer(request_layout_frame_burst);
    app.world_mut().insert_non_send(Browsers::default());
    let other = app.world_mut().spawn_empty().id();
    app.world_mut()
        .trigger(BinHostEmitEvent::from_bytes(other, "other", Vec::new()));
    assert!(
        app.world()
            .resource::<LayoutFrameRateBurst>()
            .last_emit
            .is_none()
    );

    let layout = app
        .world_mut()
        .spawn((LayoutCef, WebviewMaxFrameRate(LAYOUT_IDLE_FRAME_RATE)))
        .id();
    app.world_mut().trigger(BinHostEmitEvent::from_bytes(
        layout,
        PANE_TREE_EVENT,
        Vec::new(),
    ));
    assert!(
        app.world()
            .resource::<LayoutFrameRateBurst>()
            .last_emit
            .is_none()
    );
    assert_eq!(
        app.world().get::<WebviewMaxFrameRate>(layout).unwrap().0,
        LAYOUT_IDLE_FRAME_RATE
    );
    app.world_mut()
        .trigger(BinHostEmitEvent::from_bytes(layout, "tabs", Vec::new()));
    assert!(
        app.world()
            .resource::<LayoutFrameRateBurst>()
            .last_emit
            .is_some()
    );
    assert_eq!(
        app.world().get::<WebviewMaxFrameRate>(layout).unwrap().0,
        LAYOUT_ACTIVE_FRAME_RATE
    );

    // The command bar panel animates open on the layout surface, so it has to burst too or
    // the reveal plays at the idle rate.
    app.world_mut()
        .entity_mut(layout)
        .insert(WebviewMaxFrameRate(LAYOUT_IDLE_FRAME_RATE));
    app.world_mut().trigger(BinHostEmitEvent::from_bytes(
        layout,
        LAYOUT_COMMAND_BAR_OPEN_EVENT,
        Vec::new(),
    ));
    assert_eq!(
        app.world().get::<WebviewMaxFrameRate>(layout).unwrap().0,
        LAYOUT_ACTIVE_FRAME_RATE
    );
}

#[test]
fn active_windowed_hover_refresh_skips_native_left_drag() {
    let source = include_str!("lib.rs");
    let refresh_fn = source
        .split("fn refresh_active_windowed_hover")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_windowed_layout").next())
        .unwrap_or_default();

    assert!(refresh_fn.contains("native_left_mouse_down()"));
    assert!(refresh_fn.contains("return;"));
}

/// One test, because every case here mutates the process-wide published route and the test
/// runner is multi-threaded.
#[test]
fn published_route_is_the_only_source_of_command_bar_hit_state() {
    let frame = CommandBarWindowedFrame {
        left_px: 100.0,
        top_px: 50.0,
        width_px: 200.0,
        height_px: 100.0,
    };

    let before = native_command_bar_route().generation;
    publish_native_command_bar_route(true, Some(frame), 2.0);
    let published = native_command_bar_route();
    assert_eq!(published.generation, before.wrapping_add(1));
    assert_eq!(published.scale, 2.0);

    // A frame published by a bar that no longer owns input must not turn an unrelated click
    // into a dismiss.
    publish_native_command_bar_route(false, Some(frame), 1.0);
    assert!(!request_native_command_bar_dismiss_for_mouse_down(
        90.0, 60.0
    ));
    assert!(!take_native_command_bar_dismiss_requested());

    publish_native_command_bar_route(true, Some(frame), 1.0);
    assert!(!request_native_command_bar_dismiss_for_mouse_down(
        120.0, 60.0
    ));
    assert!(!take_native_command_bar_dismiss_requested());
    assert!(request_native_command_bar_dismiss_for_mouse_down(
        90.0, 60.0
    ));
    assert!(take_native_command_bar_dismiss_requested());
    assert!(!take_native_command_bar_dismiss_requested());

    // Revealing: owns input, but no rectangle is on screen to click outside of yet.
    publish_native_command_bar_route(true, None, 1.0);
    assert!(!request_native_command_bar_dismiss_for_mouse_down(
        90.0, 60.0
    ));

    // Closing drops a dismiss that was requested while open.
    assert!(request_native_command_bar_dismiss());
    publish_native_command_bar_route(false, None, 1.0);
    assert!(!take_native_command_bar_dismiss_requested());
}

#[test]
fn revealing_command_bar_owns_input_while_its_view_stays_parked() {
    let revealing = CommandBarState::from_modal(Display::Flex, Visibility::Hidden, true);

    assert!(revealing.owns_input());
    assert!(!revealing.is_shown());
    assert!(command_bar_windowed_view_should_render_hidden(
        Display::Flex,
        Visibility::Hidden
    ));
}

#[test]
fn collapsed_command_bar_view_is_never_render_hidden() {
    assert!(!command_bar_windowed_view_should_render_hidden(
        Display::None,
        Visibility::Hidden
    ));
    assert!(!command_bar_windowed_view_should_render_hidden(
        Display::Flex,
        Visibility::Inherited
    ));
}

#[test]
fn generic_webview_resize_excludes_command_bar_modal() {
    let source = include_str!("lib.rs");
    let resize_fn = source
        .split("fn sync_cef_webview_resize_after_ui")
        .nth(1)
        .and_then(|tail| tail.split("fn pane_count_for_browser").next())
        .unwrap_or_default();

    assert!(resize_fn.contains("Without<Modal>"));
}

#[test]
fn windowed_reconcile_wakes_until_native_pages_are_sized() {
    assert!(windowed_reconcile_should_wake(true, false, false));
    assert!(windowed_reconcile_should_wake(false, true, true));
    assert!(!windowed_reconcile_should_wake(false, true, false));
    assert!(!windowed_reconcile_should_wake(false, false, true));
}

#[test]
fn command_bar_windowed_sync_resizes_cef_to_native_frame() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_command_bar")
        .nth(1)
        .and_then(|tail| tail.split("fn apply_repaint_nudge").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("browsers.resize"));
    assert!(sync_fn.contains("native_size_changed.contains(entity)"));
    assert!(sync_fn.contains("browsers.nudge_windowed_repaint(&entity)"));
}

/// The command bar is the one windowed browser that hosts a real DOM text field, so it takes
/// AppKit first responder and lets Chromium handle typing natively. `send_key_event` forwarding
/// is a windowless API and does not produce DOM key events for a windowed browser.
#[test]
fn command_bar_windowed_sync_takes_native_focus() {
    let source = include_str!("lib.rs");
    let sync_fn = source
        .split("fn sync_windowed_command_bar")
        .nth(1)
        .and_then(|tail| tail.split("fn apply_repaint_nudge").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("browsers.set_windowed_focus(&entity, true)"));
    assert!(sync_fn.contains("browsers.set_windowed_focus(&entity, false)"));
}

#[test]
fn active_browser_url_wins_over_stale_new_stack_placeholder() {
    let stack = Entity::from_bits(1);
    let rows = [StackRow {
        title: "Google".to_string(),
        url: "https://www.google.com".to_string(),
        icon: vmux_core::PageIcon::None,
        is_active: true,
        bg_color: None,
    }];

    assert!(!should_emit_new_stack_placeholder(
        Some(stack),
        Some(stack),
        &rows
    ));
}

#[test]
fn host_payload_emits_again_when_page_ready_changes() {
    assert!(should_emit_cached_payload("tabs", "tabs", true));
    assert!(should_emit_cached_payload("tabs-2", "tabs", false));
    assert!(!should_emit_cached_payload("tabs", "tabs", false));
}

#[test]
fn layout_state_padding_reads_effective_window_node_padding() {
    let node = Node {
        padding: UiRect {
            top: Val::Px(10.0),
            right: Val::Px(11.0),
            bottom: Val::Px(12.0),
            left: Val::Px(13.0),
        },
        ..default()
    };

    assert_eq!(
        layout_window_padding_from_node(&node),
        LayoutWindowPadding {
            top: 10.0,
            right: 11.0,
            bottom: 12.0,
            left: 13.0,
        }
    );
}

mod browser_navigate_flow {
    use crate::{Browser, PendingNavSnapshots, RecentBrowserInteraction};
    use bevy::ecs::relationship::Relationship;
    use bevy::prelude::*;
    use bevy_cef::prelude::WebviewExtendStandardMaterial;
    use vmux_agent::events::AgentCommandRequest;
    use vmux_agent::plugin::AgentSessionPlugin;
    use vmux_agent::strategy::AgentStrategies;
    use vmux_core::{
        CefPageAttachRequest, LastActivatedAt, PageMetadata, PageOpenError, PageOpenHandled,
        PageOpenId, PageOpenRequest, PageOpenSet, PageOpenTask,
    };
    use vmux_layout::pane::Pane;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_layout::stack::FocusedStack;
    use vmux_service::protocol::{AgentCommand as ServiceAgentCommand, AgentRequestId};
    use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};
    use vmux_terminal::Terminal;

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
                ..Default::default()
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
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
            recording: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
        }
    }

    struct ConsumerPlugin;

    impl Plugin for ConsumerPlugin {
        fn build(&self, app: &mut App) {
            app.add_message::<vmux_layout::BrowserNavigateRequest>()
                .add_message::<vmux_layout::BrowserGoBackRequest>()
                .add_message::<vmux_layout::BrowserGoForwardRequest>()
                .add_message::<vmux_layout::OpenInNewStackRequest>()
                .add_message::<vmux_layout::ExtensionInstallRequest>()
                .add_message::<PageOpenRequest>()
                .add_message::<CefPageAttachRequest>()
                .add_message::<vmux_layout::apply::LayoutApplyRequest>()
                .add_message::<vmux_layout::apply::LayoutApplyResponse>()
                .add_message::<vmux_layout::apply::LayoutSnapshotRequest>()
                .add_message::<vmux_layout::apply::LayoutSnapshotResponse>()
                .add_message::<vmux_terminal::TerminalSendRequest>()
                .add_message::<vmux_terminal::RunShellRequest>()
                .add_message::<vmux_setting::SettingsWriteRequest>()
                .add_message::<vmux_space::SpaceCommandRequest>()
                .add_message::<vmux_history::query::HistoryOpenIntent>()
                .add_message::<vmux_layout::active_panes::ActivatePane>()
                .init_resource::<crate::PendingNavSnapshots>()
                .init_resource::<crate::RecentBrowserInteraction>()
                .configure_sets(
                    Update,
                    (
                        PageOpenSet::ResolveTarget,
                        PageOpenSet::HandleKnownPages,
                        PageOpenSet::Fallback,
                        PageOpenSet::Respond,
                    )
                        .chain(),
                )
                .add_systems(
                    Update,
                    (
                        crate::handle_browser_navigate_requests.before(PageOpenSet::ResolveTarget),
                        crate::handle_page_open_requests.in_set(PageOpenSet::ResolveTarget),
                        handle_test_known_page_open.in_set(PageOpenSet::HandleKnownPages),
                        crate::attach_cef_page_requests.in_set(PageOpenSet::Fallback),
                        crate::handle_unclaimed_page_open_tasks.in_set(PageOpenSet::Fallback),
                        crate::respond_page_open_tasks.in_set(PageOpenSet::Respond),
                        vmux_terminal::handle_terminal_send_requests,
                        vmux_terminal::handle_run_shell_requests,
                    ),
                );
        }
    }

    type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

    fn handle_test_known_page_open(
        tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
        children_q: Query<&Children>,
        mut commands: Commands,
    ) {
        for (entity, task) in &tasks {
            if task.url.starts_with("vmux://terminal/") {
                crate::clear_stack_children(task.stack, &children_q, &mut commands);
                commands.spawn((Browser, Terminal, ChildOf(task.stack)));
                commands.entity(entity).insert(PageOpenHandled);
            } else if task.url.starts_with("vmux://agent/") {
                crate::clear_stack_children(task.stack, &children_q, &mut commands);
                commands.entity(entity).insert(PageOpenHandled);
            }
        }
    }

    #[derive(Resource, Default)]
    struct CapturedNavigateUrls(Vec<String>);

    #[test]
    fn browser_navigate_triggers_request_navigate_with_url() {
        use bevy_cef::prelude::RequestNavigate;
        use vmux_layout::Browser;

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
            ConsumerPlugin,
        ));
        app.init_resource::<AgentStrategies>()
            .insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .init_resource::<CapturedNavigateUrls>();

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .insert(ChildOf(pane))
            .id();
        app.world_mut().spawn(Browser).insert(ChildOf(stack));

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);

        app.add_observer(
            |trigger: On<RequestNavigate>, mut captured: ResMut<CapturedNavigateUrls>| {
                captured.0.push(trigger.url.clone());
            },
        );

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: vmux_service::agent_events::CommandOrigin::User,
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: None,
                },
            });

        app.update();
        app.update();

        let captured = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(captured.0, vec!["https://example.com".to_string()]);
    }

    #[test]
    fn browser_navigate_auto_spawns_tab_when_pane_is_empty() {
        use vmux_layout::Browser;

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
            ConsumerPlugin,
        ));
        app.init_resource::<AgentStrategies>()
            .insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedStack>().stack = None;

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: vmux_service::agent_events::CommandOrigin::User,
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: None,
                },
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let mut tabs = world.query_filtered::<&ChildOf, With<vmux_layout::stack::Stack>>();
        let tab_count_under_pane = tabs
            .iter(world)
            .filter(|child_of| child_of.get() == pane)
            .count();
        assert_eq!(
            tab_count_under_pane, 1,
            "browser_navigate should have spawned exactly one tab in the focused pane"
        );

        let mut tab_metadata =
            world.query_filtered::<&PageMetadata, With<vmux_layout::stack::Stack>>();
        let tab_urls: Vec<String> = tab_metadata.iter(world).map(|p| p.url.clone()).collect();
        assert!(
            tab_urls.contains(&"https://example.com".to_string()),
            "tab entity should have PageMetadata with the URL; found {tab_urls:?}"
        );

        let mut browsers = world.query::<(&Browser, &PageMetadata)>();
        let urls: Vec<String> = browsers.iter(world).map(|(_, p)| p.url.clone()).collect();
        assert!(
            urls.contains(&"https://example.com".to_string()),
            "browser entity with the URL should exist; found {urls:?}"
        );
    }

    #[test]
    fn agent_browser_navigate_stacks_new_page_and_waits_for_snapshot() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ConsumerPlugin));
        app.insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        let first_stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::stack_bundle(),
                LastActivatedAt(1),
                ChildOf(pane),
            ))
            .id();
        app.world_mut().spawn((Browser, ChildOf(first_stack)));
        let request_id = [7; 16];
        app.world_mut()
            .resource_mut::<Messages<vmux_layout::BrowserNavigateRequest>>()
            .write(vmux_layout::BrowserNavigateRequest {
                url: "https://second.example".into(),
                pane: Some(pane.to_bits().to_string()),
                request_id: Some(request_id),
                new_stack: true,
                profile: Some("agent-1".into()),
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let mut stacks = world.query_filtered::<
                (Entity, &PageMetadata, &LastActivatedAt),
                With<vmux_layout::stack::Stack>,
            >();
        let second = stacks
            .iter(world)
            .find(|(_, metadata, _)| metadata.url == "https://second.example")
            .map(|(entity, _, activated)| (entity, activated.0))
            .expect("new browser stack");
        assert_ne!(second.0, first_stack);
        assert!(second.1 > 1);
        assert_eq!(world.resource::<PendingNavSnapshots>().0.len(), 1);
        assert_eq!(
            world
                .resource::<PendingNavSnapshots>()
                .0
                .values()
                .next()
                .unwrap()
                .request_id,
            request_id
        );
    }

    #[test]
    fn agent_browser_navigate_does_not_raise_new_stack_during_user_interaction() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ConsumerPlugin));
        app.insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        let first_stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::stack_bundle(),
                LastActivatedAt(10),
                ChildOf(pane),
            ))
            .id();
        app.world_mut().spawn((Browser, ChildOf(first_stack)));
        app.insert_resource(RecentBrowserInteraction {
            stack: Some(first_stack),
            at: Some(std::time::Instant::now()),
        });
        app.world_mut()
            .resource_mut::<Messages<vmux_layout::BrowserNavigateRequest>>()
            .write(vmux_layout::BrowserNavigateRequest {
                url: "https://second.example".into(),
                pane: Some(pane.to_bits().to_string()),
                request_id: None,
                new_stack: true,
                profile: Some("agent-1".into()),
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let mut stacks = world
            .query_filtered::<(&PageMetadata, &LastActivatedAt), With<vmux_layout::stack::Stack>>();
        let activated = stacks
            .iter(world)
            .find(|(metadata, _)| metadata.url == "https://second.example")
            .map(|(_, activated)| activated.0)
            .expect("new browser stack");
        assert_eq!(activated, 0);
    }

    #[test]
    fn browser_navigate_targets_specific_pane_when_id_provided() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
            ConsumerPlugin,
        ));
        app.init_resource::<AgentStrategies>()
            .insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane_a = app.world_mut().spawn(Pane).id();
        let pane_b = app.world_mut().spawn(Pane).id();

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane_a);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: vmux_service::agent_events::CommandOrigin::User,
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: Some(pane_b.to_bits().to_string()),
                },
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let mut tabs = world.query_filtered::<&ChildOf, With<vmux_layout::stack::Stack>>();
        let tabs_in_b = tabs
            .iter(world)
            .filter(|child_of| child_of.get() == pane_b)
            .count();
        let tabs_in_a = tabs
            .iter(world)
            .filter(|child_of| child_of.get() == pane_a)
            .count();
        assert_eq!(tabs_in_b, 1, "tab should be spawned in target pane B");
        assert_eq!(tabs_in_a, 0, "no tab should be spawned in focused pane A");
    }

    #[test]
    fn browser_navigate_with_terminal_url_spawns_terminal_in_focused_pane() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
            ConsumerPlugin,
        ));
        app.init_resource::<AgentStrategies>()
            .insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        let request_id = AgentRequestId::new();

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id,
                origin: vmux_service::agent_events::CommandOrigin::User,
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://terminal/".to_string(),
                    pane: None,
                },
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert!(
            terminal_count >= 1,
            "terminal should be spawned in focused pane"
        );
        assert!(
            world
                .resource::<PendingNavSnapshots>()
                .0
                .values()
                .any(|pending| pending.request_id == request_id.0),
            "terminal navigation should wait for its snapshot"
        );
    }

    #[test]
    fn browser_navigate_with_terminal_url_and_target_pane_uses_target() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
            ConsumerPlugin,
        ));
        app.init_resource::<AgentStrategies>()
            .insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane_a = app.world_mut().spawn(Pane).id();
        let pane_b = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane_a);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: vmux_service::agent_events::CommandOrigin::User,
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://terminal/".to_string(),
                    pane: Some(pane_b.to_bits().to_string()),
                },
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let mut terminals = world.query_filtered::<&ChildOf, With<Terminal>>();
        let term_parents: Vec<Entity> = terminals.iter(world).map(|c| c.get()).collect();
        let mut found_in_b = 0;
        let mut found_in_a = 0;
        for tab in &term_parents {
            if let Some(co) = world.get::<ChildOf>(*tab) {
                if co.get() == pane_b {
                    found_in_b += 1;
                } else if co.get() == pane_a {
                    found_in_a += 1;
                }
            }
        }
        assert_eq!(found_in_b, 1, "terminal should be in target pane B");
        assert_eq!(found_in_a, 0, "no terminal in focused pane A");
    }

    #[test]
    fn browser_navigate_with_unknown_vmux_url_errors() {
        use vmux_layout::Browser;

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
            ConsumerPlugin,
        ));
        app.init_resource::<AgentStrategies>()
            .insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: vmux_service::agent_events::CommandOrigin::User,
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://nonsense/".to_string(),
                    pane: None,
                },
            });

        // One extra update vs. the other navigate tests: the fallback now grants
        // unknown `vmux://` URLs a one-frame grace before rendering the error page.
        app.update();
        app.update();
        app.update();

        let world = app.world_mut();
        let mut browsers = world.query_filtered::<&PageMetadata, With<Browser>>();
        let browser_titles: Vec<String> = browsers
            .iter(world)
            .map(|meta| meta.title.clone())
            .collect();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert_eq!(
            browser_titles,
            vec!["Page not found".to_string()],
            "unknown vmux URL should render an error page"
        );
        assert_eq!(
            terminal_count, 0,
            "no terminal should be spawned for unknown vmux URL"
        );
    }

    #[test]
    fn page_open_error_renders_error_page() {
        use vmux_layout::Browser;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
        app.insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::stack_bundle(),
                vmux_history::LastActivatedAt::now(),
                ChildOf(pane),
            ))
            .id();

        app.world_mut().spawn((
            PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://terminal/bad".to_string(),
                request_id: None,
            },
            PageOpenError {
                message: "malformed terminal URL".to_string(),
            },
        ));

        app.update();
        app.update();

        let world = app.world_mut();
        let mut browsers = world.query_filtered::<&PageMetadata, With<Browser>>();
        let browser_titles: Vec<String> = browsers
            .iter(world)
            .map(|meta| meta.title.clone())
            .collect();
        assert_eq!(
            browser_titles,
            vec!["Page failed to load".to_string()],
            "page handler errors should render an error page"
        );
    }

    #[test]
    fn browser_navigate_with_claude_url_does_not_spawn_standalone_browser() {
        use vmux_layout::Browser;

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
            ConsumerPlugin,
        ));
        app.init_resource::<AgentStrategies>()
            .insert_resource(vmux_agent::plugin::AgentExecutableOverride(
                std::collections::HashMap::from([(vmux_core::agent::AgentKind::Claude, true)]),
            ))
            .insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: vmux_service::agent_events::CommandOrigin::User,
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://agent/claude/cli/".into(),
                    pane: None,
                },
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let standalone_browser_count = world
            .query_filtered::<&Browser, Without<Terminal>>()
            .iter(world)
            .count();
        assert_eq!(
            standalone_browser_count, 0,
            "claude URL should never spawn a standalone browser tab"
        );
    }

    #[test]
    fn browser_navigate_with_codex_url_does_not_spawn_standalone_browser() {
        use vmux_layout::Browser;

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
            ConsumerPlugin,
        ));
        app.init_resource::<AgentStrategies>()
            .insert_resource(vmux_agent::plugin::AgentExecutableOverride(
                std::collections::HashMap::from([(vmux_core::agent::AgentKind::Codex, true)]),
            ))
            .insert_resource(FocusedStack::default())
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: vmux_service::agent_events::CommandOrigin::User,
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://agent/codex/cli/".into(),
                    pane: None,
                },
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let standalone_browser_count = world
            .query_filtered::<&Browser, Without<Terminal>>()
            .iter(world)
            .count();
        assert_eq!(
            standalone_browser_count, 0,
            "codex URL should never spawn a standalone browser tab"
        );
    }
}

mod open_in_place_flow {
    use bevy::ecs::message::Messages;
    use bevy::prelude::*;
    use bevy_cef::prelude::{RequestNavigate, WebviewExtendStandardMaterial};
    use vmux_command::open::OpenCommand;
    use vmux_command::{AppCommand, BrowserCommand, BrowserViewCommand};
    use vmux_core::{PageOpenRequest, PageOpenTarget};
    use vmux_history::LastActivatedAt;
    use vmux_layout::Browser;
    use vmux_layout::pane::Pane;
    use vmux_layout::stack::stack_bundle;
    use vmux_layout::tab::Tab;
    use vmux_terminal::Terminal;

    #[derive(Resource, Default)]
    struct CapturedNavigateUrls(Vec<String>);

    #[derive(Resource, Default)]
    struct CapturedPageOpenRequests(Vec<PageOpenRequest>);

    fn build_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin))
            .add_message::<PageOpenRequest>()
            .add_message::<vmux_terminal::TerminalFontSizeCommand>()
            .add_systems(
                Update,
                (
                    super::super::handle_browser_commands.in_set(vmux_command::ReadAppCommands),
                    capture_page_open_requests.after(vmux_command::ReadAppCommands),
                ),
            )
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .init_resource::<CapturedNavigateUrls>()
            .init_resource::<CapturedPageOpenRequests>()
            .add_observer(
                |trigger: On<RequestNavigate>, mut captured: ResMut<CapturedNavigateUrls>| {
                    captured.0.push(trigger.url.clone());
                },
            );
        for host in [
            "terminal", "agent", "services", "settings", "team", "spaces",
        ] {
            vmux_core::register_host_spawn(&mut app, host);
        }
        app
    }

    fn capture_page_open_requests(
        mut reader: MessageReader<PageOpenRequest>,
        mut captured: ResMut<CapturedPageOpenRequests>,
    ) {
        captured.0.extend(reader.read().cloned());
    }

    fn build_focused_stack(app: &mut App) {
        let space = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(space)))
            .id();
        let stack = app
            .world_mut()
            .spawn(stack_bundle())
            .insert((ChildOf(pane), LastActivatedAt(1)))
            .id();
        app.world_mut().spawn(Browser).insert(ChildOf(stack));
    }

    fn build_focused_terminal_stack(app: &mut App) {
        let space = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(space)))
            .id();
        let stack = app
            .world_mut()
            .spawn(stack_bundle())
            .insert((ChildOf(pane), LastActivatedAt(1)))
            .id();
        app.world_mut()
            .spawn((Browser, Terminal))
            .insert(ChildOf(stack));
    }

    fn build_focused_native_stack(app: &mut App, native_url: &str) {
        let space = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(space)))
            .id();
        let stack = app
            .world_mut()
            .spawn(stack_bundle())
            .insert((ChildOf(pane), LastActivatedAt(1)))
            .id();
        app.world_mut()
            .spawn((
                Browser,
                vmux_core::PageMetadata {
                    url: native_url.to_string(),
                    title: native_url.to_string(),
                    icon: vmux_core::PageIcon::None,
                    bg_color: None,
                },
            ))
            .insert(ChildOf(stack));
    }

    #[test]
    fn in_place_with_explicit_url_triggers_request_navigate() {
        let mut app = build_app();
        build_focused_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPlace {
                    url: Some("https://example.com".into()),
                },
            )));

        app.update();

        let captured = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(captured.0, vec!["https://example.com".to_string()]);
    }

    #[test]
    fn in_place_with_vmux_url_routes_through_page_open() {
        let mut app = build_app();
        build_focused_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPlace {
                    url: Some("vmux://agent/vibe".into()),
                },
            )));

        app.update();

        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert!(navigates.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert_eq!(page_opens.0.len(), 1);
        assert_eq!(page_opens.0[0].url, "vmux://agent/vibe");
        assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
    }

    #[test]
    fn in_place_from_plain_vmux_to_web_navigates_in_place() {
        let mut app = build_app();
        build_focused_native_stack(&mut app, "vmux://history/");

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPlace {
                    url: Some("https://mistral.ai".into()),
                },
            )));

        app.update();

        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert!(page_opens.0.is_empty());
        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(navigates.0, vec!["https://mistral.ai".to_string()]);
    }

    #[test]
    fn in_place_from_web_to_plain_vmux_navigates_in_place() {
        let mut app = build_app();
        build_focused_native_stack(&mut app, "https://example.com/");

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPlace {
                    url: Some("vmux://history/".into()),
                },
            )));

        app.update();

        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert!(page_opens.0.is_empty());
        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(navigates.0, vec!["vmux://history/".to_string()]);
    }

    #[test]
    fn in_place_to_settings_routes_through_page_open() {
        let mut app = build_app();
        build_focused_native_stack(&mut app, "https://example.com/");

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPlace {
                    url: Some("vmux://settings/".into()),
                },
            )));

        app.update();

        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert!(navigates.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert_eq!(page_opens.0.len(), 1);
        assert_eq!(page_opens.0[0].url, "vmux://settings/");
        assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
    }

    #[test]
    fn in_place_to_terminal_routes_through_page_open() {
        let mut app = build_app();
        build_focused_native_stack(&mut app, "vmux://settings/");

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPlace {
                    url: Some("vmux://terminal/".into()),
                },
            )));

        app.update();

        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert!(navigates.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert_eq!(page_opens.0.len(), 1);
        assert_eq!(page_opens.0[0].url, "vmux://terminal/");
        assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
    }

    #[test]
    fn in_place_to_file_routes_through_page_open() {
        let mut app = build_app();
        build_focused_native_stack(&mut app, "https://example.com/");

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPlace {
                    url: Some("file:///tmp/x".into()),
                },
            )));

        app.update();

        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert!(navigates.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert_eq!(page_opens.0.len(), 1);
        assert_eq!(page_opens.0[0].url, "file:///tmp/x");
        assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
    }

    #[test]
    fn in_place_from_terminal_to_web_routes_through_page_open() {
        let mut app = build_app();
        build_focused_terminal_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPlace {
                    url: Some("https://google.com".into()),
                },
            )));

        app.update();

        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert!(navigates.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert_eq!(page_opens.0.len(), 1);
        assert_eq!(page_opens.0[0].url, "https://google.com");
        assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
    }

    #[test]
    fn zoom_in_on_terminal_emits_font_size_increase() {
        use bevy::ecs::message::Messages;

        let mut app = build_app();
        build_focused_terminal_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::View(
                BrowserViewCommand::ZoomIn,
            )));

        app.update();

        let cmds: Vec<vmux_terminal::TerminalFontSizeCommand> = app
            .world_mut()
            .resource_mut::<Messages<vmux_terminal::TerminalFontSizeCommand>>()
            .drain()
            .collect();
        assert_eq!(cmds, vec![vmux_terminal::TerminalFontSizeCommand::Increase]);
    }

    #[test]
    fn zoom_reset_on_terminal_emits_font_size_reset() {
        use bevy::ecs::message::Messages;

        let mut app = build_app();
        build_focused_terminal_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::View(
                BrowserViewCommand::ZoomReset,
            )));

        app.update();

        let cmds: Vec<vmux_terminal::TerminalFontSizeCommand> = app
            .world_mut()
            .resource_mut::<Messages<vmux_terminal::TerminalFontSizeCommand>>()
            .drain()
            .collect();
        assert_eq!(cmds, vec![vmux_terminal::TerminalFontSizeCommand::Reset]);
    }

    #[test]
    fn in_place_with_none_url_uses_startup_setting() {
        let mut app = build_app();
        app.insert_resource(vmux_layout::settings::EffectiveStartupUrl(
            "https://startup.example".into(),
        ));
        build_focused_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPlace { url: None },
            )));

        app.update();

        let captured = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(captured.0, vec!["https://startup.example".to_string()]);
    }

    #[test]
    fn in_place_with_none_url_and_no_startup_does_not_navigate() {
        let mut app = build_app();
        build_focused_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPlace { url: None },
            )));

        app.update();

        let captured = app.world().resource::<CapturedNavigateUrls>();
        assert!(captured.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert!(page_opens.0.is_empty());
    }
}
