use super::*;
use crate::CommandInvocation;
use vmux_api::command_bar::{CommandBarCommandEntry, CommandBarResultItem, InvokeRequest};
use vmux_api::mcp::{McpServerEntry, McpServerStatus};

#[derive(Component, Default)]
struct CapturedInvocations(Vec<InvokeRequest>);

#[derive(Component, Default)]
struct CapturedMcpSnapshots(u32);

fn capture_invocation(
    trigger: On<UiInput<InvokeRequest>>,
    mut captured: Query<&mut CapturedInvocations>,
) {
    let Ok(mut captured) = captured.get_mut(trigger.event().webview) else {
        return;
    };
    captured.0.push(trigger.event().payload.clone());
}

fn capture_mcp_snapshot(
    trigger: On<McpSnapshotRequest>,
    mut captured: Query<&mut CapturedMcpSnapshots>,
) {
    let Ok(mut captured) = captured.get_mut(trigger.event().target) else {
        return;
    };
    captured.0 += 1;
}

#[test]
fn open_versions_reject_older_inputs() {
    let mut version = OpenVersion::default();

    assert_eq!(version.accept(OpenId(4)), Some(true));
    assert_eq!(version.accept(OpenId(4)), Some(false));
    assert_eq!(version.accept(OpenId(3)), None);
    assert_eq!(version.accept(OpenId(5)), Some(true));
}

#[test]
fn request_generations_reject_previous_responses() {
    let mut generation = RequestGeneration::default();
    let first = generation.advance();
    let second = generation.advance();

    assert!(!generation.matches(first));
    assert!(generation.matches(second));
}

#[test]
fn page_entity_projects_palette_rows() {
    let open_id = OpenId(7);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
        .add_plugins(PalettePlugin);
    let page = app.world_mut().spawn(HostsLauncher).id();
    app.update();
    app.world_mut().entity_mut(page).insert((
        PaletteOpen(CommandBarOpenEvent {
            open_id,
            commands: vec![CommandBarCommandEntry {
                id: "close_tab".to_string(),
                name: "Close Tab".to_string(),
                shortcut: String::new(),
            }],
            ..Default::default()
        }),
        PaletteDraftInput {
            open_id,
            query: ">close".to_string(),
            ..Default::default()
        },
        PaletteSnapshot(CommandPaletteUiState {
            open_id,
            ..Default::default()
        }),
    ));

    app.update();

    let snapshot = app.world().get::<PaletteSnapshot>(page).unwrap();
    assert!(
        snapshot.0.projection.rows.iter().any(
            |row| matches!(row, CommandBarResultItem::Command { id, .. } if id == "close_tab")
        )
    );
}

#[test]
fn palette_key_commands_update_host_selection_and_menu_state() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
        .add_plugins(PalettePlugin);
    let page = app.world_mut().spawn(HostsLauncher).id();
    app.update();

    let open_id = OpenId(7);
    app.world_mut().entity_mut(page).insert((
        PaletteOpen(CommandBarOpenEvent {
            open_id,
            commands: vec![
                CommandBarCommandEntry {
                    id: "close_tab".to_string(),
                    name: "Close Tab".to_string(),
                    shortcut: String::new(),
                },
                CommandBarCommandEntry {
                    id: "close_window".to_string(),
                    name: "Close Window".to_string(),
                    shortcut: String::new(),
                },
            ],
            ..Default::default()
        }),
        PaletteDraftInput {
            open_id,
            query: ">close".to_string(),
            ..Default::default()
        },
        PaletteSnapshot(CommandPaletteUiState {
            open_id,
            ..Default::default()
        }),
    ));
    app.update();
    app.world_mut()
        .resource_mut::<Messages<CommandInvocation>>()
        .write(CommandInvocation::new(page, "command_bar_next"));
    app.update();

    let input = app.world().get::<PaletteDraftInput>(page).unwrap();
    assert_eq!(input.selected, 1);
    assert!(input.navigating);
    assert_eq!(input.input_revision, 1);

    app.world_mut()
        .get_mut::<PaletteSnapshot>(page)
        .unwrap()
        .0
        .projection
        .composer
        .agents = vec![
        vmux_api::command_bar::CommandPaletteAgent {
            url: "vmux://sessions/vibe/".to_string(),
            title: "Vibe".to_string(),
        },
        vmux_api::command_bar::CommandPaletteAgent {
            url: "vmux://sessions/codex/".to_string(),
            title: "Codex".to_string(),
        },
    ];
    app.world_mut()
        .entity_mut(page)
        .insert((AgentMenuOpen, PaletteMenuCursor(0)));
    app.world_mut()
        .resource_mut::<Messages<CommandInvocation>>()
        .write(CommandInvocation::new(page, "command_bar_menu_next"));
    app.update();

    assert!(app.world().get::<AgentMenuOpen>(page).is_some());
    assert_eq!(app.world().get::<PaletteMenuCursor>(page).unwrap().0, 1);
}

#[test]
fn palette_history_is_recalled_in_host_state() {
    let open_id = OpenId(11);
    let mut app = App::new();
    app.add_observer(history);
    let page = app
        .world_mut()
        .spawn((
            PaletteOpen(CommandBarOpenEvent {
                open_id,
                ..Default::default()
            }),
            PaletteDraftInput {
                open_id,
                query: "unfinished".to_string(),
                ..Default::default()
            },
            PaletteSnapshot(CommandPaletteUiState {
                open_id,
                prompt_history: vec!["first".to_string(), "second".to_string()],
                ..Default::default()
            }),
        ))
        .id();

    app.world_mut().trigger(UiInput {
        webview: page,
        payload: CommandPaletteHistoryMoveRequest {
            open_id,
            older: true,
        },
    });

    let input = app.world().get::<PaletteDraftInput>(page).unwrap();
    assert_eq!(input.query, "second");
    assert_eq!(input.history_cursor, Some(1));
    assert_eq!(input.history_scratch, "unfinished");
    assert_eq!(input.input_revision, 1);

    app.world_mut().trigger(UiInput {
        webview: page,
        payload: CommandPaletteHistoryMoveRequest {
            open_id,
            older: false,
        },
    });

    let input = app.world().get::<PaletteDraftInput>(page).unwrap();
    assert_eq!(input.query, "unfinished");
    assert_eq!(input.history_cursor, None);
    assert_eq!(input.input_revision, 2);
}

#[test]
fn mcp_filter_and_key_selection_stay_in_host_state() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
        .add_plugins(PalettePlugin);
    let page = app.world_mut().spawn(HostsLauncher).id();
    app.update();

    let open_id = OpenId(9);
    app.world_mut().entity_mut(page).insert((
        PaletteOpen(CommandBarOpenEvent {
            open_id,
            ..Default::default()
        }),
        PaletteDraftInput {
            open_id,
            query: "/mcp lin".to_string(),
            ..Default::default()
        },
        PaletteSnapshot(CommandPaletteUiState {
            open_id,
            ..Default::default()
        }),
    ));
    app.world_mut()
        .trigger(UiStateWrite::<McpServers>::from_event(
            page,
            &McpServers {
                loaded: true,
                servers: vec![
                    McpServerEntry {
                        id: "linear".to_string(),
                        name: "Linear".to_string(),
                        description: String::new(),
                        status: McpServerStatus::Connected,
                    },
                    McpServerEntry {
                        id: "github".to_string(),
                        name: "GitHub".to_string(),
                        description: String::new(),
                        status: McpServerStatus::Available,
                    },
                ],
                ..Default::default()
            },
        ));
    app.update();

    let snapshot = app.world().get::<PaletteSnapshot>(page).unwrap();
    assert!(snapshot.0.projection.mcp_open);
    assert_eq!(snapshot.0.projection.mcp_entries.len(), 1);
    assert_eq!(snapshot.0.projection.mcp_entries[0].id, "linear");

    app.world_mut()
        .resource_mut::<Messages<CommandInvocation>>()
        .write(CommandInvocation::new(page, "command_bar_dismiss"));
    app.update();

    let input = app.world().get::<PaletteDraftInput>(page).unwrap();
    assert!(input.query.is_empty());
    assert_eq!(input.input_revision, 1);
}

#[test]
fn entering_mcp_mode_requests_one_tool_snapshot() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
        .add_plugins(PalettePlugin)
        .add_observer(capture_mcp_snapshot);
    let page = app
        .world_mut()
        .spawn((HostsLauncher, CapturedMcpSnapshots::default()))
        .id();
    app.update();

    let open_id = OpenId(10);
    app.world_mut().entity_mut(page).insert((
        PaletteOpen(CommandBarOpenEvent {
            open_id,
            ..Default::default()
        }),
        PaletteDraftInput {
            open_id,
            ..Default::default()
        },
    ));
    for query in ["/mcp", "/mcp linear"] {
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteDraftRequest {
                open_id,
                query: query.to_string(),
                ..Default::default()
            },
        });
        app.update();
    }

    assert_eq!(app.world().get::<CapturedMcpSnapshots>(page).unwrap().0, 1);
    assert!(app.world().get::<PaletteMcpActive>(page).is_some());
}

#[test]
fn submission_dispatches_the_projected_row_in_host_ecs() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
        .add_plugins(PalettePlugin)
        .add_observer(capture_invocation);
    let page = app
        .world_mut()
        .spawn((HostsLauncher, CapturedInvocations::default()))
        .id();
    app.update();

    let open_id = OpenId(11);
    app.world_mut().entity_mut(page).insert((
        PaletteOpen(CommandBarOpenEvent {
            open_id,
            commands: vec![CommandBarCommandEntry {
                id: "close_tab".to_string(),
                name: "Close Tab".to_string(),
                shortcut: String::new(),
            }],
            ..Default::default()
        }),
        PaletteDraftInput {
            open_id,
            query: ">close".to_string(),
            navigating: true,
            ..Default::default()
        },
        PaletteMcp::default(),
        PaletteSnapshot(CommandPaletteUiState {
            open_id,
            ..Default::default()
        }),
    ));
    app.update();
    app.world_mut().trigger(UiInput {
        webview: page,
        payload: CommandPaletteSubmitRequest { open_id },
    });
    app.update();

    assert_eq!(
        app.world().get::<CapturedInvocations>(page).unwrap().0,
        [InvokeRequest {
            id: "close_tab".to_string(),
            open: None,
        }]
    );
    assert_eq!(
        app.world()
            .get::<PaletteDraftInput>(page)
            .unwrap()
            .close_revision,
        1
    );
}
