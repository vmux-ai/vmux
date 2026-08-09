use super::*;
use crate::settings::{
    FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
};
use bevy::ecs::relationship::Relationship;
use bevy_cef::prelude::WebviewExtendStandardMaterial;
use vmux_command::{CommandPlugin, WriteAppCommands};

fn test_settings() -> LayoutSettings {
    LayoutSettings {
        radius: 0.0,
        window: WindowSettings { padding: 0.0 },
        pane: PaneSettings { gap: 0.0 },
        side_sheet: SideSheetSettings::default(),
        focus_ring: FocusRingSettings::default(),
    }
}

#[test]
fn close_stack_request_despawns_target_keeps_siblings() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<CloseStackRequest>()
        .init_resource::<NewStackContext>()
        .add_systems(Update, handle_close_stack_requests);

    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let s1 = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
        .id();
    let s2 = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt(2), ChildOf(pane)))
        .id();

    app.world_mut()
        .resource_mut::<Messages<CloseStackRequest>>()
        .write(CloseStackRequest { stack: s1 });
    app.update();

    assert!(app.world().get_entity(s1).is_err(), "target despawned");
    assert!(app.world().get_entity(s2).is_ok(), "sibling kept");
}

#[test]
fn close_stack_request_keeps_last_stack_in_pane() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<CloseStackRequest>()
        .init_resource::<NewStackContext>()
        .add_systems(Update, handle_close_stack_requests);

    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let only = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
        .id();

    app.world_mut()
        .resource_mut::<Messages<CloseStackRequest>>()
        .write(CloseStackRequest { stack: only });
    app.update();

    assert!(app.world().get_entity(only).is_ok(), "never empties a pane");
}

#[test]
fn focused_stack_not_rewritten_when_focus_is_stable() {
    #[derive(Resource, Default)]
    struct ChangeLog(Vec<bool>);

    fn probe(focused: Res<FocusedStack>, mut log: ResMut<ChangeLog>) {
        log.0.push(focused.is_changed());
    }

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<FocusedStack>()
        .init_resource::<ChangeLog>()
        .add_systems(Update, (compute_focused_stack, probe).chain());

    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)))
        .id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        app.world().resource::<FocusedStack>().stack,
        Some(stack),
        "focus should resolve to the only stack"
    );
    // With stable focus, `FocusedStack` must NOT be marked changed every frame
    // (an unconditional ResMut write drove `sync_live_start_pages` to re-emit
    // the vmux://start payload every frame and eat keystrokes).
    let log = &app.world().resource::<ChangeLog>().0;
    assert_eq!(
        log.last(),
        Some(&false),
        "FocusedStack rewritten on a stable frame; log={log:?}"
    );
}

#[test]
fn closing_last_stack_preloads_fresh_tab_without_workspace_state() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .add_message::<CloseTabRequest>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<crate::tab::LastTabCloseAt>()
        .init_resource::<FocusedStack>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(
            Update,
            (
                handle_stack_commands.in_set(WriteAppCommands),
                crate::archive::handle_close_tab_requests,
                crate::window::spawn_requested_tab_layouts,
            )
                .chain(),
        );

    app.world_mut()
        .spawn((bevy::window::Window::default(), PrimaryWindow));
    let space = app
        .world_mut()
        .spawn((
            crate::space::Space,
            crate::space::SpaceId("s1".to_string()),
            vmux_core::Active,
        ))
        .id();
    let worktree = tempfile::tempdir().unwrap();
    app.insert_resource(crate::settings::EffectiveStartupDir(Some((
        space,
        Some(worktree.path().to_path_buf()),
    ))));
    let tab_e = app
        .world_mut()
        .spawn((
            Tab {
                name: "Worktree".to_string(),
                startup_dir: Some(worktree.path().to_string_lossy().into_owned()),
            },
            crate::tab::TabWorktree {
                repo_root: worktree.path().to_string_lossy().into_owned(),
                checkout_dir: worktree.path().to_string_lossy().into_owned(),
                branch: "test".to_string(),
                base_ref: "main".to_string(),
            },
            crate::tab::TabWorkspace {
                project_dir: worktree.path().to_string_lossy().into_owned(),
            },
            crate::tab::TabDirDecided,
            crate::tab::TabWorktreeUnavailable {
                message: "stale".to_string(),
            },
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt::now(), ChildOf(tab_e)))
        .id();
    let original_stack = app
        .world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close,
        )));

    app.update();

    assert!(app.world().get_entity(tab_e).is_err());
    assert!(app.world().get_entity(original_stack).is_err());
    let replacement_tab = app
        .world_mut()
        .query_filtered::<Entity, With<Tab>>()
        .single(app.world())
        .unwrap();
    assert_ne!(replacement_tab, tab_e);
    assert_eq!(
        app.world().resource::<FocusedStack>().tab,
        Some(replacement_tab)
    );
    assert!(
        app.world()
            .get::<crate::tab::TabWorkspace>(replacement_tab)
            .is_none()
    );
    assert!(
        app.world()
            .get::<crate::tab::TabWorktree>(replacement_tab)
            .is_none()
    );
    assert!(
        app.world()
            .get::<crate::tab::TabDirDecided>(replacement_tab)
            .is_none()
    );
    assert!(
        app.world()
            .get::<crate::tab::TabWorktreeUnavailable>(replacement_tab)
            .is_none()
    );
    assert_eq!(
        app.world().get::<Tab>(replacement_tab).unwrap().startup_dir,
        None
    );
    let ctx = app.world().resource::<NewStackContext>();
    assert!(ctx.needs_open);
    assert!(ctx.stack.is_some());
    assert_eq!(ctx.previous_stack, None);
    let new_pane = app
        .world()
        .get::<ChildOf>(ctx.stack.unwrap())
        .map(Relationship::get)
        .unwrap();
    let split_root = app
        .world()
        .get::<ChildOf>(new_pane)
        .map(Relationship::get)
        .unwrap();
    assert_eq!(
        app.world()
            .get::<ChildOf>(split_root)
            .map(Relationship::get),
        Some(replacement_tab)
    );
}

#[test]
fn closing_last_stack_in_tab_closes_the_tab_when_another_tab_exists() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .add_message::<CloseTabRequest>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<crate::tab::LastTabCloseAt>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(
            Update,
            (
                handle_stack_commands.in_set(WriteAppCommands),
                crate::archive::handle_close_tab_requests,
            )
                .chain(),
        );

    app.world_mut().spawn(PrimaryWindow);
    let root = app.world_mut().spawn_empty().id();
    let remaining_tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(1), ChildOf(root)))
        .id();
    let remaining_pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt(1), ChildOf(remaining_tab)))
        .id();
    app.world_mut().spawn((
        Stack::default(),
        LastActivatedAt(1),
        ChildOf(remaining_pane),
    ));

    let closing_tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(2), ChildOf(root)))
        .id();
    let closing_pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt(2), ChildOf(closing_tab)))
        .id();
    let closing_stack = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt(2), ChildOf(closing_pane)))
        .id();

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close,
        )));

    app.update();

    assert!(app.world().get_entity(closing_tab).is_err());
    assert!(app.world().get_entity(closing_stack).is_err());
    assert!(app.world().get_entity(remaining_tab).is_ok());
    assert!(app.world().get::<LastActivatedAt>(remaining_tab).unwrap().0 > 1);

    let ctx = app.world().resource::<NewStackContext>();
    assert_eq!(ctx.stack, None);
    assert!(!ctx.needs_open);
}

#[test]
fn closing_last_stack_in_active_rightmost_tab_activates_left_neighbor_not_first() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .add_message::<CloseTabRequest>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<crate::tab::LastTabCloseAt>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(
            Update,
            (
                handle_stack_commands.in_set(WriteAppCommands),
                crate::archive::handle_close_tab_requests,
            )
                .chain(),
        );

    app.world_mut().spawn(PrimaryWindow);
    let root = app.world_mut().spawn_empty().id();
    let make_tab = |app: &mut App, ts: i64| -> Entity {
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(ts), ChildOf(root)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(ts), ChildOf(tab)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt(ts), ChildOf(pane)));
        tab
    };
    let first = make_tab(&mut app, 1);
    let middle = make_tab(&mut app, 2);
    let active_rightmost = make_tab(&mut app, 3);

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close,
        )));

    app.update();

    assert!(app.world().get_entity(active_rightmost).is_err());
    let first_ts = app.world().get::<LastActivatedAt>(first).unwrap().0;
    let middle_ts = app.world().get::<LastActivatedAt>(middle).unwrap().0;
    assert_eq!(first_ts, 1, "first tab must not be re-activated");
    assert!(
        middle_ts > first_ts,
        "left neighbor (middle) must become most-recently-activated, not the first tab"
    );
}

#[test]
fn closing_only_stack_in_split_pane_closes_pane() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .add_message::<CloseTabRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<PendingCursorWarp>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_stack_commands.in_set(WriteAppCommands));

    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let split = app
        .world_mut()
        .spawn((
            crate::pane::split_root_bundle(crate::pane::PaneSplitDirection::Row),
            ChildOf(tab),
        ))
        .id();
    let active_pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt(2), ChildOf(split)))
        .id();
    let other_pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt(1), ChildOf(split)))
        .id();
    let original_stack = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt(2), ChildOf(active_pane)))
        .id();
    let other_stack = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt(1), ChildOf(other_pane)))
        .id();

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close,
        )));

    app.update();

    assert!(app.world().get_entity(split).is_ok());
    assert!(app.world().get_entity(active_pane).is_err());
    assert!(app.world().get_entity(other_pane).is_err());
    assert!(app.world().get_entity(original_stack).is_err());
    assert!(app.world().get_entity(other_stack).is_ok());
    assert_eq!(
        app.world()
            .get::<ChildOf>(other_stack)
            .map(Relationship::get),
        Some(split)
    );
    assert!(!app.world().entity(split).contains::<PaneSplit>());
    assert_eq!(app.world().resource::<NewStackContext>().stack, None);
    assert!(!app.world().resource::<NewStackContext>().needs_open);
}

#[test]
fn closing_stack_in_three_way_split_keeps_split_and_does_not_respawn_startup() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .add_message::<CloseTabRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<PendingCursorWarp>()
        .insert_resource(test_settings())
        .insert_resource(crate::settings::EffectiveStartupUrl(
            "vmux://agent/vibe/".to_string(),
        ))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_stack_commands.in_set(WriteAppCommands));

    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let split = app
        .world_mut()
        .spawn((
            crate::pane::split_root_bundle(crate::pane::PaneSplitDirection::Row),
            ChildOf(tab),
        ))
        .id();
    let active_pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt(3), ChildOf(split)))
        .id();
    let p2 = app
        .world_mut()
        .spawn((Pane, LastActivatedAt(2), ChildOf(split)))
        .id();
    let p3 = app
        .world_mut()
        .spawn((Pane, LastActivatedAt(1), ChildOf(split)))
        .id();
    let active_stack = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt(3), ChildOf(active_pane)))
        .id();
    let s2 = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt(2), ChildOf(p2)))
        .id();
    let s3 = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt(1), ChildOf(p3)))
        .id();

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close,
        )));
    app.update();

    assert!(
        app.world().get_entity(active_pane).is_err(),
        "closed terminal pane is despawned"
    );
    assert!(
        app.world().get_entity(active_stack).is_err(),
        "closed terminal stack is despawned"
    );
    assert!(
        app.world().entity(split).contains::<PaneSplit>(),
        "a 3-way split must stay a split after one terminal closes (tree not corrupted)"
    );
    let children: Vec<Entity> = app
        .world()
        .get::<Children>(split)
        .expect("split has children")
        .iter()
        .collect();
    assert_eq!(children, vec![p2, p3], "exactly the two survivors remain");
    assert!(app.world().get_entity(s2).is_ok() && app.world().get_entity(s3).is_ok());
    let mut stacks = app.world_mut().query_filtered::<Entity, With<Stack>>();
    assert_eq!(
        stacks.iter(app.world()).count(),
        2,
        "no replacement startup (Vibe) stack spawned"
    );
    let reqs: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<PageOpenRequest>>()
        .drain()
        .collect();
    assert!(
        reqs.is_empty(),
        "closing a terminal in an N-ary split must not open the startup URL"
    );
}

#[test]
fn empty_active_pane_opens_command_bar_even_when_other_tabs_have_stacks() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<NewStackContext>()
        .add_message::<PageOpenRequest>()
        .add_systems(Update, open_startup_url_if_no_stacks);

    let old_tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(1)))
        .id();
    let old_pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt(1), ChildOf(old_tab)))
        .id();
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt(1), ChildOf(old_pane)));

    let active_tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(2)))
        .id();
    let active_pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt(2), ChildOf(active_tab)))
        .id();

    app.update();

    let ctx = app.world().resource::<NewStackContext>();
    let Some(new_stack) = ctx.stack else {
        panic!("expected empty active pane to get pending stack");
    };
    assert!(ctx.needs_open);
    assert_eq!(
        app.world().get::<ChildOf>(new_stack).map(Relationship::get),
        Some(active_pane)
    );
}

#[test]
fn empty_active_pane_does_not_open_command_bar_when_tab_has_stacks() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<NewStackContext>()
        .add_message::<PageOpenRequest>()
        .add_systems(Update, open_startup_url_if_no_stacks);

    let tab_e = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(1)))
        .id();
    let pane_with_stack = app
        .world_mut()
        .spawn((Pane, LastActivatedAt(1), ChildOf(tab_e)))
        .id();
    app.world_mut().spawn((
        Stack::default(),
        LastActivatedAt(1),
        ChildOf(pane_with_stack),
    ));
    app.world_mut()
        .spawn((Pane, LastActivatedAt(2), ChildOf(tab_e)));

    app.update();

    let ctx = app.world().resource::<NewStackContext>();
    assert_eq!(ctx.stack, None);
    assert!(!ctx.needs_open);
}

#[test]
fn active_empty_stack_does_not_reopen_command_bar() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<NewStackContext>()
        .add_message::<PageOpenRequest>()
        .add_systems(Update, open_startup_url_if_no_stacks);

    let tab_e = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(1)))
        .id();
    let pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt(1), ChildOf(tab_e)))
        .id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
        .id();

    app.update();

    let ctx = app.world().resource::<NewStackContext>();
    assert_ne!(ctx.stack, Some(stack));
    assert!(!ctx.needs_open);
}

#[derive(Resource, Default)]
struct CollectedSpawns(Vec<PageOpenRequest>);

fn collect_spawn_requests(
    mut reader: MessageReader<PageOpenRequest>,
    mut collected: ResMut<CollectedSpawns>,
) {
    for req in reader.read() {
        collected.0.push(req.clone());
    }
}

fn build_app_with_collector() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .add_message::<CloseTabRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<PendingCursorWarp>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .init_resource::<CollectedSpawns>()
        .add_systems(
            Update,
            (
                handle_stack_commands.in_set(WriteAppCommands),
                collect_spawn_requests.after(handle_stack_commands),
            ),
        );
    app
}

fn build_hierarchy(app: &mut App) -> (Entity, Entity, Entity) {
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));
    (tab, pane, stack.id())
}

#[test]
fn closing_last_stack_requests_tab_replacement() {
    let mut app = build_app_with_collector();
    app.insert_resource(crate::settings::EffectiveStartupUrl(
        "https://startup.test".into(),
    ));
    let (tab, pane, original_stack) = build_hierarchy(&mut app);

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close,
        )));

    app.update();

    assert!(app.world().get_entity(original_stack).is_ok());
    assert!(app.world().get_entity(tab).is_ok());
    let ctx = app.world().resource::<NewStackContext>();
    assert_eq!(ctx.stack, None);
    assert!(!ctx.needs_open);

    let collected = app.world().resource::<CollectedSpawns>();
    assert!(collected.0.is_empty());
    let close_requests: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<CloseTabRequest>>()
        .drain()
        .collect();
    assert_eq!(close_requests.len(), 1);
    assert_eq!(close_requests[0].tab, tab);
    assert_eq!(
        app.world()
            .get::<ChildOf>(original_stack)
            .map(Relationship::get),
        Some(pane)
    );
}

#[test]
fn open_in_new_stack_with_explicit_url() {
    let mut app = build_app_with_collector();
    let (_tab, pane, _stack) = build_hierarchy(&mut app);

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InNewStack {
                url: Some("https://example.com".into()),
            },
        )));

    app.update();

    let collected = app.world().resource::<CollectedSpawns>();
    assert_eq!(collected.0.len(), 1, "expected one spawn request");
    match &collected.0[0] {
        PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url,
            ..
        } => {
            assert_eq!(url, "https://example.com");
            assert_eq!(
                app.world().get::<ChildOf>(*stack).map(Relationship::get),
                Some(pane),
            );
        }
        other => panic!("expected PageOpenRequest, got {other:?}"),
    }
}

#[test]
fn open_in_new_stack_none_url_queues_empty_stack_for_command_bar() {
    let mut app = build_app_with_collector();
    let (_tab, pane, _stack) = build_hierarchy(&mut app);

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InNewStack { url: None },
        )));

    app.update();

    let collected = app.world().resource::<CollectedSpawns>();
    assert!(
        collected.0.is_empty(),
        "no spawn request until URL is provided"
    );
    let ctx = app.world().resource::<NewStackContext>();
    let queued = ctx.stack.expect("an empty stack should be queued");
    assert_eq!(
        app.world().get::<ChildOf>(queued).map(Relationship::get),
        Some(pane),
    );
    assert!(ctx.needs_open, "command bar should be requested");
}

#[test]
fn in_new_stack_with_no_url_uses_startup_url() {
    let mut app = build_app_with_collector();
    app.insert_resource(crate::settings::EffectiveStartupUrl(
        "https://startup.test".into(),
    ));
    let (_tab, _pane, _stack) = build_hierarchy(&mut app);

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InNewStack { url: None },
        )));

    app.update();

    let collected = app.world().resource::<CollectedSpawns>();
    assert_eq!(collected.0.len(), 1);
    assert_eq!(collected.0[0].url, "https://startup.test");
}

#[test]
fn active_tab_param_picks_active_space_tab_not_global_max() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = App::new();
    let main = app.world_mut().spawn(crate::window::Main).id();
    let space_a = app
        .world_mut()
        .spawn((crate::space::Space, ChildOf(main)))
        .id();
    let _tab_a = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt(100),
            ChildOf(space_a),
        ))
        .id();
    let space_b = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active, ChildOf(main)))
        .id();
    let tab_b = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt(1),
            ChildOf(space_b),
        ))
        .id();

    let got = app
        .world_mut()
        .run_system_once(|param: ActiveTabParam| param.get())
        .unwrap();

    assert_eq!(got, Some(tab_b));
}

#[test]
fn active_tab_param_falls_back_to_global_when_no_scoped_active_tab() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = App::new();
    let main = app.world_mut().spawn(crate::window::Main).id();
    // An active space exists, but the only tab isn't scoped to it — the
    // fresh-start state where the default tab is parented under Main before
    // it is adopted into / marked active within its space.
    app.world_mut()
        .spawn((crate::space::Space, vmux_core::Active, ChildOf(main)));
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(5), ChildOf(main)))
        .id();

    let got = app
        .world_mut()
        .run_system_once(|param: ActiveTabParam| param.get())
        .unwrap();

    assert_eq!(
        got,
        Some(tab),
        "must fall back to the global tab so the layout isn't treated as empty (else startup respawns forever)"
    );
}
