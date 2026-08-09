use super::*;
use bevy::ecs::entity::EntityHashMap;
use vmux_layout::settings::{
    FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
};
use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};

#[test]
fn adding_archived_page_marks_store_dirty() {
    let mut app = App::new();
    app.insert_resource(AutoSave {
        debounce: Timer::from_seconds(0.5, TimerMode::Once),
        periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
        dirty: false,
    })
    .add_systems(Update, mark_dirty_on_change);
    app.update();
    app.world_mut().resource_mut::<AutoSave>().dirty = false;
    app.world_mut().spawn(ArchivedPage::default());
    app.update();
    assert!(app.world().resource::<AutoSave>().dirty);
}

#[test]
fn adding_visit_marks_store_dirty() {
    let mut app = App::new();
    app.insert_resource(AutoSave {
        debounce: Timer::from_seconds(0.5, TimerMode::Once),
        periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
        dirty: false,
    })
    .add_systems(Update, mark_dirty_on_change);
    app.update();
    app.world_mut().resource_mut::<AutoSave>().dirty = false;
    app.world_mut().spawn(vmux_history::Visit);
    app.update();
    assert!(app.world().resource::<AutoSave>().dirty);
}

#[test]
fn changing_stack_explorer_visibility_marks_store_dirty() {
    let mut app = App::new();
    app.insert_resource(AutoSave {
        debounce: Timer::from_seconds(0.5, TimerMode::Once),
        periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
        dirty: false,
    })
    .add_systems(Update, mark_dirty_on_change);
    let stack = app
        .world_mut()
        .spawn(vmux_editor::StackExplorerVisibility { visible: false })
        .id();
    app.update();
    app.world_mut().resource_mut::<AutoSave>().dirty = false;
    app.world_mut()
        .get_mut::<vmux_editor::StackExplorerVisibility>(stack)
        .unwrap()
        .visible = true;

    app.update();

    assert!(app.world().resource::<AutoSave>().dirty);
}

#[test]
fn changing_tab_startup_dir_marks_store_dirty() {
    let mut app = App::new();
    app.insert_resource(AutoSave {
        debounce: Timer::from_seconds(0.5, TimerMode::Once),
        periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
        dirty: false,
    })
    .add_systems(Update, mark_dirty_on_change);
    let tab = app.world_mut().spawn(Tab::default()).id();
    app.update();
    app.world_mut().resource_mut::<AutoSave>().dirty = false;
    app.world_mut()
        .entity_mut(tab)
        .get_mut::<Tab>()
        .unwrap()
        .startup_dir = Some("/tmp/rebound".into());

    app.update();

    assert!(app.world().resource::<AutoSave>().dirty);
}

#[test]
fn adding_tab_workspace_marks_store_dirty() {
    let mut app = App::new();
    app.insert_resource(AutoSave {
        debounce: Timer::from_seconds(0.5, TimerMode::Once),
        periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
        dirty: false,
    })
    .add_systems(Update, mark_dirty_on_change);
    let tab = app.world_mut().spawn(Tab::default()).id();
    app.update();
    app.world_mut().resource_mut::<AutoSave>().dirty = false;
    app.world_mut().entity_mut(tab).insert(TabWorkspace {
        project_dir: "/tmp/project".into(),
    });

    app.update();

    assert!(app.world().resource::<AutoSave>().dirty);
}

#[test]
fn removing_tab_worktree_marks_store_dirty() {
    let mut app = App::new();
    app.insert_resource(AutoSave {
        debounce: Timer::from_seconds(0.5, TimerMode::Once),
        periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
        dirty: false,
    })
    .add_systems(Update, mark_dirty_on_change);
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            TabWorktree {
                repo_root: "/tmp/repo".into(),
                checkout_dir: "/tmp/worktree".into(),
                branch: "vmux/test".into(),
                base_ref: "main".into(),
            },
        ))
        .id();
    app.update();
    app.world_mut().resource_mut::<AutoSave>().dirty = false;
    app.world_mut().entity_mut(tab).remove::<TabWorktree>();

    app.update();

    assert!(app.world().resource::<AutoSave>().dirty);
}

#[test]
fn sort_tabs_orders_by_order_field() {
    let a = Entity::from_bits(10);
    let b = Entity::from_bits(11);
    let c = Entity::from_bits(12);
    let input = vec![
        (a, Some(2u32), Some(100i64)),
        (b, Some(0), Some(200)),
        (c, Some(1), Some(50)),
    ];
    assert_eq!(sort_tabs_by_order(input), vec![b, c, a]);
}

#[test]
fn sort_tabs_legacy_falls_back_to_created_at() {
    let a = Entity::from_bits(10);
    let b = Entity::from_bits(11);
    let c = Entity::from_bits(12);
    let input = vec![
        (a, None, Some(2i64)),
        (b, None, Some(3)),
        (c, None, Some(1)),
    ];
    assert_eq!(sort_tabs_by_order(input), vec![c, a, b]);
}

#[test]
fn sort_tabs_ordered_before_unordered() {
    let ordered = Entity::from_bits(1);
    let legacy = Entity::from_bits(2);
    let input = vec![(legacy, None, Some(0i64)), (ordered, Some(5u32), Some(999))];
    assert_eq!(sort_tabs_by_order(input), vec![ordered, legacy]);
}

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct HomeEnvGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
    old_home: Option<std::ffi::OsString>,
    old_tmpdir: Option<std::ffi::OsString>,
}

impl HomeEnvGuard {
    fn use_temp_home(name: &str) -> Self {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old_home = std::env::var_os("HOME");
        let old_tmpdir = std::env::var_os("TMPDIR");
        let home = std::env::temp_dir().join(format!("vmux-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("create temp home");
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("TMPDIR", &home);
        }
        Self {
            _guard: guard,
            old_home,
            old_tmpdir,
        }
    }
}

impl Drop for HomeEnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.old_home {
                Some(home) => std::env::set_var("HOME", home),
                None => std::env::remove_var("HOME"),
            }
            match &self.old_tmpdir {
                Some(tmpdir) => std::env::set_var("TMPDIR", tmpdir),
                None => std::env::remove_var("TMPDIR"),
            }
        }
    }
}

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

#[test]
fn persisted_terminal_tab_reattaches_saved_process() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .init_resource::<vmux_agent::strategy::AgentStrategies>()
        .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
        .add_systems(Update, rebuild_space_views);

    let main = app.world_mut().spawn(Main).id();
    app.world_mut().spawn(PrimaryWindow);
    let space = app.world_mut().spawn((Tab::default(), ChildOf(main))).id();
    let pane = app.world_mut().spawn((Pane, ChildOf(space))).id();
    let saved_url = format!(
        "{}{}",
        TERMINAL_PAGE_URL,
        uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
    );
    let tab = app
        .world_mut()
        .spawn((
            Stack::default(),
            PageMetadata {
                title: "Terminal".to_string(),
                url: saved_url.clone(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            ChildOf(pane),
        ))
        .id();

    app.update();

    let children = app.world().get::<Children>(tab).unwrap();
    let terminal = children
        .iter()
        .find(|entity| app.world().entity(*entity).contains::<Terminal>())
        .unwrap();
    let meta = app.world().get::<PageMetadata>(terminal).unwrap();

    let _ = saved_url;
    assert_eq!(meta.url, TERMINAL_PAGE_URL);
}

#[test]
fn url_and_visit_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test_history.ron");

    let mut app_save = App::new();
    app_save.add_plugins(MinimalPlugins);
    app_save.add_plugins(vmux_core::CorePlugin);
    app_save.add_observer(save_on_default_event);

    let url_e = app_save
        .world_mut()
        .spawn((
            Save,
            vmux_core::Url,
            PageMetadata {
                url: "https://example.com".into(),
                title: "Example".into(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            vmux_core::VisitCount(3),
            vmux_core::LastVisitedAt(1000),
            vmux_core::CreatedAt(500),
        ))
        .id();

    app_save.world_mut().spawn((
        Save,
        vmux_core::Visit,
        vmux_core::VisitedUrl(url_e),
        vmux_core::CreatedAt(900),
        vmux_core::TransitionType::Typed,
    ));

    save_space_to_path(&mut app_save.world_mut().commands(), path.clone());
    app_save.update();

    assert!(path.exists(), "save file should exist");

    let mut app_load = App::new();
    app_load
        .add_plugins((MinimalPlugins, AssetPlugin::default()))
        .add_plugins(vmux_core::CorePlugin)
        .add_observer(load_on_default_event);
    app_load.update();

    app_load
        .world_mut()
        .commands()
        .trigger_load(LoadWorld::default_from_file(path));
    app_load.update();

    let url_count = app_load
        .world_mut()
        .query::<&vmux_core::Url>()
        .iter(app_load.world())
        .count();
    let visit_count = app_load
        .world_mut()
        .query::<&vmux_core::Visit>()
        .iter(app_load.world())
        .count();
    assert_eq!(url_count, 1, "Url not round-tripped");
    assert_eq!(visit_count, 1, "Visit not round-tripped");

    let (vc, lva, ca) = app_load
        .world_mut()
        .query::<(
            &vmux_core::VisitCount,
            &vmux_core::LastVisitedAt,
            &vmux_core::CreatedAt,
        )>()
        .iter(app_load.world())
        .find(|(vc, _, _)| vc.0 == 3)
        .expect("Url entity fields not round-tripped");
    assert_eq!(vc.0, 3);
    assert_eq!(lva.0, 1000);
    assert_eq!(ca.0, 500);

    let tt = app_load
        .world_mut()
        .query::<&vmux_core::TransitionType>()
        .iter(app_load.world())
        .next()
        .expect("TransitionType not round-tripped");
    assert_eq!(*tt, vmux_core::TransitionType::Typed);
}

#[test]
fn stack_explorer_visibility_round_trips_through_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("store.ron");

    let mut app_save = App::new();
    app_save
        .add_plugins(MinimalPlugins)
        .add_plugins(vmux_core::CorePlugin)
        .register_type::<vmux_editor::StackExplorerVisibility>()
        .add_observer(save_on_default_event);
    app_save
        .world_mut()
        .spawn((Save, vmux_editor::StackExplorerVisibility { visible: true }));
    save_space_to_path(&mut app_save.world_mut().commands(), path.clone());
    app_save.update();

    let mut app_load = App::new();
    app_load
        .add_plugins((MinimalPlugins, AssetPlugin::default()))
        .add_plugins(vmux_core::CorePlugin)
        .register_type::<vmux_editor::StackExplorerVisibility>()
        .add_observer(load_on_default_event);
    app_load.update();
    app_load
        .world_mut()
        .commands()
        .trigger_load(LoadWorld::default_from_file(path));
    app_load.update();

    let visibility = app_load
        .world_mut()
        .query::<&vmux_editor::StackExplorerVisibility>()
        .single(app_load.world())
        .expect("stack explorer visibility round-tripped");
    assert!(visibility.visible);
}

#[test]
fn window_geometry_round_trips_through_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("store.ron");

    let mut app_save = App::new();
    app_save.add_plugins(MinimalPlugins);
    app_save.add_plugins(vmux_core::CorePlugin);
    app_save
        .register_type::<WindowGeometry>()
        .register_type::<Option<IVec2>>()
        .register_type::<Option<Vec2>>();
    app_save.add_observer(save_on_default_event);
    app_save.world_mut().spawn((
        Save,
        WindowGeometry {
            fullscreen: true,
            position: Some(IVec2::new(11, 22)),
            size: Some(Vec2::new(640.0, 480.0)),
        },
    ));

    save_space_to_path(&mut app_save.world_mut().commands(), path.clone());
    app_save.update();
    assert!(path.exists(), "store file should exist");

    let mut app_load = App::new();
    app_load
        .add_plugins((MinimalPlugins, AssetPlugin::default()))
        .add_plugins(vmux_core::CorePlugin);
    app_load
        .register_type::<WindowGeometry>()
        .register_type::<Option<IVec2>>()
        .register_type::<Option<Vec2>>();
    app_load.add_observer(load_on_default_event);
    app_load.update();
    app_load
        .world_mut()
        .commands()
        .trigger_load(LoadWorld::default_from_file(path));
    app_load.update();

    let geom = app_load
        .world_mut()
        .query::<&WindowGeometry>()
        .single(app_load.world())
        .expect("WindowGeometry not round-tripped");
    assert!(geom.fullscreen);
    assert_eq!(geom.position, Some(IVec2::new(11, 22)));
    assert_eq!(geom.size, Some(Vec2::new(640.0, 480.0)));
}

#[test]
fn custom_save_writes_schema_version_next_to_saved_store() {
    let _home = HomeEnvGuard::use_temp_home("custom-save-schema-version");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("custom-store.ron");

    let mut app_save = App::new();
    app_save
        .add_plugins(MinimalPlugins)
        .add_plugins(vmux_core::CorePlugin)
        .add_observer(save_on_default_event);
    app_save.world_mut().spawn((
        Save,
        Space,
        SpaceId("space-1".to_string()),
        WindowGeometry {
            fullscreen: false,
            position: None,
            size: None,
        },
    ));

    save_space_to_path(&mut app_save.world_mut().commands(), path.clone());
    app_save.update();

    assert!(path.exists(), "custom store should be saved");
    assert!(
        dir.path().join("store.version").exists(),
        "schema version should be written next to custom store"
    );
    assert!(
        !store_version_path().exists(),
        "custom save must not write default store.version"
    );
}

#[test]
fn pane_id_and_position_round_trip_through_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("store.ron");

    let mut app_save = App::new();
    app_save.add_plugins(MinimalPlugins);
    app_save.add_plugins(vmux_core::CorePlugin);
    app_save.register_type::<PaneId>();
    app_save.add_observer(save_on_default_event);
    app_save
        .world_mut()
        .spawn((Save, Pane, PaneId("p-1".to_string())));
    app_save.world_mut().spawn((
        Save,
        ArchivedPage {
            url: "https://x".into(),
            ..default()
        },
        ArchivedPagePosition {
            leaf_pane_id: "p-1".into(),
            stack_index: 1,
            pane_path: vec![vmux_core::PaneStep {
                split_id: "root".into(),
                axis: vmux_core::SplitAxis::Column,
                child_index: 2,
                flex_weights: vec![1.0, 4.0],
            }],
        },
        ArchivedTabPage {
            group_id: "tab-group".into(),
            tab_name: "Recovered".into(),
            tab_startup_dir: Some("/tmp/recovered".into()),
            active: true,
        },
    ));
    save_space_to_path(&mut app_save.world_mut().commands(), path.clone());
    app_save.update();
    assert!(path.exists());

    let mut app_load = App::new();
    app_load
        .add_plugins((MinimalPlugins, AssetPlugin::default()))
        .add_plugins(vmux_core::CorePlugin)
        .register_type::<PaneId>()
        .add_observer(load_on_default_event);
    app_load.update();
    app_load
        .world_mut()
        .commands()
        .trigger_load(LoadWorld::default_from_file(path));
    app_load.update();

    let pid = app_load
        .world_mut()
        .query::<&PaneId>()
        .single(app_load.world())
        .expect("PaneId round-tripped");
    assert_eq!(pid.0, "p-1");
    let pos = app_load
        .world_mut()
        .query::<&ArchivedPagePosition>()
        .single(app_load.world())
        .expect("position round-tripped");
    assert_eq!(pos.leaf_pane_id, "p-1");
    assert_eq!(pos.pane_path[0].child_index, 2);
    assert!(matches!(
        pos.pane_path[0].axis,
        vmux_core::SplitAxis::Column
    ));
    let tab = app_load
        .world_mut()
        .query::<&ArchivedTabPage>()
        .single(app_load.world())
        .expect("tab archive metadata round-tripped");
    assert_eq!(tab.group_id, "tab-group");
    assert_eq!(tab.tab_name, "Recovered");
    assert_eq!(tab.tab_startup_dir.as_deref(), Some("/tmp/recovered"));
    assert!(tab.active);
}

#[test]
fn runtime_loaded_space_rebuilds_browser_views() {
    let _home = HomeEnvGuard::use_temp_home("runtime-loaded-space-rebuilds-browser-views");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_settings())
        .insert_resource(ActiveSpace {
            record: vmux_space::model::bootstrap_space_record(),
        })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .init_resource::<vmux_agent::strategy::AgentStrategies>()
        .add_plugins(PersistencePlugin);

    let main = app.world_mut().spawn(Main).id();
    app.world_mut().spawn(PrimaryWindow);
    app.update();

    let space = app.world_mut().spawn((Tab::default(), ChildOf(main))).id();
    let pane = app.world_mut().spawn((Pane, ChildOf(space))).id();
    let tab = app
        .world_mut()
        .spawn((
            Stack::default(),
            PageMetadata {
                title: "Example".to_string(),
                url: "https://example.com".to_string(),
                icon: vmux_core::PageIcon::Favicon("https://example.com/favicon.ico".to_string()),
                bg_color: Some("#123456".to_string()),
            },
            ChildOf(pane),
        ))
        .id();

    app.world_mut().trigger(Loaded {
        entity_map: EntityHashMap::default(),
    });
    app.update();

    let children = app.world().get::<Children>(tab).unwrap();
    let browser = children
        .iter()
        .find(|entity| app.world().entity(*entity).contains::<Browser>())
        .expect("browser child");
    let meta = app.world().get::<PageMetadata>(browser).unwrap();
    assert_eq!(meta.title, "Example");
    assert_eq!(meta.url, "https://example.com");
    assert_eq!(
        meta.icon,
        vmux_core::PageIcon::Favicon("https://example.com/favicon.ico".to_string())
    );
    assert_eq!(meta.bg_color.as_deref(), Some("#123456"));
}

#[test]
fn current_page_agent_url_does_not_mark_space_stale() {
    assert!(!space_contains_stale_agent_url(
        r#"url: "vmux://agent/echo/echo/edb5335d-20cf-4c3d-9433-8619c405a0f2""#
    ));
}

#[test]
fn known_cli_agent_url_does_not_mark_space_stale() {
    assert!(!space_contains_stale_agent_url(
        r#"url: "vmux://agent/codex/edb5335d-20cf-4c3d-9433-8619c405a0f2""#
    ));
}

#[test]
fn bare_cli_agent_url_does_not_mark_space_stale() {
    assert!(!space_contains_stale_agent_url(
        r#"url: "vmux://agent/vibe/""#
    ));
}

#[test]
fn malformed_agent_url_marks_space_stale() {
    // Under the ACP grammar `vmux://agent/<id>/<sid>` is a valid session url for any id, so an
    // unknown id is no longer stale-by-parse (the runtime handler errors gracefully for an
    // unconfigured agent). Only genuinely malformed urls (too many segments) are stale.
    assert!(!space_contains_stale_agent_url(
        r#"url: "vmux://agent/bogus/edb5335d-20cf-4c3d-9433-8619c405a0f2""#
    ));
    assert!(space_contains_stale_agent_url(
        r#"url: "vmux://agent/a/b/c/d/e""#
    ));
}

#[test]
fn current_page_agent_space_file_is_not_removed_before_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    let space_dir = dir.path().join("profiles/personal/spaces/space-1");
    std::fs::create_dir_all(&space_dir).expect("space dir");
    let path = space_dir.join("space.ron");
    std::fs::write(
        &path,
        r#"url: "vmux://agent/echo/echo/edb5335d-20cf-4c3d-9433-8619c405a0f2""#,
    )
    .expect("write space");

    assert!(!remove_stale_space_if_needed(&path));
    assert!(path.exists());
    assert!(space_dir.exists());
}

#[test]
fn prompt_only_empty_url_space_is_removed_before_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    let space_dir = dir.path().join("profiles/personal/spaces/space-1");
    std::fs::create_dir_all(&space_dir).expect("space dir");
    let path = space_dir.join("space.ron");
    std::fs::write(
        &path,
        r#"
(
  resources: {},
  entities: {
    1: (
      components: {
        "vmux_desktop::layout::stack::Stack": (
          scroll_x: 0.0,
          scroll_y: 0.0,
        ),
        "vmux_header::system::PageMetadata": (
          title: "",
          url: "",
          icon: None,
          bg_color: None,
        ),
      },
    ),
  },
)
"#,
    )
    .expect("write prompt-only space");

    assert!(remove_stale_space_if_needed(&path));
    assert!(!path.exists());
    assert!(space_dir.exists());
}

fn registry_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(vmux_core::CorePlugin);
    app
}

fn store_body_with_key(key: &str) -> String {
    format!(
        "(\n  resources: {{}},\n  entities: {{\n    1: (\n      components: {{\n        \"{key}\": (),\n      }},\n    ),\n  }},\n)\n"
    )
}

#[test]
fn store_with_unregistered_component_type_is_incompatible() {
    let app = registry_app();
    let registry = app.world().resource::<AppTypeRegistry>().read();
    let body = store_body_with_key("vmux_desktop::ghost::DoesNotExist");
    assert!(space_has_unregistered_types(&body, &registry));
}

#[test]
fn store_with_registered_component_types_is_compatible() {
    let app = registry_app();
    let registry = app.world().resource::<AppTypeRegistry>().read();
    let key = <vmux_core::PageMetadata as bevy::reflect::TypePath>::type_path();
    let body = store_body_with_key(key);
    assert!(!space_has_unregistered_types(&body, &registry));
}

#[test]
fn incompatible_store_is_removed_before_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("store.ron");
    std::fs::write(dir.path().join("store.version"), "2").expect("write version");
    std::fs::write(
        &path,
        store_body_with_key("vmux_desktop::ghost::DoesNotExist"),
    )
    .expect("write store");

    let app = registry_app();
    let registry = app.world().resource::<AppTypeRegistry>().read();
    assert!(remove_incompatible_store_if_needed(&path, &registry));
    assert!(!path.exists());
    assert!(!dir.path().join("store.version").exists());
}

#[test]
fn compatible_store_is_kept_before_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("store.ron");
    let app = registry_app();
    let registry = app.world().resource::<AppTypeRegistry>().read();
    let key = <vmux_core::PageMetadata as bevy::reflect::TypePath>::type_path();
    std::fs::write(&path, store_body_with_key(key)).expect("write store");

    assert!(!remove_incompatible_store_if_needed(&path, &registry));
    assert!(path.exists());
}

#[test]
fn incompatible_store_resets_layout_on_startup() {
    let _home = HomeEnvGuard::use_temp_home("incompatible-store-resets-layout-on-startup");
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("store dir");
    }
    std::fs::write(
        &path,
        store_body_with_key("vmux_desktop::ghost::DoesNotExist"),
    )
    .expect("write store");
    std::fs::write(store_version_path(), STORE_SCHEMA_VERSION.to_string()).expect("write version");

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_settings())
        .insert_resource(ActiveSpace {
            record: vmux_space::model::bootstrap_space_record(),
        })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .init_resource::<vmux_agent::strategy::AgentStrategies>()
        .add_plugins(PersistencePlugin);
    app.world_mut().spawn(Main);
    app.world_mut().spawn(PrimaryWindow);
    app.update();

    assert!(
        !path.exists(),
        "incompatible store should be removed on startup"
    );
    assert!(
        !store_version_path().exists(),
        "store.version should be removed with the incompatible store"
    );
    let spaces = app.world_mut().query::<&Space>().iter(app.world()).count();
    assert_eq!(spaces, 1, "a fresh space should be spawned after reset");
}

#[test]
fn auto_save_system_skips_save_without_space() {
    let _home = HomeEnvGuard::use_temp_home("auto-save-system-skips-without-space");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(vmux_core::CorePlugin)
        .register_type::<WindowGeometry>()
        .register_type::<Option<IVec2>>()
        .register_type::<Option<Vec2>>()
        .insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.0, TimerMode::Once),
            periodic: Timer::from_seconds(0.0, TimerMode::Repeating),
            dirty: true,
        })
        .add_observer(save_on_default_event)
        .add_systems(Update, auto_save_system);
    app.world_mut().spawn((
        Save,
        WindowGeometry {
            fullscreen: false,
            position: None,
            size: None,
        },
    ));
    app.update();
    app.update();
    assert!(
        !store_path().exists(),
        "auto_save must skip when no Space exists"
    );
}

#[test]
fn auto_save_system_saves_with_space() {
    let _home = HomeEnvGuard::use_temp_home("auto-save-system-saves-with-space");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(vmux_core::CorePlugin)
        .register_type::<WindowGeometry>()
        .register_type::<Option<IVec2>>()
        .register_type::<Option<Vec2>>()
        .insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.0, TimerMode::Once),
            periodic: Timer::from_seconds(0.0, TimerMode::Repeating),
            dirty: true,
        })
        .add_observer(save_on_default_event)
        .add_systems(Update, auto_save_system);
    app.world_mut().spawn((
        Save,
        Space,
        SpaceId("space-1".to_string()),
        WindowGeometry {
            fullscreen: false,
            position: None,
            size: None,
        },
    ));
    app.update();
    app.update();
    assert!(
        store_path().exists(),
        "auto_save must save when a Space exists"
    );
}
