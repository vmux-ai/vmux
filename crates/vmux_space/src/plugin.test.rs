use super::*;
use crate::model::{SpaceRecord, bootstrap_profile_name};
use vmux_layout::settings::{
    FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
};
use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};

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

fn work_space_record() -> SpaceRecord {
    SpaceRecord {
        id: "work".to_string(),
        name: "Work".to_string(),
        profile: bootstrap_profile_name(),
    }
}

#[test]
fn registers_spaces_host_before_cef_embedded_hosts_are_read() {
    let mut app = App::new();
    app.add_plugins(SpacePlugin);
    let mut query = app.world_mut().query::<&vmux_core::page::PageManifest>();
    let hosts = bevy_cef_core::prelude::CefEmbeddedHosts(
        query
            .iter(app.world())
            .map(vmux_core::page::PageManifest::embedded_host)
            .collect(),
    );

    let entry = hosts.entry_for_host("spaces").unwrap();
    assert_eq!(entry.default_document, "spaces/index.html");
}

#[test]
fn effective_startup_url_reflects_active_space_override() {
    let mut settings = test_settings();
    settings.browser.startup_url = "https://global.example".into();
    settings.spaces.insert(
        "work".into(),
        vmux_setting::SpaceOverrides {
            startup_url: Some("https://work.example".into()),
            startup_dir: None,
        },
    );

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings)
        .init_resource::<vmux_layout::settings::EffectiveStartupUrl>()
        .insert_resource(ActiveSpace {
            record: work_space_record(),
        })
        .add_systems(Update, update_effective_startup_url);

    app.update();

    assert_eq!(
        app.world()
            .resource::<vmux_layout::settings::EffectiveStartupUrl>()
            .0,
        "https://work.example"
    );
}

#[test]
fn legacy_tab_without_startup_dir_is_not_migrated() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let mut settings = test_settings();
    settings.spaces.insert(
        "work".into(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(first.path().to_string_lossy().into_owned()),
        },
    );
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings)
        .insert_resource(ActiveSpace {
            record: work_space_record(),
        });
    let space = app
        .world_mut()
        .spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("work".into()),
        ))
        .id();
    let tab = app
        .world_mut()
        .spawn((vmux_layout::tab::Tab::default(), ChildOf(space)))
        .id();

    app.update();

    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::Tab>(tab)
            .unwrap()
            .startup_dir
            .as_deref(),
        None
    );
    app.world_mut()
        .resource_mut::<vmux_setting::AppSettings>()
        .spaces
        .get_mut("work")
        .unwrap()
        .startup_dir = Some(second.path().to_string_lossy().into_owned());

    app.update();

    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::Tab>(tab)
            .unwrap()
            .startup_dir
            .as_deref(),
        None
    );
}

#[test]
fn effective_startup_dir_captures_active_space_entity_and_path() {
    let active_dir = tempfile::tempdir().unwrap();
    let inactive_dir = tempfile::tempdir().unwrap();
    let mut settings = test_settings();
    settings.spaces.insert(
        "active".into(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(active_dir.path().to_string_lossy().into_owned()),
        },
    );
    settings.spaces.insert(
        "inactive".into(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(inactive_dir.path().to_string_lossy().into_owned()),
        },
    );
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings)
        .init_resource::<vmux_layout::settings::EffectiveStartupDir>()
        .add_systems(Update, update_effective_startup_dir);
    app.world_mut().spawn((
        vmux_layout::space::Space,
        vmux_layout::space::SpaceId("inactive".into()),
    ));
    let active = app
        .world_mut()
        .spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("active".into()),
            vmux_core::Active,
        ))
        .id();

    app.update();

    assert_eq!(
        app.world()
            .resource::<vmux_layout::settings::EffectiveStartupDir>()
            .0,
        Some((active, Some(active_dir.path().to_path_buf())))
    );
}

#[test]
fn missing_startup_dir_remains_unset() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_settings())
        .init_resource::<vmux_layout::settings::EffectiveStartupDir>()
        .add_systems(Update, update_effective_startup_dir);
    let space = app
        .world_mut()
        .spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("work".into()),
            vmux_core::Active,
        ))
        .id();

    app.update();

    assert_eq!(
        app.world()
            .resource::<vmux_layout::settings::EffectiveStartupDir>()
            .0,
        Some((space, None))
    );
}

#[test]
fn unset_startup_dir_is_unchanged_without_relevant_updates() {
    #[derive(Resource, Default)]
    struct ChangeCount(u32);

    fn count_changes(
        effective: Res<vmux_layout::settings::EffectiveStartupDir>,
        mut count: ResMut<ChangeCount>,
    ) {
        if effective.is_changed() {
            count.0 += 1;
        }
    }

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_settings())
        .init_resource::<vmux_layout::settings::EffectiveStartupDir>()
        .init_resource::<ChangeCount>()
        .add_systems(
            Update,
            (
                update_effective_startup_dir,
                count_changes.after(update_effective_startup_dir),
            ),
        );
    app.world_mut().spawn((
        vmux_layout::space::Space,
        vmux_layout::space::SpaceId("work".into()),
        vmux_core::Active,
    ));

    app.update();
    app.update();

    assert_eq!(app.world().resource::<ChangeCount>().0, 1);
}

#[test]
fn effective_startup_dir_is_unchanged_without_relevant_updates() {
    #[derive(Resource, Default)]
    struct ChangeCount(u32);

    fn count_changes(
        effective: Res<vmux_layout::settings::EffectiveStartupDir>,
        mut count: ResMut<ChangeCount>,
    ) {
        if effective.is_changed() {
            count.0 += 1;
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mut settings = test_settings();
    settings.spaces.insert(
        "work".into(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(dir.path().to_string_lossy().into_owned()),
        },
    );
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings)
        .init_resource::<vmux_layout::settings::EffectiveStartupDir>()
        .init_resource::<ChangeCount>()
        .add_systems(
            Update,
            (
                update_effective_startup_dir,
                count_changes.after(update_effective_startup_dir),
            ),
        );
    app.world_mut().spawn((
        vmux_layout::space::Space,
        vmux_layout::space::SpaceId("work".into()),
        vmux_core::Active,
    ));

    app.update();
    app.update();

    assert_eq!(app.world().resource::<ChangeCount>().0, 1);
}

#[test]
fn effective_startup_dir_re_resolves_when_current_directory_disappears() {
    let primary = tempfile::tempdir().unwrap();
    let fallback = tempfile::tempdir().unwrap();
    let primary_path = primary.path().to_path_buf();
    let mut settings = test_settings();
    settings.terminal = Some(vmux_setting::TerminalSettings {
        startup_dir: Some(fallback.path().to_string_lossy().into_owned()),
        ..Default::default()
    });
    settings.spaces.insert(
        "work".into(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(primary_path.to_string_lossy().into_owned()),
        },
    );
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings)
        .init_resource::<vmux_layout::settings::EffectiveStartupDir>()
        .add_systems(Update, update_effective_startup_dir);
    let space = app
        .world_mut()
        .spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("work".into()),
            vmux_core::Active,
        ))
        .id();

    app.update();
    assert_eq!(
        app.world()
            .resource::<vmux_layout::settings::EffectiveStartupDir>()
            .0,
        Some((space, Some(primary_path)))
    );

    primary.close().unwrap();
    app.update();

    assert_eq!(
        app.world()
            .resource::<vmux_layout::settings::EffectiveStartupDir>()
            .0,
        Some((space, Some(fallback.path().to_path_buf())))
    );
}

#[test]
fn rename_reslugs_space_id_and_retags_tabs() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<TabLayoutSpawnRequest>()
        .init_resource::<vmux_layout::space::ActiveSpaceId>()
        .add_observer(on_space_command);
    app.world_mut().spawn(bevy::window::PrimaryWindow);
    let space = app
        .world_mut()
        .spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("rename-src-test".to_string()),
            Name::new("rename-src-test"),
            vmux_core::Active,
        ))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            vmux_layout::tab::Tab::default(),
            vmux_layout::space::SpaceId("rename-src-test".to_string()),
            vmux_history::LastActivatedAt::now(),
        ))
        .id();

    app.world_mut().trigger(BinReceive {
        webview: Entity::PLACEHOLDER,
        payload: SpaceCommandEvent {
            command: "rename".to_string(),
            space_id: Some("rename-src-test".to_string()),
            name: Some("Vmux Ai/Vmux".to_string()),
        },
    });
    app.update();

    assert_eq!(
        app.world()
            .get::<vmux_layout::space::SpaceId>(space)
            .map(|s| s.0.clone()),
        Some("vmux-ai/vmux".to_string())
    );
    assert_eq!(
        app.world().get::<Name>(space).map(|n| n.to_string()),
        Some("vmux-ai/vmux".to_string())
    );
    assert_eq!(
        app.world()
            .get::<vmux_layout::space::SpaceId>(tab)
            .map(|s| s.0.clone()),
        Some("vmux-ai/vmux".to_string())
    );
    assert_eq!(
        app.world()
            .resource::<vmux_layout::space::ActiveSpaceId>()
            .0
            .as_deref(),
        Some("vmux-ai/vmux")
    );
}
