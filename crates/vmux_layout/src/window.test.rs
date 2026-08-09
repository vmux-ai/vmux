use super::*;
use crate::cef::LayoutCef;
use bevy::ecs::relationship::Relationship;
use bevy::window::Monitor;
use bevy_cef::prelude::WebviewExtendStandardMaterial;

#[test]
fn scaffold_builds_tab_pane_stack_under_space() {
    use bevy::ecs::system::SystemState;
    let mut app = App::new();
    let space = app.world_mut().spawn(crate::space::Space).id();
    let window = app.world_mut().spawn_empty().id();
    let result = {
        let world = app.world_mut();
        let mut state = SystemState::<Commands>::new(world);
        let mut commands = state.get_mut(world).unwrap();
        let r = spawn_tab_scaffold_in_space(&mut commands, space, window, 8.0);
        state.apply(world);
        r
    };
    assert!(app.world().get::<crate::tab::Tab>(result.tab).is_some());
    assert!(app.world().get::<crate::pane::Pane>(result.pane).is_some());
    assert!(
        app.world()
            .get::<crate::stack::Stack>(result.stack)
            .is_some()
    );
    assert_eq!(app.world().get::<ChildOf>(result.tab).unwrap().get(), space);
}

static HOME_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct HomeEnvGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
    old_home: Option<std::ffi::OsString>,
}

impl HomeEnvGuard {
    fn use_temp_home(name: &str) -> Self {
        let guard = HOME_ENV_LOCK.lock().expect("home env lock");
        let old_home = std::env::var_os("HOME");
        let home = std::env::temp_dir().join(format!("vmux-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("create temp home");
        unsafe {
            std::env::set_var("HOME", &home);
        }
        Self {
            _guard: guard,
            old_home,
        }
    }
}

impl Drop for HomeEnvGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(home) = &self.old_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }
}

#[cfg(feature = "player-mode")]
#[test]
fn window_uses_dark_finder_style_background() {
    assert_eq!(
        window_background_color(),
        Color::srgba(0.13, 0.13, 0.14, 1.0)
    );
}

#[cfg(feature = "player-mode")]
#[test]
fn window_surface_is_transparent_in_user_mode() {
    assert_eq!(
        window_surface_alpha(crate::scene::InteractionMode::User),
        0.0
    );
}

#[cfg(feature = "player-mode")]
#[test]
fn window_surface_is_opaque_in_player_mode() {
    assert_eq!(
        window_surface_alpha(crate::scene::InteractionMode::Player),
        1.0
    );
}

#[cfg(feature = "player-mode")]
#[test]
fn window_background_material_is_opaque_in_player_mode() {
    let material = window_background_material(
        0.0,
        Vec2::new(4.0, 3.0),
        crate::scene::InteractionMode::Player,
    );

    assert_eq!(material.base.base_color.alpha(), 1.0);
    assert_eq!(material.base.alpha_mode, AlphaMode::Opaque);
    assert_eq!(material.base.cull_mode, None);
    assert_eq!(material.base.specular_transmission, 0.0);
    assert_eq!(material.base.diffuse_transmission, 0.0);
}

#[cfg(feature = "player-mode")]
#[test]
fn window_background_material_alpha_to_coverage_for_rounded_player_corners() {
    let material = window_background_material(
        12.0,
        Vec2::new(4.0, 3.0),
        crate::scene::InteractionMode::Player,
    );

    assert_eq!(material.base.base_color.alpha(), 1.0);
    assert_eq!(material.base.alpha_mode, AlphaMode::AlphaToCoverage);
}

#[cfg(feature = "player-mode")]
#[test]
fn sync_window_surface_alpha_preserves_rounded_player_corner_alpha_to_coverage() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(crate::scene::InteractionMode::User)
        .init_resource::<Assets<WindowMaterial>>()
        .add_systems(Update, sync_window_surface_alpha);
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WindowMaterial>>()
        .add(window_background_material(
            12.0,
            Vec2::new(4.0, 3.0),
            crate::scene::InteractionMode::User,
        ));
    app.world_mut()
        .spawn((WindowSurface, MeshMaterial3d(handle.clone())));

    let mut mode = app
        .world_mut()
        .resource_mut::<crate::scene::InteractionMode>();
    *mode = crate::scene::InteractionMode::Player;

    app.update();

    let material = app
        .world()
        .resource::<Assets<WindowMaterial>>()
        .get(&handle)
        .expect("window material");

    assert_eq!(material.base.base_color.alpha(), 1.0);
    assert_eq!(material.base.alpha_mode, AlphaMode::AlphaToCoverage);
}

#[cfg(feature = "player-mode")]
#[test]
fn window_background_material_is_transparent_in_user_mode() {
    let material = window_background_material(
        12.0,
        Vec2::new(4.0, 3.0),
        crate::scene::InteractionMode::User,
    );

    assert_eq!(material.base.base_color.alpha(), 0.0);
    assert_eq!(material.base.alpha_mode, AlphaMode::Blend);
}

#[cfg(feature = "player-mode")]
#[test]
fn window_background_material_keeps_corner_clip() {
    let material = window_background_material(
        12.0,
        Vec2::new(4.0, 3.0),
        crate::scene::InteractionMode::Player,
    );

    assert_eq!(
        material.extension.clip,
        Vec4::new(12.0, 4.0, 3.0, PIXELS_PER_METER)
    );
    assert_eq!(material.extension.corner_mode, Vec4::ZERO);
}

#[cfg(feature = "player-mode")]
#[test]
fn apply_webview_material_defaults_renders_from_both_sides() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, apply_webview_material_defaults);
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WebviewExtendStandardMaterial>>()
        .add(WebviewExtendStandardMaterial::default());
    app.world_mut().spawn((
        WebviewSource::new("https://example.com/"),
        WebviewMaterialHandle(handle.clone()),
    ));
    app.update();

    let material = app
        .world()
        .resource::<Assets<WebviewExtendStandardMaterial>>()
        .get(&handle)
        .expect("webview material");

    assert_eq!(material.base.alpha_mode, AlphaMode::Blend);
    assert_eq!(material.base.depth_bias, WEBVIEW_MESH_DEPTH_BIAS);
    assert_eq!(material.base.cull_mode, None);
}

#[cfg(feature = "player-mode")]
#[test]
fn transparent_webview_material_uses_straight_alpha() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, apply_webview_material_defaults);
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WebviewExtendStandardMaterial>>()
        .add(WebviewExtendStandardMaterial::default());
    app.world_mut().spawn((
        WebviewSource::new("https://example.com/"),
        WebviewTransparent,
        WebviewMaterialHandle(handle.clone()),
    ));
    app.update();

    let material = app
        .world()
        .resource::<Assets<WebviewExtendStandardMaterial>>()
        .get(&handle)
        .expect("webview material");

    assert_eq!(material.base.alpha_mode, AlphaMode::Blend);
}

fn test_settings(gap: f32) -> LayoutSettings {
    LayoutSettings {
        radius: 0.0,
        window: crate::settings::WindowSettings { padding: 0.0 },
        pane: crate::settings::PaneSettings { gap },
        side_sheet: crate::settings::SideSheetSettings::default(),
        focus_ring: crate::settings::FocusRingSettings::default(),
    }
}

fn setup_window_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(crate::scene::InteractionMode::User)
        .insert_resource(test_settings(8.0))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WindowMaterial>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>();
    app.world_mut().spawn((
        Window {
            resolution: (1200, 800).into(),
            ..default()
        },
        PrimaryWindow,
    ));
    app.world_mut().spawn(crate::scene::MainCamera);
    app.add_systems(Startup, setup);
    app
}

#[test]
fn header_lives_in_main_column_above_main() {
    let mut app = setup_window_app();
    app.update();

    let header = app
        .world_mut()
        .query_filtered::<Entity, With<Header>>()
        .single(app.world())
        .expect("header");
    let main_col = app
        .world_mut()
        .query_filtered::<Entity, With<MainColumn>>()
        .single(app.world())
        .expect("main column");
    let parent = app
        .world()
        .get::<ChildOf>(header)
        .map(Relationship::get)
        .expect("header parent");

    assert_eq!(parent, main_col);
}

#[test]
fn setup_spawns_one_window_surface() {
    let mut app = setup_window_app();
    app.update();

    let count = app
        .world_mut()
        .query_filtered::<Entity, With<WindowSurface>>()
        .iter(app.world())
        .count();

    assert_eq!(count, 1);
}

#[test]
fn setup_window_gap_matches_header_layout_gap() {
    let mut app = setup_window_app();
    app.update();

    let root = app
        .world_mut()
        .query_filtered::<Entity, With<VmuxWindow>>()
        .single(app.world())
        .expect("window root");
    let node = app.world().get::<Node>(root).expect("window node");

    assert_eq!(node.column_gap, Val::Px(crate::event::PANE_GAP_PX));
}

#[test]
fn command_bar_modal_backend_is_mode_driven() {
    let mut app = setup_window_app();
    app.update();

    let modal = app
        .world_mut()
        .query_filtered::<Entity, With<Modal>>()
        .single(app.world())
        .expect("modal");

    assert!(app.world().get::<WebviewWindowed>(modal).is_none());
}

#[test]
fn layout_uses_transparent_osr_surface() {
    let mut app = setup_window_app();
    app.update();

    let layout_shell = app
        .world_mut()
        .query_filtered::<Entity, With<LayoutCef>>()
        .single(app.world())
        .expect("layout shell");
    let modal = app
        .world_mut()
        .query_filtered::<Entity, With<Modal>>()
        .single(app.world())
        .expect("modal");

    assert!(
        app.world()
            .get::<WebviewOpaqueWindowedBackground>(layout_shell)
            .is_none()
    );
    assert!(app.world().get::<WebviewWindowed>(layout_shell).is_none());
    assert!(
        app.world()
            .get::<WebviewTransparent>(layout_shell)
            .is_some()
    );
    assert!(
        app.world()
            .get::<WebviewNativeOverlay>(layout_shell)
            .is_none()
    );
    assert_eq!(
        app.world()
            .get::<WebviewMaxFrameRate>(layout_shell)
            .map(|rate| rate.0),
        Some(30)
    );
    // The modal renders OSR through a transparent surface, so no opaque background override.
    assert!(app.world().get::<WebviewTransparent>(modal).is_some());
    assert!(
        app.world()
            .get::<WebviewOpaqueWindowedBackground>(modal)
            .is_none()
    );
    assert!(app.world().get::<WebviewNativeLiquidGlass>(modal).is_some());
}

#[test]
fn command_bar_modal_allows_windowed_native_focus() {
    let mut app = setup_window_app();
    app.update();

    let modal = app
        .world_mut()
        .query_filtered::<Entity, With<Modal>>()
        .single(app.world())
        .expect("modal");

    assert!(
        app.world()
            .get::<WebviewWindowedNativeFocus>(modal)
            .is_some()
    );
}

#[test]
fn default_tab_requests_command_bar_open() {
    let _home = HomeEnvGuard::use_temp_home("default-tab");
    let startup_dir = tempfile::tempdir().unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<crate::NewStackContext>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
        .insert_resource(LayoutSettings {
            radius: 0.0,
            window: crate::settings::WindowSettings { padding: 0.0 },
            pane: crate::settings::PaneSettings { gap: 0.0 },
            side_sheet: crate::settings::SideSheetSettings::default(),
            focus_ring: crate::settings::FocusRingSettings::default(),
        })
        .add_systems(
            Update,
            (request_default_layout, spawn_requested_tab_layouts).chain(),
        );

    app.world_mut().spawn(PrimaryWindow);
    let main = app.world_mut().spawn(Main).id();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, ChildOf(main)))
        .id();
    app.insert_resource(crate::settings::EffectiveStartupDir(Some((
        space,
        Some(startup_dir.path().to_path_buf()),
    ))));

    app.update();

    let ctx = app.world().resource::<crate::NewStackContext>();
    assert!(ctx.stack.is_some());
    assert!(ctx.needs_open);
}

#[test]
fn default_tab_stores_workspace_directory() {
    let _home = HomeEnvGuard::use_temp_home("default-tab-workspace");
    let startup_dir = tempfile::tempdir().unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<crate::NewStackContext>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
        .insert_resource(test_settings(0.0))
        .add_systems(
            Update,
            (request_default_layout, spawn_requested_tab_layouts).chain(),
        );

    app.world_mut().spawn(PrimaryWindow);
    let main = app.world_mut().spawn(Main).id();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, ChildOf(main)))
        .id();
    app.insert_resource(crate::settings::EffectiveStartupDir(Some((
        space,
        Some(startup_dir.path().to_path_buf()),
    ))));

    app.update();

    let tab = app.world_mut().query::<&Tab>().single(app.world()).unwrap();
    assert_eq!(
        tab.startup_dir.as_deref(),
        startup_dir.path().canonicalize().unwrap().to_str()
    );
}

#[test]
fn default_tab_without_configured_startup_dir_has_no_workspace() {
    let _home = HomeEnvGuard::use_temp_home("default-tab-no-workspace");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<crate::NewStackContext>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
        .insert_resource(test_settings(0.0))
        .add_systems(
            Update,
            (request_default_layout, spawn_requested_tab_layouts).chain(),
        );

    app.world_mut().spawn(PrimaryWindow);
    let main = app.world_mut().spawn(Main).id();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, ChildOf(main)))
        .id();
    app.insert_resource(crate::settings::EffectiveStartupDir(Some((space, None))));

    app.update();

    let tab = app.world_mut().query::<&Tab>().single(app.world()).unwrap();
    assert_eq!(tab.startup_dir, None);
}

#[test]
fn tab_request_with_missing_startup_dir_spawns_without_workspace() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<crate::NewStackContext>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
        .insert_resource(test_settings(0.0))
        .add_systems(Update, spawn_requested_tab_layouts);
    let window = app.world_mut().spawn(PrimaryWindow).id();
    let main = app.world_mut().spawn(Main).id();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, ChildOf(main)))
        .id();
    app.world_mut()
        .resource_mut::<Messages<crate::TabLayoutSpawnRequest>>()
        .write(crate::TabLayoutSpawnRequest {
            space,
            primary_window: window,
            name: None,
            startup_dir: Some(root.path().join("missing")),
            content: crate::TabLayoutSpawnContent::StartupUrlOrPrompt,
            clear_pending_stack: false,
            focus: true,
        });

    app.update();

    let tab = app.world_mut().query::<&Tab>().single(app.world()).unwrap();
    assert_eq!(tab.startup_dir, None);
}

#[test]
fn tab_request_keeps_space_active_when_request_was_created() {
    let startup_dir = tempfile::tempdir().unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<crate::NewStackContext>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
        .insert_resource(test_settings(0.0))
        .add_systems(Update, spawn_requested_tab_layouts);
    let window = app.world_mut().spawn(PrimaryWindow).id();
    let main = app.world_mut().spawn(Main).id();
    let requested_space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active, ChildOf(main)))
        .id();
    let later_space = app
        .world_mut()
        .spawn((crate::space::Space, ChildOf(main)))
        .id();
    app.world_mut()
        .resource_mut::<Messages<crate::TabLayoutSpawnRequest>>()
        .write(crate::TabLayoutSpawnRequest {
            space: requested_space,
            primary_window: window,
            name: None,
            startup_dir: Some(startup_dir.path().to_path_buf()),
            content: crate::TabLayoutSpawnContent::StartupUrlOrPrompt,
            clear_pending_stack: false,
            focus: true,
        });
    app.world_mut()
        .entity_mut(requested_space)
        .remove::<vmux_core::Active>();
    app.world_mut()
        .entity_mut(later_space)
        .insert(vmux_core::Active);

    app.update();

    let tab = app
        .world_mut()
        .query_filtered::<Entity, With<Tab>>()
        .single(app.world())
        .unwrap();
    assert_eq!(
        app.world()
            .get::<ChildOf>(tab)
            .map(|parent| parent.parent()),
        Some(requested_space)
    );
}

#[test]
fn cold_start_seeds_exactly_one_default_tab() {
    let _home = HomeEnvGuard::use_temp_home("cold-start-one-tab");
    let startup_dir = tempfile::tempdir().unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<crate::NewStackContext>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
        .insert_resource(LayoutSettings {
            radius: 0.0,
            window: crate::settings::WindowSettings { padding: 0.0 },
            pane: crate::settings::PaneSettings { gap: 0.0 },
            side_sheet: crate::settings::SideSheetSettings::default(),
            focus_ring: crate::settings::FocusRingSettings::default(),
        })
        .insert_resource(crate::settings::EffectiveStartupUrl(
            "vmux://agent/vibe/".to_string(),
        ))
        .add_systems(
            Startup,
            (
                request_default_layout,
                spawn_requested_tab_layouts,
                discard_startup_tab_layout_requests,
            )
                .chain(),
        )
        .add_systems(Update, spawn_requested_tab_layouts);

    app.world_mut().spawn(PrimaryWindow);
    let main = app.world_mut().spawn(Main).id();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, ChildOf(main)))
        .id();
    app.insert_resource(crate::settings::EffectiveStartupDir(Some((
        space,
        Some(startup_dir.path().to_path_buf()),
    ))));

    app.update();

    let mut tabs = app.world_mut().query_filtered::<Entity, With<Tab>>();
    assert_eq!(
        tabs.iter(app.world()).count(),
        1,
        "cold start must seed exactly one default tab; the Startup-written request must not be re-read by the Update consumer"
    );
}

#[test]
fn default_tab_adopts_existing_space_when_none_active() {
    use bevy::ecs::relationship::Relationship;
    let _home = HomeEnvGuard::use_temp_home("default-tab-adopts-space");
    let startup_dir = tempfile::tempdir().unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<crate::NewStackContext>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
        .insert_resource(LayoutSettings {
            radius: 0.0,
            window: crate::settings::WindowSettings { padding: 0.0 },
            pane: crate::settings::PaneSettings { gap: 0.0 },
            side_sheet: crate::settings::SideSheetSettings::default(),
            focus_ring: crate::settings::FocusRingSettings::default(),
        })
        .insert_resource(crate::settings::EffectiveStartupUrl(
            "vmux://agent/vibe/".to_string(),
        ))
        .add_systems(
            Startup,
            (request_default_layout, spawn_requested_tab_layouts).chain(),
        );

    app.world_mut().spawn(Main);
    app.world_mut().spawn(PrimaryWindow);
    // Fresh start: a space exists but isn't Active yet (ensure_active runs in
    // Update, after this Startup). The default tab must still be adopted into
    // the space so it becomes active + visible — not orphaned under Main.
    let space = app.world_mut().spawn(crate::space::Space).id();
    app.insert_resource(crate::settings::EffectiveStartupDir(Some((
        space,
        Some(startup_dir.path().to_path_buf()),
    ))));

    app.update();

    let mut tabs = app.world_mut().query_filtered::<&ChildOf, With<Tab>>();
    let child_of = tabs
        .iter(app.world())
        .next()
        .expect("a default tab should be spawned");
    assert_eq!(
        child_of.get(),
        space,
        "default tab must be parented under the existing space, not Main"
    );
}

#[test]
fn window_padding_tracks_layout_window_settings() {
    let source = include_str!("window.rs");
    let sync_fn = source
        .split("fn sync_window_layout_to_settings")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_main_column_gap_to_pane_count").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("settings.window.pad_top()"));
    assert!(sync_fn.contains("settings.window.pad_right()"));
    assert!(sync_fn.contains("settings.window.pad_bottom()"));
    assert!(sync_fn.contains("settings.window.pad_left()"));
}

#[test]
fn visible_fills_monitor_window_sync_clears_top_left_padding() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(crate::toggle::LayoutHidden(false))
        .insert_resource(LayoutSettings {
            radius: 0.0,
            window: crate::settings::WindowSettings { padding: 16.0 },
            pane: crate::settings::PaneSettings { gap: 0.0 },
            side_sheet: crate::settings::SideSheetSettings::default(),
            focus_ring: crate::settings::FocusRingSettings::default(),
        })
        .insert_resource(SideSheetWidth(0.0))
        .add_systems(Update, sync_window_layout_to_settings);
    app.world_mut().spawn((
        Window {
            resolution: (1200, 800).into(),
            ..default()
        },
        PrimaryWindow,
    ));
    app.world_mut().spawn(Monitor {
        name: None,
        physical_width: 1200,
        physical_height: 800,
        physical_position: IVec2::ZERO,
        refresh_rate_millihertz: None,
        scale_factor: 1.0,
        video_modes: Vec::new(),
    });
    let root = app.world_mut().spawn((VmuxWindow, Node::default())).id();

    app.update();

    let node = app.world().get::<Node>(root).expect("window node");
    assert_eq!(node.padding.top, Val::Px(0.0));
    assert_eq!(node.padding.left, Val::Px(0.0));
    assert_eq!(node.padding.right, Val::Px(16.0));
    assert_eq!(node.padding.bottom, Val::Px(16.0));
}

#[test]
fn window_geometry_round_trips_position_size_fullscreen() {
    let g = WindowGeometry {
        fullscreen: true,
        position: Some(IVec2::new(100, 200)),
        size: Some(Vec2::new(1280.0, 800.0)),
    };
    assert_eq!(g, g);
    assert_eq!(g.position, Some(IVec2::new(100, 200)));
    assert_eq!(g.size, Some(Vec2::new(1280.0, 800.0)));
    assert!(g.fullscreen);
    assert_eq!(
        WindowGeometry::default(),
        WindowGeometry {
            fullscreen: false,
            position: None,
            size: None,
        }
    );
}
