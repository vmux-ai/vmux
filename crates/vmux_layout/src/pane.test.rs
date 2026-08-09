use super::*;
use crate::{
    settings::ConfirmCloseSettings,
    settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    },
};
use bevy::window::ClosingWindow;
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

fn place_pane(app: &mut App, parent: Entity, center: Vec2, size: Vec2) -> Entity {
    use bevy::ui::{ComputedNode, UiGlobalTransform};
    let node = ComputedNode { size, ..default() };
    let id = app
        .world_mut()
        .spawn((
            Pane,
            Node::default(),
            LastActivatedAt::now(),
            ChildOf(parent),
            node,
            UiGlobalTransform::from_translation(center),
        ))
        .id();
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(id)));
    id
}

#[test]
fn repair_direct_stack_child_of_split_moves_it_to_leaf() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, repair_stacks_parented_to_splits);
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
        ))
        .id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), ChildOf(split)))
        .id();
    let leaf = app.world_mut().spawn((Pane, ChildOf(split))).id();

    app.update();

    assert_eq!(
        app.world().get::<ChildOf>(stack).map(Relationship::get),
        Some(leaf)
    );
    assert!(
        app.world()
            .get::<Children>(split)
            .is_some_and(|children| !children.contains(&stack))
    );
}

#[test]
fn stamp_spawn_seq_assigns_increasing_values_to_new_panes() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SpawnCounter>()
        .add_systems(Update, stamp_spawn_seq);

    let a = app.world_mut().spawn(Pane).id();
    app.update();
    let b = app.world_mut().spawn(Pane).id();
    app.update();

    let sa = app.world().get::<SpawnSeq>(a).expect("a stamped").0;
    let sb = app.world().get::<SpawnSeq>(b).expect("b stamped").0;
    assert!(
        sb > sa,
        "later-created pane must have higher SpawnSeq ({sb} > {sa})"
    );
}

#[test]
fn reseed_spawn_counter_exceeds_max_existing() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SpawnCounter>()
        .add_systems(Update, reseed_spawn_counter);

    app.world_mut().spawn((Pane, SpawnSeq(7)));
    app.world_mut().spawn((Pane, SpawnSeq(3)));
    app.update();

    assert_eq!(app.world().resource::<SpawnCounter>().0, 8);
}

#[test]
fn open_beside_reuses_sibling_pane_as_stack() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<OpenBesideRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<SpawnCounter>()
        .add_systems(Update, handle_open_beside_requests);

    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
        ))
        .id();
    let agent_pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt::now(), ChildOf(split)))
        .id();
    app.world_mut().spawn((
        Stack::default(),
        LastActivatedAt::now(),
        ChildOf(agent_pane),
    ));
    let other_pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt::now(), ChildOf(split)))
        .id();

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: agent_pane,
            direction: Some(PaneDirection::Right),
            url: "file:///x.rs".to_string(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.update();

    assert!(
        app.world().get::<PaneSplit>(agent_pane).is_none(),
        "agent pane must not be split when a sibling exists"
    );
    let kids: Vec<Entity> = app
        .world()
        .get::<Children>(other_pane)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    let stacks = kids
        .into_iter()
        .filter(|&e| app.world().get::<Stack>(e).is_some())
        .count();
    assert_eq!(
        stacks, 1,
        "page should open as a new stack in the sibling pane"
    );
}

#[test]
fn open_beside_splits_when_no_sibling() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<OpenBesideRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<SpawnCounter>()
        .add_systems(Update, handle_open_beside_requests);

    let pane = app.world_mut().spawn((Pane, LastActivatedAt::now())).id();
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane,
            direction: Some(PaneDirection::Right),
            url: "file:///x.rs".to_string(),
            request_id: [0u8; 16],
            focus: true,
        });
    app.update();

    assert!(
        app.world().get::<PaneSplit>(pane).is_some(),
        "a lone pane should split when there is no sibling"
    );
}

fn place_pane_with_url(app: &mut App, parent: Entity, seq: u64, size: Vec2, url: &str) -> Entity {
    use bevy::ui::{ComputedNode, UiGlobalTransform};
    let pane = app
        .world_mut()
        .spawn((
            Pane,
            SpawnSeq(seq),
            Node::default(),
            LastActivatedAt::now(),
            ChildOf(parent),
            ComputedNode { size, ..default() },
            UiGlobalTransform::from_translation(size * 0.5),
        ))
        .id();
    let stack = app
        .world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    app.world_mut()
        .entity_mut(stack)
        .insert(vmux_core::PageMetadata {
            url: url.to_string(),
            ..default()
        });
    pane
}

fn stack_in_pane(app: &App, pane: Entity) -> Entity {
    let stacks: Vec<Entity> = app
        .world()
        .get::<Children>(pane)
        .map(|c| {
            c.iter()
                .filter(|&e| app.world().get::<Stack>(e).is_some())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(stacks.len(), 1, "expected one stack in pane");
    stacks[0]
}

fn page_open_requests(app: &App) -> Vec<PageOpenRequest> {
    let messages = app.world().resource::<Messages<PageOpenRequest>>();
    let mut cursor = messages.get_cursor();
    cursor.read(messages).cloned().collect()
}

fn materialize_page_metadata(app: &mut App) {
    for request in page_open_requests(app) {
        if let PageOpenTarget::Stack(stack) = request.target {
            app.world_mut()
                .entity_mut(stack)
                .insert(vmux_core::PageMetadata {
                    url: request.url,
                    ..default()
                });
        }
    }
}

fn open_beside_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<OpenBesideRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<SpawnCounter>()
        .add_systems(Update, handle_open_beside_requests);
    app
}

#[test]
fn auto_same_type_adds_tab_without_splitting() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let browser_pane =
        place_pane_with_url(&mut app, tab, 5, Vec2::new(800.0, 600.0), "https://a.com");

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: browser_pane,
            direction: None,
            url: "https://b.com".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.update();

    assert!(
        app.world().get::<PaneSplit>(browser_pane).is_none(),
        "same type must not split"
    );
    let stacks = app
        .world()
        .get::<Children>(browser_pane)
        .map(|c| {
            c.iter()
                .filter(|&e| app.world().get::<Stack>(e).is_some())
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        stacks, 2,
        "new browser page tabs into the existing browser pane"
    );
}

#[test]
fn auto_batched_files_stack_in_first_file_pane() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(800.0, 900.0),
        "vmux://agent/claude/session",
    );
    place_pane_with_url(
        &mut app,
        tab,
        2,
        Vec2::new(900.0, 400.0),
        "vmux://terminal/123",
    );

    for url in ["file:///repo/a.rs", "file:///repo/b.rs"] {
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: url.into(),
                request_id: [0u8; 16],
                focus: false,
            });
    }
    app.update();

    let requests = page_open_requests(&app);
    let file_stack_parents: Vec<Entity> = requests
        .iter()
        .filter_map(|request| match &request.target {
            PageOpenTarget::Stack(stack) if request.url.starts_with("file:") => app
                .world()
                .get::<ChildOf>(*stack)
                .map(|parent| parent.get()),
            _ => None,
        })
        .collect();

    assert_eq!(file_stack_parents.len(), 2);
    assert_eq!(
        file_stack_parents[0], file_stack_parents[1],
        "same-frame file opens should stack in one file pane"
    );
}

#[test]
fn auto_batched_new_types_split_from_newest_target() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(1600.0, 900.0),
        "vmux://agent/claude/session",
    );

    for (i, url) in [
        "https://github.com/vmux-ai/vmux",
        "file:///repo/crates/vmux_agent/src/plugin.rs",
        "vmux://terminal/",
    ]
    .into_iter()
    .enumerate()
    {
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: url.into(),
                request_id: [i as u8; 16],
                focus: false,
            });
    }
    app.update();

    let requests = page_open_requests(&app);
    let parent_for = |prefix: &str| -> Entity {
        requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url.starts_with(prefix) => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap()
    };
    let browser_parent = parent_for("https:");
    let file_parent = parent_for("file:");
    let terminal_parent = parent_for("vmux://terminal/");

    for parent in [browser_parent, file_parent, terminal_parent] {
        assert!(
            app.world().get::<PaneSplit>(parent).is_none(),
            "new stack must live in a leaf pane, not directly under a split"
        );
    }

    let browser_split = app.world().get::<ChildOf>(browser_parent).unwrap().get();
    let file_split = app.world().get::<ChildOf>(file_parent).unwrap().get();
    assert_eq!(
        app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
        file_split
    );
    assert_eq!(
        app.world().get::<ChildOf>(file_split).unwrap().get(),
        browser_split
    );
    assert_eq!(
        app.world().get::<PaneSplit>(agent_pane).unwrap().direction,
        PaneSplitDirection::Row
    );
    assert_eq!(
        app.world()
            .get::<PaneSplit>(browser_split)
            .unwrap()
            .direction,
        PaneSplitDirection::Column
    );
    assert_eq!(
        app.world().get::<PaneSplit>(file_split).unwrap().direction,
        PaneSplitDirection::Row
    );
}

#[test]
fn auto_batched_new_browser_stacks_in_existing_browser_bucket_after_other_work() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(1600.0, 900.0),
        "vmux://agent/claude/session",
    );

    for (i, url) in [
        "https://github.com/vmux-ai/vmux/pull/221",
        "file:///repo/crates/vmux_agent/src/plugin.rs",
        "file:///repo/crates/vmux_layout/src/pane.rs",
        "vmux://terminal/",
        "https://github.com/vmux-ai/vmux/actions/runs/28544986467",
    ]
    .into_iter()
    .enumerate()
    {
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: url.into(),
                request_id: [i as u8; 16],
                focus: false,
            });
    }
    app.update();

    let requests = page_open_requests(&app);
    let parent_for_url = |url: &str| -> Entity {
        requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url == url => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap()
    };
    let pr_parent = parent_for_url("https://github.com/vmux-ai/vmux/pull/221");
    let ci_parent = parent_for_url("https://github.com/vmux-ai/vmux/actions/runs/28544986467");
    let terminal_parent = parent_for_url("vmux://terminal/");

    assert_eq!(
        ci_parent, pr_parent,
        "new CI browser page should tab into the existing browser pane"
    );
    assert_ne!(ci_parent, terminal_parent);
}

#[test]
fn auto_batched_new_browser_stacks_after_nonbrowser_tab_reuse() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(1600.0, 900.0),
        "vmux://agent/claude/session",
    );

    for (i, url) in [
        "https://github.com/vmux-ai/vmux/pull/221",
        "file:///repo/crates/vmux_agent/src/plugin.rs",
        "vmux://terminal/",
        "https://github.com/vmux-ai/vmux/actions/runs/28544986467",
        "file:///repo/crates/vmux_layout/src/pane.rs",
        "https://github.com/vmux-ai/vmux/pull/221/files",
    ]
    .into_iter()
    .enumerate()
    {
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: url.into(),
                request_id: [i as u8; 16],
                focus: false,
            });
    }
    app.update();

    let requests = page_open_requests(&app);
    let parent_for_url = |url: &str| -> Entity {
        requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url == url => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap()
    };
    let ci_parent = parent_for_url("https://github.com/vmux-ai/vmux/actions/runs/28544986467");
    let files_parent = parent_for_url("https://github.com/vmux-ai/vmux/pull/221/files");

    assert_eq!(
        files_parent, ci_parent,
        "browser pages after file tab reuse should stack in the newest browser pane"
    );
}

#[test]
fn auto_file_bucket_stays_reusable_after_multiple_file_tabs() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(1600.0, 900.0),
        "vmux://agent/claude/session",
    );

    for (i, url) in [
        "https://github.com/vmux-ai/vmux/pull/221",
        "file:///repo/crates/vmux_agent/src/plugin.rs",
        "file:///repo/crates/vmux_layout/src/pane.rs",
        "vmux://terminal/",
        "file:///repo/crates/vmux_layout/src/placement.rs",
    ]
    .into_iter()
    .enumerate()
    {
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: url.into(),
                request_id: [i as u8; 16],
                focus: false,
            });
        app.update();
        materialize_page_metadata(&mut app);
    }

    let requests = page_open_requests(&app);
    let parent_for_url = |url: &str| -> Entity {
        requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url == url => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap()
    };
    let plugin_parent = parent_for_url("file:///repo/crates/vmux_agent/src/plugin.rs");
    let pane_parent = parent_for_url("file:///repo/crates/vmux_layout/src/pane.rs");
    let placement_parent = parent_for_url("file:///repo/crates/vmux_layout/src/placement.rs");
    let terminal_parent = parent_for_url("vmux://terminal/");

    assert_eq!(pane_parent, plugin_parent);
    assert_eq!(
        placement_parent, plugin_parent,
        "later files should reuse the existing file pane even after it has multiple file tabs"
    );
    assert_eq!(
        app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
        app.world().get::<ChildOf>(plugin_parent).unwrap().get(),
        "terminal should split the current file tail"
    );
}

#[test]
fn auto_terminal_splits_current_file_tail_after_file_bucket_reuse() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(1600.0, 900.0),
        "vmux://agent/claude/session",
    );

    for (i, url) in [
        "https://github.com/vmux-ai/vmux/pull/221",
        "file:///repo/crates/vmux_layout/src/pane.rs",
        "file:///repo/crates/vmux_agent/src/plugin.rs",
    ]
    .into_iter()
    .enumerate()
    {
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: url.into(),
                request_id: [i as u8; 16],
                focus: false,
            });
        app.update();
        materialize_page_metadata(&mut app);
    }

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: agent_pane,
            direction: None,
            url: "vmux://terminal/".into(),
            request_id: [9; 16],
            focus: false,
        });
    app.update();

    let requests = page_open_requests(&app);
    let parent_for_url = |url: &str| -> Entity {
        requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url == url => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap()
    };
    let pr_parent = parent_for_url("https://github.com/vmux-ai/vmux/pull/221");
    let plugin_parent = parent_for_url("file:///repo/crates/vmux_agent/src/plugin.rs");
    let terminal_parent = parent_for_url("vmux://terminal/");

    assert_eq!(
        app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
        app.world().get::<ChildOf>(plugin_parent).unwrap().get(),
        "terminal should split the current file tail"
    );
    assert_ne!(
        app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
        app.world().get::<ChildOf>(pr_parent).unwrap().get()
    );
}

#[test]
fn auto_first_file_splits_terminal_when_terminal_is_newer() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(1600.0, 900.0),
        "vmux://agent/claude/session",
    );
    let browser_pane = place_pane_with_url(
        &mut app,
        tab,
        10,
        Vec2::new(900.0, 400.0),
        "https://news.ycombinator.com/news",
    );
    let terminal_pane = place_pane_with_url(
        &mut app,
        tab,
        20,
        Vec2::new(900.0, 400.0),
        "vmux://terminal/",
    );

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: agent_pane,
            direction: None,
            url: "file:///repo/README.md".into(),
            request_id: [9; 16],
            focus: false,
        });
    app.update();

    let requests = page_open_requests(&app);
    let file_parent = requests
        .iter()
        .find_map(|request| match &request.target {
            PageOpenTarget::Stack(stack) if request.url == "file:///repo/README.md" => app
                .world()
                .get::<ChildOf>(*stack)
                .map(|parent| parent.get()),
            _ => None,
        })
        .unwrap();

    assert_eq!(
        app.world().get::<ChildOf>(file_parent).unwrap().get(),
        terminal_pane,
        "first file should split the newest terminal pane"
    );
    assert!(
        app.world().get::<PaneSplit>(browser_pane).is_none(),
        "browser pane must not split for first file"
    );
}

#[test]
fn auto_browser_open_after_files_becomes_anchor_for_terminal() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(1600.0, 900.0),
        "vmux://agent/claude/session",
    );

    for (i, url) in [
        "file:///repo/.git/HEAD",
        "file:///repo/.git/refs/heads/main",
        "https://news.ycombinator.com/news",
        "vmux://terminal/",
    ]
    .into_iter()
    .enumerate()
    {
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: url.into(),
                request_id: [i as u8; 16],
                focus: false,
            });
        app.update();
        materialize_page_metadata(&mut app);
    }

    let requests = page_open_requests(&app);
    let parent_for_url = |url: &str| -> Entity {
        requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url == url => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap()
    };
    let file_parent = parent_for_url("file:///repo/.git/refs/heads/main");
    let browser_parent = parent_for_url("https://news.ycombinator.com/news");
    let terminal_parent = parent_for_url("vmux://terminal/");
    let terminal_split = app.world().get::<ChildOf>(terminal_parent).unwrap().get();

    assert_eq!(
        terminal_split,
        app.world().get::<ChildOf>(browser_parent).unwrap().get(),
        "terminal should split the browser pane when the browser opened after files"
    );
    assert_ne!(
        terminal_split,
        app.world().get::<ChildOf>(file_parent).unwrap().get(),
        "terminal must not split the older file pane"
    );
}

#[test]
fn auto_file_after_terminal_stacks_in_existing_file_bucket() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(1600.0, 900.0),
        "vmux://agent/claude/session",
    );

    for (i, url) in [
        "file:///repo/.git/HEAD",
        "file:///repo/.git/refs/heads/main",
        "https://news.ycombinator.com/news",
        "vmux://terminal/",
        "file:///repo/README.md",
    ]
    .into_iter()
    .enumerate()
    {
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: url.into(),
                request_id: [i as u8; 16],
                focus: false,
            });
        app.update();
        materialize_page_metadata(&mut app);
    }

    let requests = page_open_requests(&app);
    let parent_for_url = |url: &str| -> Entity {
        requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url == url => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap()
    };
    let stale_file_parent = parent_for_url("file:///repo/.git/refs/heads/main");
    let terminal_parent = parent_for_url("vmux://terminal/");
    let readme_parent = parent_for_url("file:///repo/README.md");

    assert_eq!(
        readme_parent, stale_file_parent,
        "README should tab into the existing file pane"
    );
    assert_ne!(
        readme_parent, terminal_parent,
        "README must not tab into the terminal pane"
    );
}

#[test]
fn auto_duplicate_url_reuses_pending_open_in_same_batch() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(1600.0, 900.0),
        "vmux://agent/claude/session",
    );

    for i in 0..2 {
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux/pull/221".into(),
                request_id: [i; 16],
                focus: false,
            });
    }
    app.update();

    let requests = page_open_requests(&app);
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url == "https://github.com/vmux-ai/vmux/pull/221")
            .count(),
        1
    );
}

#[test]
fn auto_duplicate_url_reuses_pending_page_open_task() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(1600.0, 900.0),
        "vmux://agent/claude/session",
    );

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: agent_pane,
            direction: None,
            url: "https://github.com/vmux-ai/vmux/pull/221".into(),
            request_id: [0; 16],
            focus: false,
        });
    app.update();

    let first_stack = page_open_requests(&app)
        .iter()
        .find_map(|request| match request.target {
            PageOpenTarget::Stack(stack)
                if request.url == "https://github.com/vmux-ai/vmux/pull/221" =>
            {
                Some(stack)
            }
            _ => None,
        })
        .unwrap();
    app.world_mut().spawn(vmux_core::PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack: first_stack,
        url: "https://github.com/vmux-ai/vmux/pull/221".into(),
        request_id: None,
    });

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: agent_pane,
            direction: None,
            url: "https://github.com/vmux-ai/vmux/pull/221".into(),
            request_id: [1; 16],
            focus: false,
        });
    app.update();

    let requests = page_open_requests(&app);
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url == "https://github.com/vmux-ai/vmux/pull/221")
            .count(),
        1
    );
}

#[test]
fn direction_batched_new_type_uses_split_target_size() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(1600.0, 900.0),
        "vmux://agent/claude/session",
    );

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: agent_pane,
            direction: Some(PaneDirection::Right),
            url: "file:///repo/crates/vmux_agent/src/plugin.rs".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: agent_pane,
            direction: None,
            url: "vmux://terminal/".into(),
            request_id: [1u8; 16],
            focus: false,
        });
    app.update();

    let requests = page_open_requests(&app);
    let parent_for = |prefix: &str| -> Entity {
        requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url.starts_with(prefix) => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap()
    };
    let file_parent = parent_for("file:");
    let terminal_parent = parent_for("vmux://terminal/");
    let file_split = app.world().get::<ChildOf>(file_parent).unwrap().get();

    assert_eq!(
        app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
        file_split
    );
    assert_eq!(
        app.world().get::<PaneSplit>(file_split).unwrap().direction,
        PaneSplitDirection::Column,
        "the forced-right target is 800x900, so the next pane should split it vertically"
    );
}

#[test]
fn auto_new_type_splits_anchor() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let browser_pane =
        place_pane_with_url(&mut app, tab, 5, Vec2::new(1600.0, 900.0), "https://a.com");

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: browser_pane,
            direction: None,
            url: "file:///x.rs".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.update();

    let split = app
        .world()
        .get::<PaneSplit>(browser_pane)
        .expect("a new file type must split the anchor");
    assert_eq!(
        split.direction,
        PaneSplitDirection::Row,
        "wide anchor splits along its longer (x) side => Row"
    );
}

#[test]
fn auto_reuse_focuses_existing_url_without_new_stack() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let browser_pane =
        place_pane_with_url(&mut app, tab, 5, Vec2::new(800.0, 600.0), "https://a.com");
    let before = app
        .world_mut()
        .query_filtered::<Entity, With<Stack>>()
        .iter(app.world())
        .count();

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: browser_pane,
            direction: None,
            url: "https://a.com".into(),
            request_id: [0u8; 16],
            focus: true,
        });
    app.update();

    let after = app
        .world_mut()
        .query_filtered::<Entity, With<Stack>>()
        .iter(app.world())
        .count();
    assert_eq!(
        after, before,
        "reuse focuses the existing page; no new stack spawned"
    );
    assert!(
        app.world().get::<PaneSplit>(browser_pane).is_none(),
        "reuse must not split"
    );
}

#[test]
fn auto_reuse_focuses_existing_file_with_different_fragment() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let file_pane = place_pane_with_url(
        &mut app,
        tab,
        5,
        Vec2::new(800.0, 600.0),
        "file:///repo/src/main.rs#L10",
    );
    let before = app
        .world_mut()
        .query_filtered::<Entity, With<Stack>>()
        .iter(app.world())
        .count();

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: file_pane,
            direction: None,
            url: "file:///repo/src/main.rs#L42".into(),
            request_id: [0u8; 16],
            focus: true,
        });
    app.update();

    let after = app
        .world_mut()
        .query_filtered::<Entity, With<Stack>>()
        .iter(app.world())
        .count();
    assert_eq!(
        after, before,
        "same file with a new fragment focuses the existing page"
    );
}

#[test]
fn auto_reuse_file_with_different_fragment_navigates_existing_stack() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let file_pane = place_pane_with_url(
        &mut app,
        tab,
        5,
        Vec2::new(800.0, 600.0),
        "file:///repo/src/main.rs#L10",
    );
    let stack = stack_in_pane(&app, file_pane);

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: file_pane,
            direction: None,
            url: "file:///repo/src/main.rs#L42".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.update();

    let opens = page_open_requests(&app);
    assert_eq!(opens.len(), 1);
    match &opens[0] {
        PageOpenRequest {
            target: PageOpenTarget::Stack(target),
            url,
            ..
        } => {
            assert_eq!(*target, stack);
            assert_eq!(url, "file:///repo/src/main.rs#L42");
        }
        other => panic!("expected PageOpenRequest for existing stack, got {other:?}"),
    }
}

#[test]
fn explicit_direction_reuse_focuses_existing_file_with_different_fragment() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let file_pane = place_pane_with_url(
        &mut app,
        tab,
        5,
        Vec2::new(800.0, 600.0),
        "file:///repo/src/main.rs#L10",
    );
    let before = app
        .world_mut()
        .query_filtered::<Entity, With<Stack>>()
        .iter(app.world())
        .count();

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: file_pane,
            direction: Some(PaneDirection::Right),
            url: "file:///repo/src/main.rs#L42".into(),
            request_id: [0u8; 16],
            focus: true,
        });
    app.update();

    let after = app
        .world_mut()
        .query_filtered::<Entity, With<Stack>>()
        .iter(app.world())
        .count();
    assert_eq!(
        after, before,
        "reuse wins before explicit direction can create a duplicate"
    );
    assert!(
        app.world().get::<PaneSplit>(file_pane).is_none(),
        "reuse must not split the existing pane"
    );
}

#[test]
fn reuse_with_focus_false_does_not_activate_existing_tab() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let old_tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt(1),
            ChildOf(space),
        ))
        .id();
    let active_tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(10), ChildOf(space)))
        .id();
    let file_pane = place_pane_with_url(
        &mut app,
        old_tab,
        5,
        Vec2::new(800.0, 600.0),
        "file:///repo/src/main.rs#L10",
    );
    place_pane_with_url(
        &mut app,
        active_tab,
        6,
        Vec2::new(800.0, 600.0),
        "https://active.example",
    );

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: file_pane,
            direction: None,
            url: "file:///repo/src/main.rs#L42".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.update();

    assert_eq!(app.world().get::<LastActivatedAt>(old_tab).unwrap().0, 1);
    assert_eq!(
        app.world().get::<LastActivatedAt>(active_tab).unwrap().0,
        10
    );
}

#[test]
fn auto_browser_reuses_bucket_before_terminal_splits_existing_tail() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let browser_pane =
        place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 600.0), "https://a.com");
    let file_pane = place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: file_pane,
            direction: None,
            url: "https://github.com/vmux-ai/vmux".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.update();

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: file_pane,
            direction: None,
            url: "vmux://terminal/".into(),
            request_id: [1u8; 16],
            focus: false,
        });
    app.update();

    let requests = page_open_requests(&app);
    let parent_for_url = |url: &str| -> Entity {
        requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url == url => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap()
    };
    let github_parent = parent_for_url("https://github.com/vmux-ai/vmux");
    let terminal_parent = parent_for_url("vmux://terminal/");

    assert!(
        app.world().get::<PaneSplit>(browser_pane).is_none(),
        "new browser URL should stack in the existing browser pane"
    );
    assert_eq!(
        github_parent, browser_pane,
        "new browser URL should stack in the existing browser pane"
    );
    assert!(
        app.world().get::<PaneSplit>(file_pane).is_some(),
        "terminal should split the current file tail"
    );
    assert_eq!(
        app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
        file_pane,
        "terminal should split the current file tail"
    );
}

#[test]
fn auto_browser_reuses_bucket_before_same_batch_terminal_splits_existing_tail() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let browser_pane =
        place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 600.0), "https://a.com");
    let file_pane = place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: file_pane,
            direction: None,
            url: "https://github.com/vmux-ai/vmux".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: file_pane,
            direction: None,
            url: "vmux://terminal/".into(),
            request_id: [1u8; 16],
            focus: false,
        });
    app.update();

    let requests = page_open_requests(&app);
    let parent_for_url = |url: &str| -> Entity {
        requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url == url => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap()
    };
    let github_parent = parent_for_url("https://github.com/vmux-ai/vmux");
    let terminal_parent = parent_for_url("vmux://terminal/");

    assert!(
        app.world().get::<PaneSplit>(browser_pane).is_none(),
        "new browser URL should stack in the existing browser pane"
    );
    assert_eq!(
        github_parent, browser_pane,
        "new browser URL should stack in the existing browser pane"
    );
    assert!(
        app.world().get::<PaneSplit>(file_pane).is_some(),
        "terminal should split the current file tail"
    );
    assert_eq!(
        app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
        file_pane,
        "terminal should split the current file tail"
    );
}

#[test]
fn forced_split_anchor_keeps_current_tail_when_browser_reuses_bucket() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let browser_pane =
        place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 600.0), "https://a.com");
    let file_pane = place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: file_pane,
            direction: None,
            url: "https://github.com/vmux-ai/vmux".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.update();

    let requests = page_open_requests(&app);
    let github_parent = requests
        .iter()
        .find_map(|request| match &request.target {
            PageOpenTarget::Stack(stack) if request.url == "https://github.com/vmux-ai/vmux" => app
                .world()
                .get::<ChildOf>(*stack)
                .map(|parent| parent.get()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        github_parent, browser_pane,
        "new browser URL should reuse the existing browser pane"
    );

    app.insert_resource(SplitAnchorInput { anchor: file_pane })
        .init_resource::<SplitAnchorOut>()
        .add_systems(Update, split_anchor_test_sys);
    app.update();

    assert_eq!(app.world().resource::<SplitAnchorOut>().0, Some(file_pane));
}

#[test]
fn forced_split_anchor_ignores_exact_reused_browser_page() {
    let mut app = open_beside_app();
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let browser_pane = place_pane_with_url(
        &mut app,
        tab,
        1,
        Vec2::new(800.0, 600.0),
        "https://github.com/vmux-ai/vmux",
    );
    let file_pane = place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: file_pane,
            direction: None,
            url: "https://github.com/vmux-ai/vmux".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.update();

    app.insert_resource(SplitAnchorInput { anchor: file_pane })
        .init_resource::<SplitAnchorOut>()
        .add_systems(Update, split_anchor_test_sys);
    app.update();

    assert!(
        app.world().get::<PaneSplit>(browser_pane).is_none(),
        "exact browser reuse must not split the existing browser pane"
    );
    assert_eq!(app.world().resource::<SplitAnchorOut>().0, Some(file_pane));
}

#[derive(Resource)]
struct SplitAnchorInput {
    anchor: Entity,
}
#[derive(Resource, Default)]
struct SplitAnchorOut(Option<Entity>);

fn split_anchor_test_sys(
    input: Res<SplitAnchorInput>,
    ctx: PlacementCtx,
    mut out: ResMut<SplitAnchorOut>,
) {
    out.0 = Some(resolve_split_anchor_pane(input.anchor, &ctx));
}

#[derive(Resource)]
struct SpiralInput {
    anchor: Entity,
    url: String,
}
#[derive(Resource, Default)]
struct SpiralOut(Option<Entity>);

fn spiral_test_sys(
    input: Res<SpiralInput>,
    mut commands: Commands,
    ctx: PlacementCtx,
    mut out: ResMut<SpiralOut>,
) {
    let mut batch = std::collections::HashSet::new();
    out.0 = Some(resolve_spiral_pane(
        &mut commands,
        input.anchor,
        &input.url,
        false,
        &mut batch,
        &ctx,
    ));
}

fn spiral_app(anchor_url: &str, other: Option<(&str, u64, Vec2)>) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SpiralOut>()
        .add_systems(Update, spiral_test_sys);
    let space = app
        .world_mut()
        .spawn((crate::space::Space, vmux_core::Active))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            vmux_core::Active,
            LastActivatedAt::now(),
            ChildOf(space),
        ))
        .id();
    let agent = place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 900.0), anchor_url);
    if let Some((url, seq, size)) = other {
        place_pane_with_url(&mut app, tab, seq, size, url);
    }
    (app, agent)
}

#[test]
fn run_terminal_spirals_off_newest_nonagent_leaf() {
    let (mut app, agent) = spiral_app(
        "vmux://agent/vibe/x",
        Some(("https://a.com", 9, Vec2::new(1600.0, 900.0))),
    );
    let browser = app
        .world_mut()
        .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
        .iter(app.world())
        .find(|&e| e != agent)
        .unwrap();
    app.world_mut().insert_resource(SpiralInput {
        anchor: agent,
        url: "vmux://terminal/".into(),
    });
    app.update();

    let split = app
        .world()
        .get::<PaneSplit>(browser)
        .expect("newest non-agent (browser) leaf must split for the new terminal type");
    assert_eq!(
        split.direction,
        PaneSplitDirection::Column,
        "first terminal stacks below the browser"
    );
    assert!(
        app.world().get::<PaneSplit>(agent).is_none(),
        "agent pane untouched"
    );
    let out = app.world().resource::<SpiralOut>().0.unwrap();
    assert_ne!(out, browser, "returns the new leaf, not the split node");
}

#[test]
fn run_terminal_adds_tab_to_existing_terminal_stack() {
    let (mut app, agent) = spiral_app(
        "vmux://agent/vibe/x",
        Some(("vmux://terminal/7", 9, Vec2::new(1600.0, 900.0))),
    );
    let term_pane = app
        .world_mut()
        .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
        .iter(app.world())
        .find(|&e| e != agent)
        .unwrap();
    app.world_mut().insert_resource(SpiralInput {
        anchor: agent,
        url: "vmux://terminal/".into(),
    });
    app.update();

    assert!(
        app.world().get::<PaneSplit>(term_pane).is_none(),
        "existing terminal stack must not split"
    );
    assert_eq!(
        app.world().resource::<SpiralOut>().0,
        Some(term_pane),
        "new terminal tabs into the existing terminal pane"
    );
}

#[test]
fn force_pane_close_dispatches_pane_close_without_dialog() {
    use vmux_command::CommandPlugin;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin));
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt::now(), ChildOf(tab), ForcePaneClose))
        .id();

    process_force_pane_closes(app.world_mut());

    assert!(
        app.world().get::<ForcePaneClose>(pane).is_none(),
        "ForcePaneClose marker should be consumed"
    );
    assert!(
        app.world().get::<CloseConfirmed>(pane).is_some(),
        "pane should be marked CloseConfirmed so the close skips the dialog"
    );
    let closes: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .drain()
        .filter(|c| {
            matches!(
                c,
                AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Close))
            )
        })
        .collect();
    assert_eq!(closes.len(), 1, "exactly one PaneCommand::Close dispatched");
}

#[test]
fn select_right_picks_most_recently_active_among_overlapping_neighbors() {
    // Layout: A (left, full height), B (top-right), C (bottom-right).
    // From A, both B and C overlap on Y. Expect: navigate to whichever was
    // active most recently (B in this test).
    use bevy::ui::{ComputedNode, UiGlobalTransform};
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .init_resource::<PaneHoverIntent>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<NewStackContext>()
        .add_systems(Update, on_pane_select.in_set(WriteAppCommands));

    let _window = app.world_mut().spawn(PrimaryWindow).id();
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let split_v = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let a = place_pane(
        &mut app,
        split_v,
        Vec2::new(399.5, 450.0),
        Vec2::new(791.0, 892.0),
    );
    let split_h = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Column,
            },
            ChildOf(split_v),
        ))
        .id();
    let b = place_pane(
        &mut app,
        split_h,
        Vec2::new(1199.5, 225.0),
        Vec2::new(793.0, 442.0),
    );
    let c = place_pane(
        &mut app,
        split_h,
        Vec2::new(1199.5, 675.0),
        Vec2::new(793.0, 442.0),
    );

    // Sanity: ensure ComputedNode is set for B and C
    let _ = app.world().get::<ComputedNode>(b).unwrap();
    let _ = app.world().get::<UiGlobalTransform>(b).unwrap();

    // Activate C first, then B (B is the most recently active right-side pane).
    app.world_mut().entity_mut(c).insert(LastActivatedAt::now());
    std::thread::sleep(std::time::Duration::from_millis(2));
    app.world_mut().entity_mut(b).insert(LastActivatedAt::now());
    // Then activate A so it's the current pane.
    std::thread::sleep(std::time::Duration::from_millis(2));
    app.world_mut().entity_mut(a).insert(LastActivatedAt::now());

    let prev_b = app.world().get::<LastActivatedAt>(b).unwrap().0;
    let prev_c = app.world().get::<LastActivatedAt>(c).unwrap().0;
    assert!(prev_b > prev_c, "B should be more recently active than C");

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(
            PaneCommand::SelectRight,
        )));
    app.update();

    let new_b = app.world().get::<LastActivatedAt>(b).unwrap().0;
    let new_c = app.world().get::<LastActivatedAt>(c).unwrap().0;
    assert!(
        new_b > prev_b,
        "B (most recently active) should be re-activated by SelectRight"
    );
    assert_eq!(new_c, prev_c, "C should not be re-activated");
}

#[test]
fn select_left_picks_full_height_neighbor_from_sub_split_pane() {
    // Layout: A on left (full height), B top-right, C bottom-right.
    // From B, pressing 'h' should navigate to A (their bounding boxes overlap on Y).
    use bevy::ui::{ComputedNode, UiGlobalTransform};
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .init_resource::<PaneHoverIntent>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<NewStackContext>()
        .add_systems(Update, on_pane_select.in_set(WriteAppCommands));

    let _window = app.world_mut().spawn(PrimaryWindow).id();
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let split_v = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    // Realistic layout with gaps (8px pane gap, 4px window padding):
    // A: left, full height (791x892)
    let a = place_pane(
        &mut app,
        split_v,
        Vec2::new(399.5, 450.0),
        Vec2::new(791.0, 892.0),
    );
    let split_h = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Column,
            },
            ChildOf(split_v),
        ))
        .id();
    // B: top-right, half height (793x442)
    let b = place_pane(
        &mut app,
        split_h,
        Vec2::new(1199.5, 225.0),
        Vec2::new(793.0, 442.0),
    );
    // C: bottom-right, half height (793x442)
    let _c = place_pane(
        &mut app,
        split_h,
        Vec2::new(1199.5, 675.0),
        Vec2::new(793.0, 442.0),
    );

    let _ = (a, b);
    // sanity: ensure ComputedNode is set
    let _ = app.world().get::<ComputedNode>(b).unwrap();
    let _ = app.world().get::<UiGlobalTransform>(b).unwrap();

    app.world_mut().entity_mut(a).insert(LastActivatedAt(1));
    app.world_mut().entity_mut(b).insert(LastActivatedAt(10));
    app.world_mut().entity_mut(_c).insert(LastActivatedAt(0));
    let prev_a = app.world().get::<LastActivatedAt>(a).unwrap().0;

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(
            PaneCommand::SelectLeft,
        )));
    app.update();

    let new_a = app.world().get::<LastActivatedAt>(a).unwrap().0;
    assert!(
        new_a > prev_a,
        "SelectLeft from B should navigate to A (full-height left neighbor)"
    );
}

#[test]
fn select_left_picks_left_neighbor_in_horizontal_split() {
    use bevy::ui::{ComputedNode, UiGlobalTransform};
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .init_resource::<PaneHoverIntent>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<NewStackContext>()
        .add_systems(Update, on_pane_select.in_set(WriteAppCommands));

    let _window = app.world_mut().spawn(PrimaryWindow).id();
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let left = place_pane(
        &mut app,
        split,
        Vec2::new(400.0, 450.0),
        Vec2::new(800.0, 900.0),
    );
    let right = place_pane(
        &mut app,
        split,
        Vec2::new(1200.0, 450.0),
        Vec2::new(800.0, 900.0),
    );

    // make `right` the active pane
    app.world_mut()
        .entity_mut(right)
        .insert(LastActivatedAt::now());
    std::thread::sleep(std::time::Duration::from_millis(2));

    // sanity: ensure ComputedNode is set as expected
    assert_eq!(
        app.world().get::<ComputedNode>(right).unwrap().size,
        Vec2::new(800.0, 900.0)
    );
    assert_eq!(
        app.world()
            .get::<UiGlobalTransform>(right)
            .unwrap()
            .transform_point2(Vec2::ZERO),
        Vec2::new(1200.0, 450.0)
    );

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(
            PaneCommand::SelectLeft,
        )));
    app.update();

    let new_active_left = app
        .world()
        .get::<LastActivatedAt>(left)
        .map(|t| t.0)
        .expect("left has LastActivatedAt");
    let prev_active_right = app
        .world()
        .get::<LastActivatedAt>(right)
        .map(|t| t.0)
        .expect("right has LastActivatedAt");
    assert!(
        new_active_left > prev_active_right,
        "SelectLeft should mark left as more recently activated than right"
    );
}

#[test]
fn closing_last_pane_keeps_window_with_fresh_stack() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .init_resource::<PaneHoverIntent>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<NewStackContext>()
        .init_resource::<ConfirmCloseSettings>()
        .add_message::<PageOpenRequest>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_pane_commands.in_set(WriteAppCommands));

    let window = app.world_mut().spawn(PrimaryWindow).id();
    let tab_e = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let pane = app
        .world_mut()
        .spawn((Pane, LastActivatedAt::now(), ChildOf(tab_e)))
        .id();
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Close)));

    app.update();

    assert!(
        !app.world().entity(window).contains::<ClosingWindow>(),
        "closing the last pane must keep the window open"
    );
    let mut panes = app
        .world_mut()
        .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>();
    assert_eq!(
        panes.iter(app.world()).count(),
        1,
        "a fresh leaf pane should replace the closed one"
    );
    let mut stacks = app.world_mut().query_filtered::<Entity, With<Stack>>();
    assert_eq!(
        stacks.iter(app.world()).count(),
        1,
        "the fresh pane should contain one stack"
    );
}

#[test]
fn closing_pane_preserves_surviving_split_direction() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .init_resource::<PaneHoverIntent>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<NewStackContext>()
        .init_resource::<ConfirmCloseSettings>()
        .add_message::<PageOpenRequest>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_pane_commands.in_set(WriteAppCommands));

    let _window = app.world_mut().spawn(PrimaryWindow).id();
    let tab_e = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            Node {
                flex_direction: FlexDirection::Row,
                ..default()
            },
            ChildOf(tab_e),
        ))
        .id();
    let left = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Column,
            },
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    let left_top = place_pane(
        &mut app,
        left,
        Vec2::new(200.0, 200.0),
        Vec2::new(400.0, 400.0),
    );
    let left_bottom = place_pane(
        &mut app,
        left,
        Vec2::new(200.0, 600.0),
        Vec2::new(400.0, 400.0),
    );
    let right = place_pane(
        &mut app,
        root,
        Vec2::new(600.0, 400.0),
        Vec2::new(400.0, 800.0),
    );

    app.world_mut()
        .entity_mut(right)
        .insert(LastActivatedAt::now());

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Close)));
    app.update();

    assert!(
        app.world().get_entity(right).is_err(),
        "closed pane should be despawned"
    );
    let split = app
        .world()
        .get::<PaneSplit>(root)
        .expect("root must remain a split after the right pane closes");
    assert_eq!(
        split.direction,
        PaneSplitDirection::Column,
        "surviving left split was horizontal (Column); closing right must keep it Column, not flip to Row"
    );
    let node = app.world().get::<Node>(root).expect("root has a Node");
    assert_eq!(
        node.flex_direction,
        FlexDirection::Column,
        "root Node flex_direction must follow the adopted Column split direction"
    );
    let children = app
        .world()
        .get::<Children>(root)
        .expect("root has children");
    let leaves: Vec<Entity> = children.iter().collect();
    assert_eq!(leaves.len(), 2, "root should hold the two surviving leaves");
    assert!(leaves.contains(&left_top) && leaves.contains(&left_bottom));
}

#[test]
fn closing_one_of_three_siblings_keeps_split_intact() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .init_resource::<PaneHoverIntent>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<NewStackContext>()
        .init_resource::<ConfirmCloseSettings>()
        .add_message::<PageOpenRequest>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_pane_commands.in_set(WriteAppCommands));

    let _window = app.world_mut().spawn(PrimaryWindow).id();
    let tab_e = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            Node {
                flex_direction: FlexDirection::Row,
                ..default()
            },
            ChildOf(tab_e),
        ))
        .id();
    let a = place_pane(
        &mut app,
        root,
        Vec2::new(200.0, 400.0),
        Vec2::new(400.0, 800.0),
    );
    let b = place_pane(
        &mut app,
        root,
        Vec2::new(600.0, 400.0),
        Vec2::new(400.0, 800.0),
    );
    let c = place_pane(
        &mut app,
        root,
        Vec2::new(1000.0, 400.0),
        Vec2::new(400.0, 800.0),
    );

    app.world_mut().entity_mut(a).insert(LastActivatedAt(10));
    app.world_mut().entity_mut(c).insert(LastActivatedAt(20));
    app.world_mut().entity_mut(b).insert(LastActivatedAt(30));
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Close)));
    app.update();

    assert!(
        app.world().get_entity(b).is_err(),
        "the closed middle pane must be despawned"
    );
    let split = app
        .world()
        .get::<PaneSplit>(root)
        .expect("a 3-way split must stay a split after one pane closes");
    assert_eq!(
        split.direction,
        PaneSplitDirection::Row,
        "surviving split keeps its direction"
    );
    let children: Vec<Entity> = app
        .world()
        .get::<Children>(root)
        .expect("root has children")
        .iter()
        .collect();
    assert_eq!(
        children.len(),
        2,
        "root keeps exactly the two surviving leaves directly under it"
    );
    assert!(
        children.contains(&a) && children.contains(&c),
        "both survivors remain direct children of the split"
    );
    assert!(
        app.world().get_entity(a).is_ok() && app.world().get_entity(c).is_ok(),
        "survivors are not despawned"
    );
    for survivor in [a, c] {
        let has_stack = app
            .world()
            .get::<Children>(survivor)
            .is_some_and(|ch| ch.iter().any(|e| app.world().get::<Stack>(e).is_some()));
        assert!(has_stack, "survivor keeps its own stack");
    }
}

#[test]
fn split_gap_only_applies_on_split_axis() {
    let row = pane_split_gaps(PaneSplitDirection::Row, 8.0);
    let column = pane_split_gaps(PaneSplitDirection::Column, 8.0);

    assert_eq!(row.column_gap, Val::Px(8.0));
    assert_eq!(row.row_gap, Val::Px(0.0));
    assert_eq!(column.column_gap, Val::Px(0.0));
    assert_eq!(column.row_gap, Val::Px(8.0));
}

#[test]
fn zoomed_component_constructs_and_reads_back() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let leaf = app.world_mut().spawn(Pane).id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            Zoomed {
                leaf,
                hidden: vec![],
            },
        ))
        .id();

    let z = app.world().get::<Zoomed>(tab).expect("Zoomed present");
    assert_eq!(z.leaf, leaf);
    assert!(z.hidden.is_empty());
}

#[test]
fn zoom_command_inserts_zoomed_with_correct_hidden_set() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .init_resource::<PaneHoverIntent>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<NewStackContext>()
        .init_resource::<ConfirmCloseSettings>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_zoom_command.in_set(WriteAppCommands));
    register_zoom_hooks(&mut app);

    let _window = app.world_mut().spawn(PrimaryWindow).id();
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let leaf_a = app
        .world_mut()
        .spawn((
            Pane,
            Node::default(),
            LastActivatedAt::now(),
            ChildOf(split),
        ))
        .id();
    let leaf_b = app
        .world_mut()
        .spawn((
            Pane,
            Node::default(),
            LastActivatedAt::now(),
            ChildOf(split),
        ))
        .id();
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
    app.world_mut()
        .entity_mut(leaf_b)
        .insert(LastActivatedAt::now());

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));

    app.update();

    let z = app
        .world()
        .get::<Zoomed>(tab)
        .expect("Zoomed inserted on tab");
    assert_eq!(z.leaf, leaf_b);
    assert_eq!(z.hidden, vec![leaf_a]);
}

#[test]
fn zoom_command_on_zoomed_tab_removes_zoomed() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .init_resource::<PaneHoverIntent>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<NewStackContext>()
        .init_resource::<ConfirmCloseSettings>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_zoom_command.in_set(WriteAppCommands));
    register_zoom_hooks(&mut app);

    let _window = app.world_mut().spawn(PrimaryWindow).id();
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let leaf_a = app
        .world_mut()
        .spawn((
            Pane,
            Node::default(),
            LastActivatedAt::now(),
            ChildOf(split),
        ))
        .id();
    let leaf_b = app
        .world_mut()
        .spawn((
            Pane,
            Node::default(),
            LastActivatedAt::now(),
            ChildOf(split),
        ))
        .id();
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
    app.world_mut()
        .entity_mut(leaf_b)
        .insert(LastActivatedAt::now());

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));
    app.update();
    assert!(app.world().get::<Zoomed>(tab).is_some());

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));
    app.update();
    assert!(app.world().get::<Zoomed>(tab).is_none());
}

#[test]
fn zoom_command_on_single_pane_tab_is_noop() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .init_resource::<PaneHoverIntent>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<NewStackContext>()
        .init_resource::<ConfirmCloseSettings>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_zoom_command.in_set(WriteAppCommands));
    register_zoom_hooks(&mut app);

    let _window = app.world_mut().spawn(PrimaryWindow).id();
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let only = app
        .world_mut()
        .spawn((Pane, Node::default(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(only)));

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));
    app.update();

    assert!(app.world().get::<Zoomed>(tab).is_none());
}

#[test]
fn pane_hover_activates_hovered_pane_in_single_update() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<crate::scene::InteractionMode>()
        .init_resource::<PaneHoverIntent>()
        .insert_resource(ButtonInput::<KeyCode>::default())
        .add_systems(Update, poll_cursor_pane_focus);

    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    app.world_mut()
        .entity_mut(window)
        .get_mut::<Window>()
        .unwrap()
        .set_physical_cursor_position(Some(bevy::math::DVec2::new(400.0, 450.0)));
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(1)))
        .id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let left = place_pane(
        &mut app,
        split,
        Vec2::new(400.0, 450.0),
        Vec2::new(800.0, 900.0),
    );
    let right = place_pane(
        &mut app,
        split,
        Vec2::new(1200.0, 450.0),
        Vec2::new(800.0, 900.0),
    );
    app.world_mut().entity_mut(left).insert(LastActivatedAt(1));
    app.world_mut()
        .entity_mut(right)
        .insert(LastActivatedAt(10));
    let left_stack = app
        .world()
        .get::<Children>(left)
        .unwrap()
        .iter()
        .find(|&e| app.world().get::<Stack>(e).is_some())
        .unwrap();
    app.world_mut()
        .entity_mut(left_stack)
        .insert(LastActivatedAt(1));

    app.update();

    assert!(
        app.world().get::<LastActivatedAt>(left).unwrap().0 > 10,
        "hovered pane should activate in the same update"
    );
    assert!(
        app.world().get::<LastActivatedAt>(left_stack).unwrap().0 > 1,
        "hovered pane active stack should activate in the same update"
    );
}

#[test]
fn pane_hover_uses_native_cursor_position_fallback() {
    let source = include_str!("pane.rs");
    let poll_fn = source
        .split("fn poll_cursor_pane_focus")
        .nth(1)
        .and_then(|tail| tail.split("fn click_pane_in_player_mode").next())
        .unwrap_or_default();

    assert!(poll_fn.contains("pane_hover_cursor_position(window_entity, window)"));
    assert!(source.contains("fn native_window_cursor_position"));
    assert!(source.contains("NSEvent::mouseLocation()"));
    assert!(source.contains("convertPointFromScreen"));
}

#[cfg(target_os = "macos")]
#[test]
fn native_pane_hover_reads_latest_pointer_in_update() {
    let source = include_str!("pane.rs");
    let apply = source
        .split("fn apply_pending_hover")
        .nth(1)
        .and_then(|tail| tail.split("fn click_pane_in_player_mode").next())
        .unwrap_or_default();

    assert!(apply.contains("crate::native_pointer::snapshot()"));
    assert!(apply.contains("pointer.motion_sequence"));
}

#[test]
fn pane_hover_activates_target_stack() {
    let source = include_str!("pane.rs");
    let poll_fn = source
        .split("fn poll_cursor_pane_focus")
        .nth(1)
        .and_then(|tail| tail.split("fn pane_hover_cursor_position").next())
        .unwrap_or_default();

    assert!(poll_fn.contains("active_stack_in_pane(target"));
    assert!(poll_fn.contains("commands.entity(target_stack).insert(LastActivatedAt::now())"));
}

#[test]
fn pane_hover_runs_before_focus_cache_computes() {
    let source = include_str!("pane.rs");
    let plugin_build = source
        .split("impl Plugin for PanePlugin")
        .nth(1)
        .and_then(|tail| tail.split("fn register_zoom_hooks").next())
        .unwrap_or_default();

    assert!(plugin_build.contains("poll_cursor_pane_focus.before(crate::stack::ComputeFocusSet)"));
}

#[test]
fn removing_zoomed_pane_clears_zoom_state() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    register_zoom_hooks(&mut app);
    app.add_systems(PostUpdate, clear_zoom_on_pane_removal);

    let leaf_a = app.world_mut().spawn((Pane, Node::default())).id();
    let leaf_b = app.world_mut().spawn((Pane, Node::default())).id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            Zoomed {
                leaf: leaf_b,
                hidden: vec![leaf_a],
            },
        ))
        .id();

    app.update();

    app.world_mut().despawn(leaf_b);
    app.update();

    assert!(
        app.world().get::<Zoomed>(tab).is_none(),
        "Zoomed should be cleared when its leaf is despawned"
    );
}

#[test]
fn split_command_auto_unzooms_first() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .init_resource::<PaneHoverIntent>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<NewStackContext>()
        .init_resource::<ConfirmCloseSettings>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_zoom_command.in_set(WriteAppCommands));
    register_zoom_hooks(&mut app);

    let _window = app.world_mut().spawn(PrimaryWindow).id();
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let leaf_a = app
        .world_mut()
        .spawn((
            Pane,
            Node::default(),
            LastActivatedAt::now(),
            ChildOf(split),
        ))
        .id();
    let leaf_b = app
        .world_mut()
        .spawn((
            Pane,
            Node::default(),
            LastActivatedAt::now(),
            ChildOf(split),
        ))
        .id();
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
    app.world_mut()
        .entity_mut(leaf_b)
        .insert(LastActivatedAt::now());

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));
    app.update();
    assert!(app.world().get::<Zoomed>(tab).is_some());

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPane {
                direction: PaneDirection::Bottom,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: None,
            },
        )));
    app.update();

    assert!(
        app.world().get::<Zoomed>(tab).is_none(),
        "open-in-pane should auto-unzoom"
    );
}

#[test]
fn select_command_auto_unzooms() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .init_resource::<PaneHoverIntent>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<NewStackContext>()
        .init_resource::<ConfirmCloseSettings>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_zoom_command.in_set(WriteAppCommands));
    register_zoom_hooks(&mut app);

    let _window = app.world_mut().spawn(PrimaryWindow).id();
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let leaf_a = app
        .world_mut()
        .spawn((
            Pane,
            Node::default(),
            LastActivatedAt::now(),
            ChildOf(split),
        ))
        .id();
    let leaf_b = app
        .world_mut()
        .spawn((
            Pane,
            Node::default(),
            LastActivatedAt::now(),
            ChildOf(split),
        ))
        .id();
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
    app.world_mut()
        .entity_mut(leaf_b)
        .insert(LastActivatedAt::now());

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Zoom)));
    app.update();
    assert!(app.world().get::<Zoomed>(tab).is_some());

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Pane(
            PaneCommand::SelectLeft,
        )));
    app.update();

    assert!(
        app.world().get::<Zoomed>(tab).is_none(),
        "navigation should auto-unzoom"
    );
}

#[test]
fn removing_zoomed_restores_display_flex() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    register_zoom_hooks(&mut app);

    let leaf = app.world_mut().spawn((Pane, Node::default())).id();
    let sib = app
        .world_mut()
        .spawn((
            Pane,
            Node {
                display: Display::None,
                ..default()
            },
        ))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            Zoomed {
                leaf,
                hidden: vec![sib],
            },
        ))
        .id();

    app.update();

    app.world_mut().entity_mut(tab).remove::<Zoomed>();
    app.update();

    assert_eq!(app.world().get::<Node>(sib).unwrap().display, Display::Flex);
    let _ = leaf;
}

#[test]
fn sync_zoom_visibility_sets_display_none_on_hidden_entities() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, sync_zoom_visibility);

    let leaf = app.world_mut().spawn((Pane, Node::default())).id();
    let sib_a = app.world_mut().spawn((Pane, Node::default())).id();
    let sib_b = app.world_mut().spawn((Pane, Node::default())).id();
    let tab = app
        .world_mut()
        .spawn((
            Tab::default(),
            Zoomed {
                leaf,
                hidden: vec![sib_a, sib_b],
            },
        ))
        .id();

    app.update();

    assert_eq!(
        app.world().get::<Node>(sib_a).unwrap().display,
        Display::None
    );
    assert_eq!(
        app.world().get::<Node>(sib_b).unwrap().display,
        Display::None
    );
    assert_eq!(
        app.world().get::<Node>(leaf).unwrap().display,
        Display::Flex
    );

    let _ = tab;
}

#[test]
fn siblings_to_hide_collects_sibling_at_each_split_ancestor() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let tab = app.world_mut().spawn(Tab::default()).id();
    let split_root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let left = app.world_mut().spawn((Pane, ChildOf(split_root))).id();
    let right_split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Column,
            },
            ChildOf(split_root),
        ))
        .id();
    let right_top = app.world_mut().spawn((Pane, ChildOf(right_split))).id();
    let right_bot = app.world_mut().spawn((Pane, ChildOf(right_split))).id();

    let result = {
        let world = app.world();
        siblings_to_hide(world, right_top, tab)
    };

    assert_eq!(result.len(), 2);
    assert!(result.contains(&right_bot));
    assert!(result.contains(&left));
}

#[test]
fn siblings_to_hide_is_empty_for_single_pane_tab() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let tab = app.world_mut().spawn(Tab::default()).id();
    let only = app.world_mut().spawn((Pane, ChildOf(tab))).id();

    let result = {
        let world = app.world();
        siblings_to_hide(world, only, tab)
    };

    assert!(result.is_empty());
}

#[test]
fn pane_split_gap_sync_clears_cross_axis_gap() {
    let split = PaneSplit {
        direction: PaneSplitDirection::Row,
    };
    let mut node = Node {
        column_gap: Val::Px(16.0),
        row_gap: Val::Px(16.0),
        ..default()
    };

    apply_pane_split_gaps(&split, &mut node, 8.0);

    assert_eq!(node.column_gap, Val::Px(8.0));
    assert_eq!(node.row_gap, Val::Px(0.0));
}

#[derive(Resource, Default)]
struct InPaneCollectedSpawns(Vec<PageOpenRequest>);

fn collect_in_pane_spawns(
    mut reader: MessageReader<PageOpenRequest>,
    mut collected: ResMut<InPaneCollectedSpawns>,
) {
    for req in reader.read() {
        collected.0.push(req.clone());
    }
}

fn build_in_pane_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<PendingCursorWarp>()
        .init_resource::<InPaneCollectedSpawns>()
        .insert_resource(test_settings())
        .add_systems(
            Update,
            (
                handle_open_in_pane.in_set(WriteAppCommands),
                collect_in_pane_spawns.after(handle_open_in_pane),
            ),
        );
    let _window = app.world_mut().spawn(PrimaryWindow).id();
    app
}

fn build_single_pane(app: &mut App) -> (Entity, Entity, Entity) {
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let pane = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let stack = app
        .world_mut()
        .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    (tab, pane, stack)
}

fn build_pre_split(app: &mut App) -> (Entity, Entity, Entity, Entity) {
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(1)))
        .id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            PaneSize::default(),
            Node {
                flex_grow: 1.0,
                flex_direction: bevy::ui::FlexDirection::Row,
                ..default()
            },
            ChildOf(tab),
        ))
        .id();
    let left = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt(10), ChildOf(split)))
        .id();
    let right = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt(5), ChildOf(split)))
        .id();
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt(10), ChildOf(left)));
    app.world_mut()
        .spawn((Stack::default(), LastActivatedAt(5), ChildOf(right)));
    (tab, split, left, right)
}

#[test]
fn find_sibling_pane_returns_right_neighbor() {
    use bevy_ecs::system::RunSystemOnce;
    use vmux_command::open::PaneDirection;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            PaneSize::default(),
            Node::default(),
            ChildOf(tab),
        ))
        .id();
    let left = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(split)))
        .id();
    let right = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(split)))
        .id();

    app.update();

    let has_children = app.world().get::<Children>(split).is_some();
    assert!(has_children, "split entity should have Children component");
    let children: Vec<Entity> = app.world().get::<Children>(split).unwrap().iter().collect();
    assert!(
        children.contains(&left),
        "split children should contain left"
    );
    assert!(
        children.contains(&right),
        "split children should contain right"
    );

    let result = app.world_mut().run_system_once(
        move |child_of_q: Query<&ChildOf>,
              split_dir_q: Query<&PaneSplit>,
              pane_children: Query<&Children, With<Pane>>,
              leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>| {
            find_sibling_pane(
                left,
                &PaneDirection::Right,
                &child_of_q,
                &split_dir_q,
                &pane_children,
                &leaf_panes,
            )
        },
    );

    assert_eq!(result.unwrap(), Some(right));
}

#[test]
fn find_sibling_pane_returns_none_for_single_pane() {
    use bevy_ecs::system::RunSystemOnce;
    use vmux_command::open::PaneDirection;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let pane = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();

    app.update();

    let result = app.world_mut().run_system_once(
        move |child_of_q: Query<&ChildOf>,
              split_dir_q: Query<&PaneSplit>,
              pane_children: Query<&Children, With<Pane>>,
              leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>| {
            find_sibling_pane(
                pane,
                &PaneDirection::Right,
                &child_of_q,
                &split_dir_q,
                &pane_children,
                &leaf_panes,
            )
        },
    );

    assert_eq!(result.unwrap(), None);
}

#[test]
fn split_leaf_into_two_reparents_tabs_and_splits() {
    use bevy_ecs::system::RunSystemOnce;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let active = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now()))
        .id();
    let existing = app
        .world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(active)))
        .id();

    let p2 = app
        .world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  children: Query<&Children, With<Pane>>,
                  tabq: Query<Entity, With<Stack>>| {
                let existing_tabs: Vec<Entity> = children
                    .get(active)
                    .map(|c| c.iter().filter(|&e| tabq.contains(e)).collect())
                    .unwrap_or_default();
                split_leaf_into_two(
                    &mut commands,
                    active,
                    PaneSplitDirection::Row,
                    &existing_tabs,
                    true,
                )
            },
        )
        .unwrap();

    let world = app.world_mut();
    assert!(
        world.get::<PaneSplit>(active).is_some(),
        "active became a split root"
    );
    assert_ne!(
        world.entity(existing).get::<ChildOf>().unwrap().get(),
        active,
        "stack reparented off active"
    );
    assert!(world.get::<PaneSplit>(p2).is_none(), "p2 is a leaf");
}

#[test]
fn split_or_extend_batched_runs_make_no_empty_panes() {
    use bevy_ecs::system::RunSystemOnce;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let tab = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt::now()))
        .id();
    let anchor = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    let agent_stack = app
        .world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor)))
        .id();

    // Two agent runs splitting the same anchor in ONE command buffer: the
    // first really splits, the second extends the now-split anchor.
    let (p2a, p2b) = app
        .world_mut()
        .run_system_once(move |mut commands: Commands| {
            let existing = [agent_stack];
            let p2a = split_or_extend(
                &mut commands,
                anchor,
                PaneSplitDirection::Row,
                &existing,
                false,
                false,
            );
            let p2b = split_or_extend(
                &mut commands,
                anchor,
                PaneSplitDirection::Row,
                &existing,
                false,
                true,
            );
            (p2a, p2b)
        })
        .unwrap();

    let children: Vec<Entity> = app
        .world()
        .get::<Children>(anchor)
        .expect("anchor has children")
        .iter()
        .collect();
    assert_eq!(
        children.len(),
        3,
        "anchor holds exactly the stack-holder + two terminal leaves (no orphaned empty pane)"
    );
    assert!(
        children.contains(&p2a) && children.contains(&p2b),
        "both new terminal leaves are direct children of the split"
    );
    let stack_holders = children
        .iter()
        .filter(|&&c| {
            app.world()
                .get::<Children>(c)
                .is_some_and(|cc| cc.iter().any(|e| app.world().get::<Stack>(e).is_some()))
        })
        .count();
    assert_eq!(
        stack_holders, 1,
        "the agent stack lives in exactly one child"
    );
    let empty_leaves = children
        .iter()
        .filter(|&&c| {
            app.world()
                .get::<Children>(c)
                .map(|cc| cc.iter().count())
                .unwrap_or(0)
                == 0
        })
        .count();
    assert_eq!(
        empty_leaves, 2,
        "exactly the two terminal-host leaves are empty; no orphan leftover"
    );
}

#[test]
fn open_beside_splits_the_given_pane() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<OpenBesideRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<SpawnCounter>()
        .add_systems(Update, handle_open_beside_requests);
    let tab = app.world_mut().spawn(crate::tab::tab_bundle()).id();
    let anchor_pane = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    app.world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor_pane)));

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: anchor_pane,
            direction: Some(PaneDirection::Right),
            url: "vmux://terminal/".into(),
            request_id: [0u8; 16],
            focus: true,
        });
    app.update();

    let world = app.world_mut();
    assert!(world.get::<PaneSplit>(anchor_pane).is_some());
    let kids = world.entity(anchor_pane).get::<Children>().unwrap();
    assert_eq!(kids.iter().count(), 2);
}

#[test]
fn open_beside_with_focus_false_leaves_new_stack_unactivated() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<OpenBesideRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<SpawnCounter>()
        .add_systems(Update, handle_open_beside_requests);
    let tab = app.world_mut().spawn(crate::tab::tab_bundle()).id();
    let anchor_pane = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    app.world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor_pane)));

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: anchor_pane,
            direction: Some(PaneDirection::Right),
            url: "vmux://terminal/".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.update();

    let world = app.world_mut();
    let mut stacks = world.query_filtered::<&LastActivatedAt, With<Stack>>();
    let unactivated = stacks.iter(world).filter(|la| la.0 == 0).count();
    assert_eq!(
        unactivated, 1,
        "focus:false leaves exactly the new stack un-activated (ts 0) so focus stays put"
    );
}

#[test]
fn batched_open_beside_makes_no_empty_panes() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<OpenBesideRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<SpawnCounter>()
        .add_systems(Update, handle_open_beside_requests);
    let tab = app.world_mut().spawn(crate::tab::tab_bundle()).id();
    let anchor_pane = app
        .world_mut()
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
        .id();
    app.world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor_pane)));

    // Three open_page calls in one tick, all anchored to the same pane (as
    // the agent does for "open a few terminals beside me").
    for direction in [
        PaneDirection::Right,
        PaneDirection::Bottom,
        PaneDirection::Left,
    ] {
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: anchor_pane,
                direction: Some(direction),
                url: "vmux://terminal/".into(),
                request_id: [0u8; 16],
                focus: false,
            });
    }
    app.update();

    assert!(
        app.world().get::<PaneSplit>(anchor_pane).is_some(),
        "anchor becomes a split"
    );
    let children: Vec<Entity> = app
        .world()
        .get::<Children>(anchor_pane)
        .expect("anchor has children")
        .iter()
        .collect();
    assert_eq!(
        children.len(),
        4,
        "anchor holds the stack-holder + three terminal leaves, with no orphaned empty panes"
    );
    for child in children {
        let has_stack = app
            .world()
            .get::<Children>(child)
            .is_some_and(|cc| cc.iter().any(|e| app.world().get::<Stack>(e).is_some()));
        assert!(
            has_stack,
            "every child pane has a stack; none is an empty orphan"
        );
    }
}

#[test]
fn in_pane_new_split_right_creates_pane_to_the_right() {
    use vmux_command::open::{PaneDirection, PaneOpenMode, PaneTarget};
    let mut app = build_in_pane_app();
    let (_tab, pane, _stack) = build_single_pane(&mut app);

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: Some("https://x".into()),
            },
        )));
    app.update();

    assert!(
        app.world().get::<PaneSplit>(pane).is_some(),
        "original pane should now be a split"
    );
    let ps = app.world().get::<PaneSplit>(pane).unwrap();
    assert_eq!(ps.direction, PaneSplitDirection::Row);

    let children: Vec<Entity> = app
        .world()
        .get::<Children>(pane)
        .unwrap()
        .iter()
        .filter(|e| app.world().get::<Pane>(*e).is_some())
        .collect();
    assert_eq!(children.len(), 2, "should have two child panes");

    let collected = app.world().resource::<InPaneCollectedSpawns>();
    assert_eq!(collected.0.len(), 1);
    match &collected.0[0] {
        PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url,
            ..
        } => {
            assert_eq!(url, "https://x");
            let stack_parent = app.world().get::<ChildOf>(*stack).map(|c| c.get()).unwrap();
            assert_eq!(
                stack_parent, children[1],
                "new stack should be in the second (right) pane"
            );
        }
        other => panic!("expected PageOpenRequest, got {other:?}"),
    }
}

#[test]
fn in_pane_new_split_warps_cursor_to_new_pane() {
    use vmux_command::open::{PaneDirection, PaneOpenMode, PaneTarget};
    let mut app = build_in_pane_app();
    let (_tab, pane, _stack) = build_single_pane(&mut app);

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: Some("https://x".into()),
            },
        )));
    app.update();

    let children: Vec<Entity> = app
        .world()
        .get::<Children>(pane)
        .unwrap()
        .iter()
        .filter(|e| app.world().get::<Pane>(*e).is_some())
        .collect();

    assert_eq!(
        app.world().resource::<PendingCursorWarp>().target,
        Some(children[1]),
        "split should warp cursor to the newly active pane"
    );
}

#[test]
fn in_pane_new_split_without_url_or_startup_opens_prompt_stack() {
    use vmux_command::open::{PaneDirection, PaneOpenMode, PaneTarget};
    let mut app = build_in_pane_app();
    let (_tab, pane, _stack) = build_single_pane(&mut app);

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: None,
            },
        )));
    app.update();

    assert!(app.world().get::<PaneSplit>(pane).is_some());
    let collected = app.world().resource::<InPaneCollectedSpawns>();
    assert!(collected.0.is_empty());
    let ctx = app.world().resource::<NewStackContext>();
    assert!(ctx.stack.is_some());
    assert!(ctx.needs_open);
}

#[test]
fn in_pane_existing_in_place_navigates_neighbor_active_stack() {
    use vmux_command::open::{PaneDirection, PaneOpenMode, PaneTarget};
    let mut app = build_in_pane_app();
    let (_tab, _split, _left, right) = build_pre_split(&mut app);

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::Existing,
                mode: PaneOpenMode::InPlace,
                url: Some("https://new".into()),
            },
        )));
    app.update();

    let collected = app.world().resource::<InPaneCollectedSpawns>();
    assert_eq!(collected.0.len(), 1);
    match &collected.0[0] {
        PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url,
            ..
        } => {
            assert_eq!(url, "https://new");
            let stack_parent = app.world().get::<ChildOf>(*stack).map(|c| c.get()).unwrap();
            assert_eq!(
                stack_parent, right,
                "should navigate the existing right pane's stack"
            );
        }
        other => panic!("expected PageOpenRequest, got {other:?}"),
    }
}

#[test]
fn in_pane_existing_new_stack_adds_stack_to_neighbor() {
    use vmux_command::open::{PaneDirection, PaneOpenMode, PaneTarget};
    let mut app = build_in_pane_app();
    let (_tab, _split, _left, right) = build_pre_split(&mut app);

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::Existing,
                mode: PaneOpenMode::NewStack,
                url: Some("https://x".into()),
            },
        )));
    app.update();

    let collected = app.world().resource::<InPaneCollectedSpawns>();
    assert_eq!(collected.0.len(), 1);
    let new_stack = match &collected.0[0] {
        PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url,
            ..
        } => {
            assert_eq!(url, "https://x");
            let stack_parent = app.world().get::<ChildOf>(*stack).map(|c| c.get()).unwrap();
            assert_eq!(stack_parent, right);
            *stack
        }
        other => panic!("expected PageOpenRequest, got {other:?}"),
    };

    app.update();

    let right_stacks: Vec<Entity> = app
        .world()
        .get::<Children>(right)
        .map(|c| {
            c.iter()
                .filter(|e| app.world().get::<Stack>(*e).is_some())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(right_stacks.len(), 2, "right pane should now have 2 stacks");
    assert!(right_stacks.contains(&new_stack));
}

#[test]
fn in_pane_existing_falls_back_to_new_split_when_no_sibling() {
    use vmux_command::open::{PaneDirection, PaneOpenMode, PaneTarget};
    let mut app = build_in_pane_app();
    let (_tab, pane, _stack) = build_single_pane(&mut app);

    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::Existing,
                mode: PaneOpenMode::InPlace,
                url: Some("https://x".into()),
            },
        )));
    app.update();

    assert!(
        app.world().get::<PaneSplit>(pane).is_some(),
        "should have fallen back to splitting"
    );

    let collected = app.world().resource::<InPaneCollectedSpawns>();
    assert_eq!(collected.0.len(), 1);
    assert_eq!(collected.0[0].url, "https://x");
}

#[test]
fn assign_pane_ids_fills_missing_and_keeps_existing() {
    let mut app = App::new();
    app.add_systems(Update, super::assign_pane_ids);
    let bare = app.world_mut().spawn(super::Pane).id();
    let kept = app
        .world_mut()
        .spawn((super::Pane, super::PaneId("fixed".to_string())))
        .id();
    app.update();
    let assigned = app.world().get::<super::PaneId>(bare).expect("id assigned");
    assert!(!assigned.0.is_empty());
    assert_eq!(app.world().get::<super::PaneId>(kept).unwrap().0, "fixed");
}
