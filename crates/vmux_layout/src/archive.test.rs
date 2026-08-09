use super::*;
use bevy::ecs::relationship::Relationship;
use vmux_core::terminal::TerminalKind;

fn page(url: &str, closed_at: i64) -> ArchivedPage {
    ArchivedPage {
        url: url.to_string(),
        title: String::new(),
        space_id: "s".to_string(),
        closed_at,
        launch: None,
        tab_index: None,
    }
}

#[test]
fn capture_spawns_archived_page() {
    let mut app = App::new();
    app.add_message::<PageArchiveRequest>()
        .add_systems(Update, capture_archived_pages);
    app.world_mut()
        .resource_mut::<Messages<PageArchiveRequest>>()
        .write(PageArchiveRequest {
            url: "https://a.example".to_string(),
            title: "A".to_string(),
            space_id: "s".to_string(),
            launch: None,
            tab_index: None,
            leaf_pane_id: String::new(),
            stack_index: 0,
            pane_path: Vec::new(),
        });
    app.update();
    let mut q = app.world_mut().query::<&ArchivedPage>();
    let all: Vec<_> = q.iter(app.world()).collect();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].url, "https://a.example");
}

#[test]
fn capture_skips_empty_url() {
    let mut app = App::new();
    app.add_message::<PageArchiveRequest>()
        .add_systems(Update, capture_archived_pages);
    app.world_mut()
        .resource_mut::<Messages<PageArchiveRequest>>()
        .write(PageArchiveRequest {
            url: String::new(),
            title: String::new(),
            space_id: "s".to_string(),
            launch: None,
            tab_index: None,
            leaf_pane_id: String::new(),
            stack_index: 0,
            pane_path: Vec::new(),
        });
    app.update();
    let mut q = app.world_mut().query::<&ArchivedPage>();
    assert_eq!(q.iter(app.world()).count(), 0);
}

#[test]
fn capture_spawns_position_component() {
    let mut app = App::new();
    app.add_message::<PageArchiveRequest>()
        .add_systems(Update, capture_archived_pages);
    app.world_mut()
        .resource_mut::<Messages<PageArchiveRequest>>()
        .write(PageArchiveRequest {
            url: "https://a.example".to_string(),
            title: "A".to_string(),
            space_id: "s".to_string(),
            launch: None,
            tab_index: Some(0),
            leaf_pane_id: "leaf-1".to_string(),
            stack_index: 2,
            pane_path: vec![vmux_core::PaneStep {
                split_id: "root".to_string(),
                axis: vmux_core::SplitAxis::Row,
                child_index: 1,
                flex_weights: vec![1.0, 2.0],
            }],
        });
    app.update();
    let mut q = app
        .world_mut()
        .query::<(&ArchivedPage, &ArchivedPagePosition)>();
    let (page, pos) = q.single(app.world()).expect("archived page + position");
    assert_eq!(page.url, "https://a.example");
    assert_eq!(pos.leaf_pane_id, "leaf-1");
    assert_eq!(pos.stack_index, 2);
    assert_eq!(pos.pane_path.len(), 1);
    assert_eq!(pos.pane_path[0].child_index, 1);
}

#[test]
fn maintain_enforces_cap_dropping_oldest() {
    let mut app = App::new();
    app.add_systems(Update, maintain_archive);
    let now = now_millis();
    for i in 0..(MAX_ARCHIVE_ENTRIES as i64 + 1) {
        app.world_mut().spawn(page(&format!("u{i}"), now - i));
    }
    app.update();
    let mut q = app.world_mut().query::<&ArchivedPage>();
    let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
    assert_eq!(urls.len(), MAX_ARCHIVE_ENTRIES);
    let oldest = format!("u{}", MAX_ARCHIVE_ENTRIES);
    assert!(!urls.contains(&oldest));
}

#[test]
fn maintain_purges_expired() {
    let mut app = App::new();
    app.add_systems(Update, maintain_archive);
    let now = now_millis();
    app.world_mut().spawn(page("fresh", now));
    app.world_mut()
        .spawn(page("stale", now - ARCHIVE_TTL_MS - 1));
    app.update();
    let mut q = app.world_mut().query::<&ArchivedPage>();
    let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
    assert_eq!(urls, vec!["fresh".to_string()]);
}

fn drain_archive_reqs(app: &mut App) -> Vec<PageArchiveRequest> {
    app.world_mut()
        .resource_mut::<Messages<PageArchiveRequest>>()
        .drain()
        .collect()
}

#[test]
fn close_command_archives_focused_stack() {
    let mut app = App::new();
    app.add_message::<AppCommand>()
        .add_message::<PageArchiveRequest>()
        .init_resource::<FocusedStack>()
        .add_systems(Update, super::archive_on_stack_close);
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    let stack = app
        .world_mut()
        .spawn((
            Stack::default(),
            PageMetadata {
                url: "https://gone.example".to_string(),
                title: "Gone".to_string(),
                ..default()
            },
            ChildOf(space),
        ))
        .id();
    app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close,
        )));
    app.update();
    let reqs = drain_archive_reqs(&mut app);
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].url, "https://gone.example");
    assert_eq!(reqs[0].space_id, "s1");
}

#[test]
fn close_command_skips_empty_url_stack() {
    let mut app = App::new();
    app.add_message::<AppCommand>()
        .add_message::<PageArchiveRequest>()
        .init_resource::<FocusedStack>()
        .add_systems(Update, super::archive_on_stack_close);
    let stack = app
        .world_mut()
        .spawn((Stack::default(), PageMetadata::default()))
        .id();
    app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close,
        )));
    app.update();
    assert!(drain_archive_reqs(&mut app).is_empty());
}

#[test]
fn close_records_tab_index_of_closing_stack() {
    let mut app = App::new();
    app.add_message::<AppCommand>()
        .add_message::<PageArchiveRequest>()
        .init_resource::<FocusedStack>()
        .add_systems(Update, super::archive_on_stack_close);
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    app.world_mut().spawn((Tab::default(), ChildOf(space)));
    let tab1 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let pane = app.world_mut().spawn(ChildOf(tab1)).id();
    let stack = app
        .world_mut()
        .spawn((
            Stack::default(),
            PageMetadata {
                url: "https://gone.example".to_string(),
                ..default()
            },
            ChildOf(pane),
        ))
        .id();
    app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close,
        )));
    app.update();
    let reqs = drain_archive_reqs(&mut app);
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].tab_index, Some(1));
}

#[test]
fn close_records_pane_path_and_leaf() {
    use crate::pane::{Pane, PaneId, PaneSize, PaneSplit, PaneSplitDirection};
    let mut app = App::new();
    app.add_message::<AppCommand>()
        .add_message::<PageArchiveRequest>()
        .init_resource::<FocusedStack>()
        .add_systems(Update, super::archive_on_stack_close);
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            PaneId("root".to_string()),
            ChildOf(tab),
        ))
        .id();
    let leaf0 = app
        .world_mut()
        .spawn((
            Pane,
            PaneId("leaf0".to_string()),
            PaneSize { flex_grow: 1.0 },
            ChildOf(root),
        ))
        .id();
    let leaf1 = app
        .world_mut()
        .spawn((
            Pane,
            PaneId("leaf1".to_string()),
            PaneSize { flex_grow: 3.0 },
            ChildOf(root),
        ))
        .id();
    let _ = leaf0;
    app.world_mut().spawn((Stack::default(), ChildOf(leaf1)));
    let stack = app
        .world_mut()
        .spawn((
            Stack::default(),
            PageMetadata {
                url: "https://z.example".to_string(),
                ..default()
            },
            ChildOf(leaf1),
        ))
        .id();
    app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close,
        )));
    app.update();
    let reqs = drain_archive_reqs(&mut app);
    assert_eq!(reqs.len(), 1);
    let req = &reqs[0];
    assert_eq!(req.leaf_pane_id, "leaf1");
    assert_eq!(req.stack_index, 1);
    assert_eq!(req.pane_path.len(), 1);
    assert_eq!(req.pane_path[0].split_id, "root");
    assert_eq!(req.pane_path[0].child_index, 1);
    assert_eq!(req.pane_path[0].flex_weights, vec![1.0, 3.0]);
    assert!(matches!(req.pane_path[0].axis, SplitAxis::Row));
}

#[test]
fn close_tab_archives_every_stack_in_one_group() {
    let mut app = App::new();
    app.add_message::<CloseTabRequest>()
        .add_message::<TabLayoutSpawnRequest>()
        .init_resource::<LastTabCloseAt>()
        .add_systems(Update, super::handle_close_tab_requests);
    app.world_mut()
        .spawn((bevy::window::Window::default(), PrimaryWindow));
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string()), vmux_core::Active))
        .id();
    app.world_mut().spawn((
        Tab::default(),
        vmux_history::LastActivatedAt(1),
        ChildOf(space),
    ));
    let tab = app
        .world_mut()
        .spawn((
            Tab {
                name: "Work".to_string(),
                startup_dir: Some("/tmp/work".to_string()),
            },
            vmux_history::LastActivatedAt(2),
            vmux_core::Active,
            ChildOf(space),
        ))
        .id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            PaneId("root".to_string()),
            ChildOf(tab),
        ))
        .id();
    let left = app
        .world_mut()
        .spawn((
            Pane,
            PaneId("left".to_string()),
            PaneSize { flex_grow: 1.0 },
            ChildOf(root),
        ))
        .id();
    let right = app
        .world_mut()
        .spawn((
            Pane,
            PaneId("right".to_string()),
            PaneSize { flex_grow: 2.0 },
            ChildOf(root),
        ))
        .id();
    app.world_mut().spawn((
        Stack::default(),
        PageMetadata {
            url: "https://left.example".to_string(),
            ..default()
        },
        vmux_history::LastActivatedAt(3),
        ChildOf(left),
    ));
    app.world_mut().spawn((
        Stack::default(),
        PageMetadata {
            url: "https://right.example".to_string(),
            ..default()
        },
        vmux_history::LastActivatedAt(4),
        ChildOf(right),
    ));
    app.world_mut().spawn((
        Stack::default(),
        PageMetadata::default(),
        vmux_history::LastActivatedAt(2),
        ChildOf(left),
    ));
    app.world_mut()
        .resource_mut::<Messages<CloseTabRequest>>()
        .write(CloseTabRequest { tab });

    app.update();

    assert!(app.world().get_entity(tab).is_err());
    let mut query = app
        .world_mut()
        .query::<(&ArchivedPage, &ArchivedPagePosition, &ArchivedTabPage)>();
    let archived: Vec<_> = query
        .iter(app.world())
        .map(|(page, position, tab)| {
            (
                page.url.clone(),
                position.leaf_pane_id.clone(),
                tab.group_id.clone(),
                tab.tab_name.clone(),
                tab.tab_startup_dir.clone(),
                tab.active,
            )
        })
        .collect();
    assert_eq!(archived.len(), 3);
    assert_eq!(
        archived
            .iter()
            .map(|entry| entry.2.as_str())
            .collect::<HashSet<_>>()
            .len(),
        1
    );
    assert!(archived.iter().all(|entry| entry.3 == "Work"));
    assert!(
        archived
            .iter()
            .all(|entry| entry.4.as_deref() == Some("/tmp/work"))
    );
    assert_eq!(archived.iter().filter(|entry| entry.5).count(), 1);
    assert!(
        archived
            .iter()
            .any(|entry| entry.0 == "https://right.example" && entry.5)
    );
    assert!(
        archived
            .iter()
            .any(|entry| entry.1 == "left" && entry.0 == "https://left.example")
    );
    assert!(
        archived
            .iter()
            .any(|entry| entry.1 == "right" && entry.0 == "https://right.example")
    );
    assert!(
        archived
            .iter()
            .any(|entry| entry.1 == "left" && entry.0.is_empty())
    );
}

#[test]
fn closing_last_two_tabs_same_frame_requests_one_replacement() {
    let mut app = App::new();
    app.add_message::<CloseTabRequest>()
        .add_message::<TabLayoutSpawnRequest>()
        .init_resource::<LastTabCloseAt>()
        .add_systems(Update, super::handle_close_tab_requests);
    app.world_mut()
        .spawn((bevy::window::Window::default(), PrimaryWindow));
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string()), vmux_core::Active))
        .id();
    app.insert_resource(crate::settings::EffectiveStartupDir(Some((
        space,
        Some(PathBuf::from("/tmp")),
    ))));
    let first = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_history::LastActivatedAt(2),
            vmux_core::Active,
            ChildOf(space),
        ))
        .id();
    let second = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_history::LastActivatedAt(1),
            ChildOf(space),
        ))
        .id();
    app.world_mut()
        .resource_mut::<Messages<CloseTabRequest>>()
        .write(CloseTabRequest { tab: first });
    app.world_mut()
        .resource_mut::<Messages<CloseTabRequest>>()
        .write(CloseTabRequest { tab: second });

    app.update();

    assert!(app.world().get_entity(first).is_err());
    assert!(app.world().get_entity(second).is_err());
    let requests: Vec<TabLayoutSpawnRequest> = app
        .world_mut()
        .resource_mut::<Messages<TabLayoutSpawnRequest>>()
        .drain()
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].space, space);
    assert_eq!(requests[0].startup_dir, None);
    assert!(requests[0].focus);
}

fn reopen_app() -> App {
    let mut app = App::new();
    app.add_message::<AppCommand>()
        .add_message::<PageOpenRequest>()
        .add_message::<SpawnAgentInStackRequest>()
        .add_message::<TerminalSpawnRequest>()
        .init_resource::<crate::space::ActiveSpaceEntity>()
        .init_resource::<crate::settings::LayoutSettings>()
        .add_systems(Update, super::handle_reopen_closed_page);
    app.world_mut()
        .spawn((bevy::window::Window::default(), bevy::window::PrimaryWindow));
    app
}

fn dispatch_reopen(app: &mut App) {
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Reopen,
        )));
    app.update();
}

fn drain_opens(app: &mut App) -> Vec<PageOpenRequest> {
    app.world_mut()
        .resource_mut::<Messages<PageOpenRequest>>()
        .drain()
        .collect()
}

#[derive(Resource, Default)]
struct CapturedTerminalSpawnTargets(Vec<bool>);

fn capture_terminal_spawn_targets(
    mut reader: MessageReader<TerminalSpawnRequest>,
    stacks: Query<(), With<Stack>>,
    mut captured: ResMut<CapturedTerminalSpawnTargets>,
) {
    for request in reader.read() {
        captured.0.push(
            request
                .target_stack
                .is_some_and(|stack| stacks.contains(stack)),
        );
    }
}

#[test]
fn reopen_terminal_dispatches_after_target_stack_materializes() {
    let mut app = App::new();
    app.add_message::<AppCommand>()
        .add_message::<PageOpenRequest>()
        .add_message::<SpawnAgentInStackRequest>()
        .add_message::<TerminalSpawnRequest>()
        .init_resource::<crate::space::ActiveSpaceEntity>()
        .init_resource::<crate::settings::LayoutSettings>()
        .init_resource::<CapturedTerminalSpawnTargets>()
        .add_systems(
            Update,
            (
                super::handle_reopen_closed_page,
                capture_terminal_spawn_targets,
            )
                .chain_ignore_deferred(),
        );
    app.world_mut()
        .spawn((bevy::window::Window::default(), bevy::window::PrimaryWindow));
    app.world_mut().spawn((Space, SpaceId("s1".to_string())));
    app.world_mut().spawn(ArchivedPage {
        url: TERMINAL_PAGE_URL.to_string(),
        space_id: "s1".to_string(),
        closed_at: 5,
        ..default()
    });

    dispatch_reopen(&mut app);
    app.update();

    assert_eq!(
        app.world().resource::<CapturedTerminalSpawnTargets>().0,
        vec![true]
    );
}

#[test]
fn reopen_tab_group_restores_panes_and_stacks_together() {
    let mut app = reopen_app();
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    app.world_mut().spawn((
        Tab {
            name: "Existing".to_string(),
            startup_dir: None,
        },
        ChildOf(space),
    ));
    for (url, leaf, stack_index, child_index, active) in [
        ("https://left-1.example", "left", 0, 0, false),
        ("https://left-2.example", "left", 1, 0, false),
        ("", "left", 2, 0, false),
        ("https://right.example", "right", 0, 1, true),
    ] {
        app.world_mut().spawn((
            ArchivedPage {
                url: url.to_string(),
                space_id: "s1".to_string(),
                closed_at: 10,
                tab_index: Some(1),
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: leaf.to_string(),
                stack_index,
                pane_path: vec![PaneStep {
                    split_id: "root".to_string(),
                    axis: SplitAxis::Row,
                    child_index,
                    flex_weights: vec![1.0, 2.0],
                }],
            },
            ArchivedTabPage {
                group_id: "group-1".to_string(),
                tab_name: "Recovered".to_string(),
                tab_startup_dir: Some("/tmp/recovered".to_string()),
                active,
            },
        ));
    }

    dispatch_reopen(&mut app);

    let tabs: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<Tab>>()
        .iter(app.world())
        .collect();
    assert_eq!(tabs.len(), 2);
    let recovered = tabs
        .into_iter()
        .find(|entity| {
            app.world()
                .get::<Tab>(*entity)
                .is_some_and(|tab| tab.name == "Recovered")
        })
        .expect("recovered tab");
    let recovered_tab = app.world().get::<Tab>(recovered).unwrap();
    assert_eq!(recovered_tab.startup_dir.as_deref(), Some("/tmp/recovered"));
    let space_tabs: Vec<Entity> = app
        .world()
        .get::<Children>(space)
        .unwrap()
        .iter()
        .filter(|entity| app.world().get::<Tab>(*entity).is_some())
        .collect();
    assert_eq!(space_tabs[1], recovered);
    let root = app
        .world_mut()
        .query::<(Entity, &PaneId)>()
        .iter(app.world())
        .find(|(_, id)| id.0 == "root")
        .map(|(entity, _)| entity)
        .expect("root pane");
    assert_eq!(
        app.world()
            .get::<ChildOf>(root)
            .map(|parent| parent.parent()),
        Some(recovered)
    );
    assert_eq!(
        app.world()
            .get::<PaneSplit>(root)
            .map(|split| split.direction),
        Some(PaneSplitDirection::Row)
    );
    assert!(
        app.world()
            .get::<vmux_history::LastActivatedAt>(root)
            .is_some_and(|activated| activated.0 > 0)
    );
    for (leaf_id, expected_urls) in [
        (
            "left",
            vec![
                "https://left-1.example".to_string(),
                "https://left-2.example".to_string(),
                String::new(),
            ],
        ),
        ("right", vec!["https://right.example".to_string()]),
    ] {
        let leaf = app
            .world_mut()
            .query::<(Entity, &PaneId)>()
            .iter(app.world())
            .find(|(_, id)| id.0 == leaf_id)
            .map(|(entity, _)| entity)
            .expect("leaf pane");
        let expected_flex = if leaf_id == "left" { 1.0 } else { 2.0 };
        assert_eq!(
            app.world().get::<PaneSize>(leaf).map(|size| size.flex_grow),
            Some(expected_flex)
        );
        let urls: Vec<String> = app
            .world()
            .get::<Children>(leaf)
            .unwrap()
            .iter()
            .filter_map(|stack| app.world().get::<PageMetadata>(stack))
            .map(|metadata| metadata.url.clone())
            .collect();
        assert_eq!(urls, expected_urls);
        if leaf_id == "right" {
            assert!(
                app.world()
                    .get::<vmux_history::LastActivatedAt>(leaf)
                    .is_some_and(|activated| activated.0 > 0)
            );
            let active_stack = app
                .world()
                .get::<Children>(leaf)
                .unwrap()
                .iter()
                .find(|stack| {
                    app.world()
                        .get::<PageMetadata>(*stack)
                        .is_some_and(|metadata| metadata.url == "https://right.example")
                })
                .expect("active stack");
            assert!(
                app.world()
                    .get::<vmux_history::LastActivatedAt>(active_stack)
                    .is_some_and(|activated| activated.0 > 0)
            );
        }
    }
    assert_eq!(
        app.world_mut()
            .query::<(&Stack, &vmux_history::LastActivatedAt)>()
            .iter(app.world())
            .filter(|(_, activated)| activated.0 > 0)
            .count(),
        1
    );
    assert_eq!(drain_opens(&mut app).len(), 3);
    assert_eq!(
        app.world_mut()
            .query::<&ArchivedPage>()
            .iter(app.world())
            .count(),
        0
    );
}

#[test]
fn reopen_web_opens_in_origin_space_and_consumes_entry() {
    let mut app = reopen_app();
    app.world_mut()
        .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
    app.world_mut().spawn(ArchivedPage {
        url: "https://a.example".to_string(),
        title: "A".to_string(),
        space_id: "s1".to_string(),
        closed_at: 5,
        launch: None,
        tab_index: None,
    });
    dispatch_reopen(&mut app);

    let opens = drain_opens(&mut app);
    assert_eq!(opens.len(), 1);
    assert_eq!(opens[0].url, "https://a.example");
    assert!(matches!(opens[0].target, PageOpenTarget::Stack(_)));
    let mut q = app.world_mut().query::<&ArchivedPage>();
    assert_eq!(q.iter(app.world()).count(), 0);
    let mut metas = app
        .world_mut()
        .query::<(&crate::stack::Stack, &PageMetadata)>();
    assert!(
        metas
            .iter(app.world())
            .any(|(_, m)| m.url == "https://a.example")
    );
}

#[test]
fn reopen_picks_newest_first() {
    let mut app = reopen_app();
    app.world_mut()
        .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
    app.world_mut().spawn(ArchivedPage {
        url: "https://old.example".to_string(),
        title: String::new(),
        space_id: "s1".to_string(),
        closed_at: 1,
        launch: None,
        tab_index: None,
    });
    app.world_mut().spawn(ArchivedPage {
        url: "https://new.example".to_string(),
        title: String::new(),
        space_id: "s1".to_string(),
        closed_at: 2,
        launch: None,
        tab_index: None,
    });
    dispatch_reopen(&mut app);
    let opens = drain_opens(&mut app);
    assert_eq!(opens.len(), 1);
    assert_eq!(opens[0].url, "https://new.example");
}

#[test]
fn reopen_terminal_respawns_at_cwd() {
    let mut app = reopen_app();
    app.world_mut()
        .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
    app.world_mut().spawn(ArchivedPage {
        url: "vmux://terminal/".to_string(),
        title: String::new(),
        space_id: "s1".to_string(),
        closed_at: 5,
        launch: Some(TerminalLaunch {
            command: "/bin/zsh".to_string(),
            args: vec![],
            cwd: "/work".to_string(),
            env: vec![],
            kind: TerminalKind::Plain,
        }),
        tab_index: None,
    });
    dispatch_reopen(&mut app);
    assert!(drain_opens(&mut app).is_empty());
    let spawns: Vec<TerminalSpawnRequest> = app
        .world_mut()
        .resource_mut::<Messages<TerminalSpawnRequest>>()
        .drain()
        .collect();
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].cwd, Some(PathBuf::from("/work")));
    assert!(spawns[0].target_stack.is_some());
}

fn drain_agent_spawns(app: &mut App) -> Vec<SpawnAgentInStackRequest> {
    app.world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .drain()
        .collect()
}

#[test]
fn reopen_agent_starts_fresh_when_no_session_id() {
    let mut app = reopen_app();
    app.world_mut()
        .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
    app.world_mut().spawn(ArchivedPage {
        url: format!("{}cli", AgentKind::Claude.cli_url_prefix()),
        title: String::new(),
        space_id: "s1".to_string(),
        closed_at: 5,
        launch: Some(TerminalLaunch {
            command: "claude".to_string(),
            args: vec![],
            cwd: "/proj".to_string(),
            env: vec![],
            kind: TerminalKind::Claude,
        }),
        tab_index: None,
    });
    dispatch_reopen(&mut app);
    assert!(drain_opens(&mut app).is_empty());
    let spawns = drain_agent_spawns(&mut app);
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].kind, AgentKind::Claude);
    assert_eq!(spawns[0].cwd, PathBuf::from("/proj"));
    assert!(spawns[0].session_id.is_none());
}

#[test]
fn reopen_agent_recovers_session_id_from_url() {
    let mut app = reopen_app();
    app.world_mut()
        .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
    app.world_mut().spawn(ArchivedPage {
        url: format!("{}cli/sess-123", AgentKind::Claude.cli_url_prefix()),
        title: String::new(),
        space_id: "s1".to_string(),
        closed_at: 5,
        launch: Some(TerminalLaunch {
            command: "claude".to_string(),
            args: vec![],
            cwd: "/proj".to_string(),
            env: vec![],
            kind: TerminalKind::Claude,
        }),
        tab_index: None,
    });
    dispatch_reopen(&mut app);
    assert!(drain_opens(&mut app).is_empty());
    let spawns = drain_agent_spawns(&mut app);
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].kind, AgentKind::Claude);
    assert_eq!(spawns[0].session_id.as_deref(), Some("sess-123"));
}

#[test]
fn reopen_empty_archive_is_noop() {
    let mut app = reopen_app();
    app.world_mut()
        .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
    dispatch_reopen(&mut app);
    assert!(drain_opens(&mut app).is_empty());
}

#[test]
fn reopen_falls_back_to_active_space_when_origin_gone() {
    let mut app = reopen_app();
    let active = app
        .world_mut()
        .spawn((
            crate::space::Space,
            crate::space::SpaceId("active".to_string()),
        ))
        .id();
    app.world_mut()
        .insert_resource(crate::space::ActiveSpaceEntity(Some(active)));
    app.world_mut().spawn(ArchivedPage {
        url: "https://x.example".to_string(),
        title: String::new(),
        space_id: "ghost".to_string(),
        closed_at: 5,
        launch: None,
        tab_index: None,
    });
    dispatch_reopen(&mut app);
    let opens = drain_opens(&mut app);
    assert_eq!(opens.len(), 1);
    let mut tabs = app.world_mut().query::<(&crate::tab::Tab, &ChildOf)>();
    assert!(tabs.iter(app.world()).any(|(_, co)| co.get() == active));
}

#[test]
fn reopen_restores_tab_at_original_index() {
    let mut app = reopen_app();
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    let t0 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let t1 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    app.world_mut().spawn(ArchivedPage {
        url: "https://z.example".to_string(),
        title: String::new(),
        space_id: "s1".to_string(),
        closed_at: 5,
        launch: None,
        tab_index: Some(0),
    });
    dispatch_reopen(&mut app);

    let tabs_q = app.world().entity(space).get::<Children>().unwrap();
    let tab_order: Vec<Entity> = tabs_q.iter().collect();
    assert_eq!(tab_order.len(), 3);
    assert_ne!(tab_order[0], t0);
    assert_ne!(tab_order[0], t1);
    assert_eq!(tab_order[1], t0);
    assert_eq!(tab_order[2], t1);
}

#[test]
fn reopen_appends_when_origin_space_gone() {
    let mut app = reopen_app();
    let active = app
        .world_mut()
        .spawn((Space, SpaceId("active".to_string())))
        .id();
    app.world_mut()
        .insert_resource(crate::space::ActiveSpaceEntity(Some(active)));
    let t0 = app
        .world_mut()
        .spawn((Tab::default(), ChildOf(active)))
        .id();
    let t1 = app
        .world_mut()
        .spawn((Tab::default(), ChildOf(active)))
        .id();
    app.world_mut().spawn(ArchivedPage {
        url: "https://z.example".to_string(),
        title: String::new(),
        space_id: "ghost".to_string(),
        closed_at: 5,
        launch: None,
        tab_index: Some(0),
    });
    dispatch_reopen(&mut app);

    let tabs_q = app.world().entity(active).get::<Children>().unwrap();
    let tab_order: Vec<Entity> = tabs_q.iter().collect();
    assert_eq!(tab_order.len(), 3);
    assert_eq!(tab_order[0], t0);
    assert_eq!(tab_order[1], t1);
    assert_ne!(tab_order[2], t0);
    assert_ne!(tab_order[2], t1);
}

#[test]
fn reopen_into_surviving_leaf_pane_at_index() {
    use crate::pane::{Pane, PaneId};
    let mut app = reopen_app();
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let leaf = app
        .world_mut()
        .spawn((Pane, PaneId("leaf-A".to_string()), ChildOf(tab)))
        .id();
    app.world_mut().spawn((Stack::default(), ChildOf(leaf)));
    app.world_mut().spawn((
        ArchivedPage {
            url: "https://z.example".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            ..default()
        },
        ArchivedPagePosition {
            leaf_pane_id: "leaf-A".to_string(),
            stack_index: 0,
            pane_path: Vec::new(),
        },
    ));
    dispatch_reopen(&mut app);

    let children = app.world().entity(leaf).get::<Children>().unwrap();
    let stacks: Vec<Entity> = children
        .iter()
        .filter(|&e| app.world().entity(e).contains::<Stack>())
        .collect();
    assert_eq!(stacks.len(), 2, "stack added into the existing leaf pane");
    let opens = drain_opens(&mut app);
    assert_eq!(opens.len(), 1);
    assert_eq!(opens[0].url, "https://z.example");
}

#[test]
fn reopen_without_position_recreates_tab() {
    let mut app = reopen_app();
    app.world_mut()
        .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
    app.world_mut().spawn(ArchivedPage {
        url: "https://a.example".to_string(),
        space_id: "s1".to_string(),
        closed_at: 5,
        ..default()
    });
    dispatch_reopen(&mut app);
    let opens = drain_opens(&mut app);
    assert_eq!(opens.len(), 1);
    let mut tabs = app.world_mut().query::<&crate::tab::Tab>();
    assert_eq!(tabs.iter(app.world()).count(), 1, "a tab was recreated");
}

#[test]
fn reopen_readds_leaf_under_surviving_split() {
    use crate::pane::{Pane, PaneId, PaneSplit, PaneSplitDirection};
    let mut app = reopen_app();
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            PaneId("root".to_string()),
            ChildOf(tab),
        ))
        .id();
    app.world_mut()
        .spawn((Pane, PaneId("survivor".to_string()), ChildOf(root)));
    app.world_mut().spawn((
        ArchivedPage {
            url: "https://z".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            ..default()
        },
        ArchivedPagePosition {
            leaf_pane_id: "gone-leaf".to_string(),
            stack_index: 0,
            pane_path: vec![PaneStep {
                split_id: "root".to_string(),
                axis: SplitAxis::Row,
                child_index: 1,
                flex_weights: vec![1.0, 1.0],
            }],
        },
    ));
    dispatch_reopen(&mut app);

    let root_children = app.world().entity(root).get::<Children>().unwrap();
    let panes: Vec<Entity> = root_children
        .iter()
        .filter(|&e| app.world().entity(e).contains::<Pane>())
        .collect();
    assert_eq!(
        panes.len(),
        2,
        "reopened leaf re-added under surviving split"
    );
    let has_stack = panes.iter().any(|&p| {
        app.world()
            .entity(p)
            .get::<Children>()
            .map(|c| c.iter().any(|e| app.world().entity(e).contains::<Stack>()))
            .unwrap_or(false)
    });
    assert!(has_stack);
    assert_eq!(drain_opens(&mut app).len(), 1);
}

#[test]
fn reopen_reconstructs_collapsed_split_level() {
    use crate::pane::{Pane, PaneId, PaneSplit, PaneSplitDirection};
    let mut app = reopen_app();
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            PaneId("root".to_string()),
            ChildOf(tab),
        ))
        .id();
    app.world_mut()
        .spawn((Pane, PaneId("root-leaf".to_string()), ChildOf(root)));
    app.world_mut().spawn((
        ArchivedPage {
            url: "https://z".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            ..default()
        },
        ArchivedPagePosition {
            leaf_pane_id: "deep-leaf".to_string(),
            stack_index: 0,
            pane_path: vec![
                PaneStep {
                    split_id: "root".to_string(),
                    axis: SplitAxis::Row,
                    child_index: 1,
                    flex_weights: vec![1.0, 1.0],
                },
                PaneStep {
                    split_id: "nested".to_string(),
                    axis: SplitAxis::Column,
                    child_index: 0,
                    flex_weights: vec![1.0, 1.0],
                },
            ],
        },
    ));
    dispatch_reopen(&mut app);

    let mut ids = app.world_mut().query::<&crate::pane::PaneId>();
    let recreated_nested = ids.iter(app.world()).any(|id| id.0 == "nested");
    assert!(recreated_nested, "nested split recreated by id");
    let stack_count = app.world_mut().query::<&Stack>().iter(app.world()).count();
    assert_eq!(stack_count, 1);
    assert_eq!(drain_opens(&mut app).len(), 1);
}

#[test]
fn reopen_focuses_restored_stack_and_ancestors() {
    use crate::pane::{Pane, PaneId};
    let mut app = reopen_app();
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let leaf = app
        .world_mut()
        .spawn((Pane, PaneId("leaf-A".to_string()), ChildOf(tab)))
        .id();
    app.world_mut().spawn((
        ArchivedPage {
            url: "https://z".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            ..default()
        },
        ArchivedPagePosition {
            leaf_pane_id: "leaf-A".to_string(),
            stack_index: 0,
            pane_path: Vec::new(),
        },
    ));
    dispatch_reopen(&mut app);
    assert!(
        app.world()
            .entity(leaf)
            .get::<vmux_history::LastActivatedAt>()
            .is_some()
    );
    assert!(
        app.world()
            .entity(tab)
            .get::<vmux_history::LastActivatedAt>()
            .is_some()
    );
}

#[test]
fn reopen_focus_propagates_through_reattached_splits() {
    use crate::pane::{Pane, PaneId, PaneSplit, PaneSplitDirection};
    let mut app = reopen_app();
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            PaneId("root".to_string()),
            ChildOf(tab),
        ))
        .id();
    let mid = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Column,
            },
            PaneId("mid".to_string()),
            ChildOf(root),
        ))
        .id();
    app.world_mut().spawn((
        ArchivedPage {
            url: "https://z".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            ..default()
        },
        ArchivedPagePosition {
            leaf_pane_id: "leaf-deep".to_string(),
            stack_index: 0,
            pane_path: vec![
                PaneStep {
                    split_id: "root".to_string(),
                    axis: SplitAxis::Row,
                    child_index: 1,
                    flex_weights: vec![1.0, 1.0],
                },
                PaneStep {
                    split_id: "mid".to_string(),
                    axis: SplitAxis::Column,
                    child_index: 1,
                    flex_weights: vec![1.0, 1.0],
                },
            ],
        },
    ));
    dispatch_reopen(&mut app);
    assert!(
        app.world()
            .entity(mid)
            .get::<vmux_history::LastActivatedAt>()
            .is_some(),
        "reattached intermediate split is activated through the restored chain"
    );
}

#[test]
fn reopen_stale_leaf_id_that_is_now_split_uses_descendant_leaf() {
    let mut app = reopen_app();
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            PaneId("root".to_string()),
            ChildOf(tab),
        ))
        .id();
    let promoted = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Column,
            },
            PaneId("old-leaf".to_string()),
            ChildOf(root),
        ))
        .id();
    let survivor = app
        .world_mut()
        .spawn((Pane, PaneId("survivor".to_string()), ChildOf(promoted)))
        .id();
    app.world_mut().spawn((Stack::default(), ChildOf(survivor)));
    app.world_mut().spawn((
        ArchivedPage {
            url: "https://reopened.example".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            ..default()
        },
        ArchivedPagePosition {
            leaf_pane_id: "old-leaf".to_string(),
            stack_index: 0,
            pane_path: vec![PaneStep {
                split_id: "root".to_string(),
                axis: SplitAxis::Row,
                child_index: 0,
                flex_weights: vec![1.0],
            }],
        },
    ));

    dispatch_reopen(&mut app);

    let reopened = app
        .world_mut()
        .query::<(Entity, &PageMetadata)>()
        .iter(app.world())
        .find(|(_, metadata)| metadata.url == "https://reopened.example")
        .map(|(entity, _)| entity)
        .expect("reopened stack");
    let parent = app.world().get::<ChildOf>(reopened).unwrap().parent();
    assert_eq!(parent, survivor);
    assert!(app.world().get::<PaneSplit>(parent).is_none());
    assert!(
        app.world()
            .get::<Children>(promoted)
            .is_some_and(|children| !children.contains(&reopened))
    );
}

#[test]
fn reopen_resplits_collapsed_two_pane() {
    use crate::pane::{Pane, PaneId, PaneSplit};
    let mut app = reopen_app();
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s1".to_string())))
        .id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let root = app
        .world_mut()
        .spawn((Pane, PaneId("root".to_string()), ChildOf(tab)))
        .id();
    let survivor_stack = app
        .world_mut()
        .spawn((Stack::default(), ChildOf(root)))
        .id();
    app.world_mut().spawn((
        ArchivedPage {
            url: "https://z".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            ..default()
        },
        ArchivedPagePosition {
            leaf_pane_id: "paneR".to_string(),
            stack_index: 0,
            pane_path: vec![PaneStep {
                split_id: "root".to_string(),
                axis: SplitAxis::Row,
                child_index: 1,
                flex_weights: vec![1.0, 1.0],
            }],
        },
    ));
    dispatch_reopen(&mut app);

    assert!(
        app.world().entity(root).get::<PaneSplit>().is_some(),
        "root was re-split"
    );
    let panes: Vec<Entity> = app
        .world()
        .entity(root)
        .get::<Children>()
        .unwrap()
        .iter()
        .filter(|&e| app.world().entity(e).contains::<Pane>())
        .collect();
    assert_eq!(panes.len(), 2, "two panes under the restored split");
    let total_stacks = app.world_mut().query::<&Stack>().iter(app.world()).count();
    assert_eq!(total_stacks, 2);
    let survivor_pane = app
        .world()
        .entity(survivor_stack)
        .get::<ChildOf>()
        .unwrap()
        .parent();
    assert!(
        panes.contains(&survivor_pane),
        "survivor re-homed into a pane"
    );
    assert_eq!(drain_opens(&mut app).len(), 1);
}
