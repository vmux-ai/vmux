use super::*;
use bevy::ecs::schedule::Schedules;
use std::time::{Duration, Instant};
use vmux_core::agent::{AgentKind, AgentSession};
use vmux_core::page::PageReady;
use vmux_layout::settings::{
    FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
};
use vmux_setting::{BrowserSettings, ShortcutSettings};

#[test]
fn service_bridge_routes_acp_agent_info() {
    let source = include_str!("plugin.rs");
    let handler = source
        .split("fn poll_service_messages")
        .nth(1)
        .expect("service handler")
        .split("fn flush_pending_terminal_input")
        .next()
        .expect("service handler body");
    assert!(handler.contains("ServiceMessage::Shared(SharedEvent::AcpAgentInfo"));
    assert!(handler.contains(".page_agent_info"));
}

#[test]
fn bracketed_paste_wraps_payload() {
    assert_eq!(bracketed_paste(b"hi"), b"\x1b[200~hi\x1b[201~".to_vec());
}

#[test]
fn image_path_payload_uses_vibe_attach_syntax() {
    assert_eq!(image_path_payload(true, "/tmp/a b.png"), "'/tmp/a b.png'");
    assert_eq!(image_path_payload(false, "/tmp/a b.png"), "/tmp/a b.png");
    assert_eq!(
        image_path_payload(true, "/tmp/bob's.png"),
        "'/tmp/bob'\\''s.png'"
    );
}

#[test]
fn write_clipboard_image_temp_writes_png_bytes() {
    let png = [137u8, 80, 78, 71, 1, 2, 3];
    let path = write_clipboard_image_temp(process_id(7), &png).expect("temp write");
    assert_eq!(std::fs::read(&path).unwrap(), png);
    let _ = std::fs::remove_file(&path);
}

fn process_id(byte: u8) -> ProcessId {
    ProcessId([byte; 16])
}

#[test]
fn terminal_reinput_appends_to_existing_pending_input() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<TerminalReinputRequest>()
        .add_systems(Update, handle_terminal_reinput_requests);
    let pid = process_id(7);
    let terminal = app
        .world_mut()
        .spawn((
            Terminal,
            pid,
            PendingTerminalInput {
                data: b"initial\r".to_vec(),
            },
        ))
        .id();

    app.world_mut()
        .resource_mut::<Messages<TerminalReinputRequest>>()
        .write(TerminalReinputRequest {
            process_id: pid,
            data: b"next\r".to_vec(),
        });
    app.update();

    assert_eq!(
        app.world()
            .get::<PendingTerminalInput>(terminal)
            .unwrap()
            .data,
        b"initial\rnext\r"
    );
}

#[test]
fn terminal_reinput_preserves_multiple_messages_in_order() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<TerminalReinputRequest>()
        .add_systems(Update, handle_terminal_reinput_requests);
    let pid = process_id(8);
    let terminal = app.world_mut().spawn((Terminal, pid)).id();

    app.world_mut()
        .resource_mut::<Messages<TerminalReinputRequest>>()
        .write(TerminalReinputRequest {
            process_id: pid,
            data: b"one\r".to_vec(),
        });
    app.world_mut()
        .resource_mut::<Messages<TerminalReinputRequest>>()
        .write(TerminalReinputRequest {
            process_id: pid,
            data: b"two\r".to_vec(),
        });
    app.update();

    assert_eq!(
        app.world()
            .get::<PendingTerminalInput>(terminal)
            .unwrap()
            .data,
        b"one\rtwo\r"
    );
}

#[test]
fn term_link_open_emits_browser_open_command() {
    #[derive(Resource, Default)]
    struct Captured(Vec<AppCommand>);
    fn capture(mut r: MessageReader<AppCommand>, mut c: ResMut<Captured>) {
        for m in r.read() {
            c.0.push(m.clone());
        }
    }

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<AppCommand>()
        .add_message::<vmux_command::CommandIssued>()
        .init_resource::<Captured>()
        .add_observer(on_term_link_open)
        .add_systems(Update, capture);
    let webview = app.world_mut().spawn(vmux_core::team::User).id();

    app.world_mut().trigger(BinReceive::<TermLinkOpenRequest> {
        webview,
        payload: TermLinkOpenRequest {
            url: "https://vmux.ai".into(),
        },
    });
    app.update();

    let captured = app.world().resource::<Captured>();
    assert!(
        captured.0.iter().any(|c| matches!(
            c,
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
                url: Some(u),
            })) if u == "https://vmux.ai"
        )),
        "expected InNewStack open command, got {:?}",
        captured.0
    );
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
fn terminal_send_resolves_target_by_process_id_uuid() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<crate::TerminalSendRequest>()
        .insert_resource(vmux_layout::stack::FocusedStack::default())
        .add_systems(Update, handle_terminal_send_requests);

    let parent = app.world_mut().spawn_empty().id();
    let pid = process_id(7);
    let terminal = app
        .world_mut()
        .spawn((Terminal, pid))
        .insert(ChildOf(parent))
        .id();

    app.world_mut()
        .resource_mut::<Messages<crate::TerminalSendRequest>>()
        .write(crate::TerminalSendRequest {
            text: "hi".to_string(),
            terminal: Some(pid.to_string()),
        });
    app.update();

    let pending = app
        .world()
        .get::<PendingTerminalInput>(terminal)
        .expect("input routed to terminal by process id uuid");
    assert_eq!(pending.data, b"hi".to_vec());
}

#[test]
fn terminal_stack_spawn_uses_requested_shell() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<TerminalStackSpawnRequest>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, respond_terminal_stack_spawn);

    let pane = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<Messages<TerminalStackSpawnRequest>>()
        .write(TerminalStackSpawnRequest {
            pane,
            cwd: None,
            shell: Some("/bin/agent-sh".to_string()),
            agent_run: true,
            pending_input: None,
            process_id: None,
            activate: false,
        });
    app.update();

    let mut launches = app
        .world_mut()
        .query_filtered::<(Entity, &crate::launch::TerminalLaunch), With<Terminal>>();
    let (terminal, launch) = launches.iter(app.world()).next().expect("terminal spawned");
    assert_eq!(launch.command, "/bin/agent-sh");
    assert!(
        app.world()
            .get::<crate::AgentRunTerminal>(terminal)
            .is_some()
    );
}

#[test]
fn terminal_page_open_accepts_url_without_trailing_slash() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_settings())
        .init_resource::<vmux_space::spaces::ActiveSpace>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_terminal_page_open);

    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::stack_bundle())
        .id();
    let task = app
        .world_mut()
        .spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://terminal".to_string(),
            request_id: None,
        })
        .id();

    app.update();

    assert!(app.world().get::<PageOpenHandled>(task).is_some());
    let mut terminals = app.world_mut().query_filtered::<&ChildOf, With<Terminal>>();
    assert_eq!(
        terminals
            .iter(app.world())
            .filter(|child_of| child_of.get() == stack)
            .count(),
        1
    );
}

#[test]
fn open_terminal_page_uses_per_space_startup_dir() {
    let dir = tempfile::tempdir().unwrap();
    let record = vmux_space::model::bootstrap_space_record();
    let mut settings = test_settings();
    settings.spaces.insert(
        record.id.clone(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(dir.path().to_string_lossy().into()),
        },
    );

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings)
        .insert_resource(vmux_space::spaces::ActiveSpace { record })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_terminal_page_open);

    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::stack_bundle())
        .id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://terminal".to_string(),
        request_id: None,
    });

    app.update();

    let mut launches = app
        .world_mut()
        .query_filtered::<&crate::launch::TerminalLaunch, With<Terminal>>();
    let launch = launches.iter(app.world()).next().expect("terminal spawned");
    assert_eq!(launch.cwd, dir.path().to_string_lossy());
}

#[test]
fn open_terminal_page_without_workspace_uses_shell_default() {
    let record = vmux_space::model::bootstrap_space_record();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_settings())
        .insert_resource(vmux_space::spaces::ActiveSpace { record })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_terminal_page_open);

    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::stack_bundle())
        .id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://terminal".to_string(),
        request_id: None,
    });

    app.update();

    let mut launches = app
        .world_mut()
        .query_filtered::<&crate::launch::TerminalLaunch, With<Terminal>>();
    let launch = launches.iter(app.world()).next().expect("terminal spawned");
    assert!(launch.cwd.is_empty());
}

#[test]
fn open_terminal_page_prefers_ancestor_tab_startup_dir() {
    let space_dir = tempfile::tempdir().unwrap();
    let tab_dir = tempfile::tempdir().unwrap();
    let record = vmux_space::model::bootstrap_space_record();
    let mut settings = test_settings();
    settings.spaces.insert(
        record.id.clone(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(space_dir.path().to_string_lossy().into()),
        },
    );

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings)
        .insert_resource(vmux_space::spaces::ActiveSpace { record })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_terminal_page_open);

    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "t".into(),
            startup_dir: Some(tab_dir.path().to_string_lossy().into()),
        })
        .id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(tab)))
        .id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://terminal".to_string(),
        request_id: None,
    });

    app.update();

    let mut launches = app
        .world_mut()
        .query_filtered::<&crate::launch::TerminalLaunch, With<Terminal>>();
    let launch = launches.iter(app.world()).next().expect("terminal spawned");
    assert_eq!(
        launch.cwd,
        tab_dir.path().canonicalize().unwrap().to_string_lossy()
    );
}

#[test]
fn open_terminal_page_rejects_invalid_ancestor_tab_startup_dir() {
    let fallback_dir = tempfile::tempdir().unwrap();
    let record = vmux_space::model::bootstrap_space_record();
    let mut settings = test_settings();
    settings.spaces.insert(
        record.id.clone(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(fallback_dir.path().to_string_lossy().into()),
        },
    );

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings)
        .insert_resource(vmux_space::spaces::ActiveSpace { record })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_terminal_page_open);

    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "t".into(),
            startup_dir: Some("/no/such/vmux-tab-workspace".into()),
        })
        .id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(tab)))
        .id();
    let task = app
        .world_mut()
        .spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://terminal".to_string(),
            request_id: None,
        })
        .id();

    app.update();

    assert!(app.world().get::<PageOpenError>(task).is_some());
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<Terminal>>()
            .iter(app.world())
            .count(),
        0
    );
}

#[test]
fn layout_terminal_rejects_invalid_ancestor_tab_startup_dir() {
    let fallback_dir = tempfile::tempdir().unwrap();
    let record = vmux_space::model::bootstrap_space_record();
    let mut settings = test_settings();
    settings.spaces.insert(
        record.id.clone(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(fallback_dir.path().to_string_lossy().into()),
        },
    );

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<LayoutSpawnRequest>()
        .insert_resource(settings)
        .insert_resource(vmux_space::spaces::ActiveSpace { record })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, spawn_layout_requested_content);

    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "t".into(),
            startup_dir: Some("/no/such/vmux-tab-workspace".into()),
        })
        .id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(tab)))
        .id();
    app.world_mut()
        .resource_mut::<Messages<LayoutSpawnRequest>>()
        .write(LayoutSpawnRequest::Terminal { stack });

    app.update();

    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<Terminal>>()
            .iter(app.world())
            .count(),
        0
    );
}

#[test]
fn missing_service_process_restarts_matching_terminal() {
    let missing = process_id(7);
    let target = Entity::from_bits(1);
    let plain_launch = || crate::launch::TerminalLaunch {
        command: default_shell(),
        args: vec![],
        cwd: String::new(),
        env: vec![],
        kind: crate::launch::TerminalKind::Plain,
    };
    let restart = missing_terminal_restart(
        missing,
        [
            (Entity::from_bits(2), process_id(8), plain_launch(), None),
            (target, missing, plain_launch(), None),
        ],
    )
    .unwrap();

    assert_eq!(restart.entity, target);
    assert!(restart.agent_kind.is_none());
    assert!(matches!(
        restart.command,
        ClientMessage::CreateProcess {
            process_id: _,
            command,
            args,
            cwd,
            env,
            cols: 80,
            rows: 24
        } if command == default_shell() && args.is_empty() && cwd.is_empty() && env.is_empty()
    ));
}

#[test]
fn process_create_budget_bounds_in_flight() {
    assert_eq!(
        process_create_budget(0, 8),
        8,
        "full budget when nothing in flight"
    );
    assert_eq!(process_create_budget(3, 8), 5);
    assert_eq!(process_create_budget(8, 8), 0, "no budget at the cap");
    assert_eq!(
        process_create_budget(99, 8),
        0,
        "never negative when over the cap"
    );
}

#[test]
fn process_not_found_message_parses_process_id() {
    let missing = process_id(9);

    assert_eq!(
        missing_process_id(&format!("process not found: {missing}")),
        Some(missing)
    );
    assert_eq!(missing_process_id("permission denied"), None);
}

#[test]
fn terminal_update_schedule_has_no_before_after_cycle() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        vmux_command::CommandPlugin,
        vmux_layout::stack::StackPlugin,
    ))
    .add_message::<LayoutSpawnRequest>()
    .add_plugins(TerminalUpdatePlugin);

    let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
    let mut update = schedules.remove(Update).unwrap();
    let result = update.initialize(app.world_mut());

    if let Err(error) = result {
        panic!("{}", error.to_string(update.graph(), app.world()));
    }
}

#[test]
fn terminal_input_targets_fallback_to_focused_terminal_in_user_mode() {
    let stack = Entity::from_bits(1);
    let process_id = process_id(7);

    let targets = resolve_terminal_input_targets(
        [],
        false,
        Some(stack),
        [(stack, process_id)],
        vmux_layout::scene::InteractionMode::User,
    );

    assert_eq!(targets, vec![process_id]);
}

#[test]
fn terminal_input_targets_do_not_steal_input_from_non_terminal_target() {
    let stack = Entity::from_bits(1);

    let targets = resolve_terminal_input_targets(
        [],
        true,
        Some(stack),
        [(stack, process_id(7))],
        vmux_layout::scene::InteractionMode::User,
    );

    assert!(targets.is_empty());
}

#[test]
fn terminal_input_targets_choose_focused_terminal_when_multiple_targets_exist() {
    let stale_stack = Entity::from_bits(1);
    let focused_stack = Entity::from_bits(2);
    let stale_pid = process_id(7);
    let focused_pid = process_id(8);

    let targets = resolve_terminal_input_targets(
        [(stale_stack, stale_pid), (focused_stack, focused_pid)],
        true,
        Some(focused_stack),
        [(stale_stack, stale_pid), (focused_stack, focused_pid)],
        vmux_layout::scene::InteractionMode::User,
    );

    assert_eq!(targets, vec![focused_pid]);
}

#[test]
fn terminal_input_targets_choose_focused_terminal_when_targets_are_stale() {
    let stale_stack = Entity::from_bits(1);
    let focused_stack = Entity::from_bits(2);
    let stale_pid = process_id(7);
    let focused_pid = process_id(8);

    let targets = resolve_terminal_input_targets(
        [(stale_stack, stale_pid)],
        true,
        Some(focused_stack),
        [(stale_stack, stale_pid), (focused_stack, focused_pid)],
        vmux_layout::scene::InteractionMode::User,
    );

    assert_eq!(targets, vec![focused_pid]);
}

#[test]
fn terminal_input_targets_ignore_stale_targets_when_focus_is_not_terminal() {
    let stale_stack = Entity::from_bits(1);
    let focused_stack = Entity::from_bits(2);
    let stale_pid = process_id(7);

    let targets = resolve_terminal_input_targets(
        [(stale_stack, stale_pid)],
        true,
        Some(focused_stack),
        [(stale_stack, stale_pid)],
        vmux_layout::scene::InteractionMode::User,
    );

    assert!(targets.is_empty());
}

#[test]
fn agent_focus_transition_restores_focus_to_active_blurred_agent() {
    assert_eq!(
        agent_focus_transition(true, true, true),
        Some(AgentFocusTransition::FocusIn)
    );
}

#[test]
fn web_terminal_key_events_delegate_text_to_pty_bytes() {
    let event = TermKeyEvent {
        key: "a".to_string(),
        code: "KeyA".to_string(),
        modifiers: 0,
        text: Some("a".to_string()),
    };

    assert_eq!(term_key_event_to_bytes(&event), b"a".to_vec());
}

#[test]
fn web_terminal_key_events_delegate_control_sequences() {
    let event = TermKeyEvent {
        key: "c".to_string(),
        code: "KeyC".to_string(),
        modifiers: MOD_CTRL,
        text: None,
    };

    assert_eq!(term_key_event_to_bytes(&event), vec![3]);
}

#[test]
fn web_terminal_key_events_ignore_modifier_keys() {
    let event = TermKeyEvent {
        key: "Shift".to_string(),
        code: "ShiftLeft".to_string(),
        modifiers: MOD_SHIFT,
        text: None,
    };

    assert!(term_key_event_to_bytes(&event).is_empty());
}

#[test]
fn web_terminal_shortcuts_emit_app_command_before_pty_input() {
    let event = TermKeyEvent {
        key: "l".to_string(),
        code: "KeyL".to_string(),
        modifiers: MOD_SUPER,
        text: Some("l".to_string()),
    };
    let mut state = TerminalWebShortcutState::default();

    assert_eq!(
        resolve_terminal_web_shortcut(&event, None, &mut state),
        TerminalWebShortcutAction::Command(AppCommand::Browser(vmux_command::BrowserCommand::Bar(
            vmux_command::BrowserBarCommand::OpenPageInCommandBar
        )))
    );
}

#[test]
fn web_terminal_menu_accel_shortcuts_emit_app_command_before_pty_input() {
    let event = TermKeyEvent {
        key: "S".to_string(),
        code: "KeyS".to_string(),
        modifiers: MOD_SUPER | MOD_SHIFT,
        text: Some("S".to_string()),
    };
    let mut state = TerminalWebShortcutState::default();

    assert_eq!(
        resolve_terminal_web_shortcut(&event, None, &mut state),
        TerminalWebShortcutAction::Command(AppCommand::Layout(
            vmux_command::LayoutCommand::ToggleLayout(vmux_command::ToggleLayoutCommand::Toggle)
        ))
    );
}

#[test]
fn terminal_page_emits_key_events_from_native_webview() {
    let source = include_str!("page.rs");

    assert!(source.contains("emit_key("));
    assert!(source.contains("onkeydown"));
    assert!(source.contains("TermKeyEvent"));
}

#[test]
fn terminal_page_focus_does_not_draw_browser_outline() {
    let source = include_str!("page.rs");

    assert!(source.contains("outline:none"));
}

#[test]
fn agent_loading_uses_matrix_rain() {
    let page = include_str!("page.rs");
    assert!(page.contains("MatrixRain {"));
    assert!(page.contains("accent.rain_rgb"));
    assert!(page.contains("terminal: true"));

    let rain = include_str!("matrix_rain.rs");
    assert!(rain.contains("request_animation_frame"));
    assert!(rain.contains("use_drop"));
    assert!(rain.contains("prefers-reduced-motion"));
    assert!(rain.contains("device_pixel_ratio().clamp(1.0, 1.5)"));
}

#[test]
fn terminal_web_shortcut_wakes_next_command_frame() {
    let source = include_str!("plugin.rs");
    let on_term_key = source
        .split("fn on_term_key")
        .nth(1)
        .and_then(|tail| tail.split("fn on_term_ready").next())
        .unwrap_or_default();

    assert!(on_term_key.contains("EventLoopProxyWrapper"));
    assert!(on_term_key.contains("WinitUserEvent::WakeUp"));
}

fn mouse_event(button: u8, col: u16, row: u16, pressed: bool, moving: bool) -> TermMouseEvent {
    TermMouseEvent {
        button,
        col,
        row,
        modifiers: 0,
        pressed,
        moving,
    }
}

#[test]
fn drag_enters_visual_mode_on_first_motion_and_exits_on_release() {
    let mut state = MouseSessionState::default();
    let now = std::time::Instant::now();

    let down = mouse_event(0, 2, 3, true, false);
    assert_eq!(
        mouse_terminal_actions(&mut state, &down, false, now),
        vec![MouseTerminalAction::SetSelection(None)]
    );

    let drag = mouse_event(0, 5, 3, true, true);
    assert_eq!(
        mouse_terminal_actions(
            &mut state,
            &drag,
            false,
            now + std::time::Duration::from_millis(10),
        ),
        vec![
            MouseTerminalAction::EnterCopyMode,
            MouseTerminalAction::SetSelection(Some(TermSelectionRange {
                start_col: 2,
                start_row: 3,
                end_col: 5,
                end_row: 3,
                is_block: false,
            })),
        ]
    );

    let release = mouse_event(0, 5, 3, false, false);
    assert_eq!(
        mouse_terminal_actions(
            &mut state,
            &release,
            false,
            now + std::time::Duration::from_millis(20),
        ),
        vec![MouseTerminalAction::ExitCopyMode]
    );
}

#[test]
fn single_click_never_enters_visual_mode() {
    let mut state = MouseSessionState::default();
    let now = std::time::Instant::now();

    let down = mouse_event(0, 2, 3, true, false);
    assert_eq!(
        mouse_terminal_actions(&mut state, &down, false, now),
        vec![MouseTerminalAction::SetSelection(None)]
    );

    let release = mouse_event(0, 2, 3, false, false);
    assert_eq!(
        mouse_terminal_actions(
            &mut state,
            &release,
            false,
            now + std::time::Duration::from_millis(20),
        ),
        Vec::<MouseTerminalAction>::new()
    );
}

#[test]
fn captured_mouse_without_shift_still_forwards_drag_motion() {
    let mut state = MouseSessionState::default();
    let event = mouse_event(0, 4, 5, true, true);

    assert_eq!(
        mouse_terminal_actions(&mut state, &event, true, std::time::Instant::now()),
        vec![MouseTerminalAction::ForwardInput(sgr_mouse_sequence(
            32, 4, 5, 0, true,
        ))]
    );
}

#[test]
fn hover_motion_without_app_capture_is_not_forwarded() {
    let mut state = MouseSessionState::default();
    let hover = mouse_event(3, 9, 4, true, true);

    assert_eq!(
        mouse_terminal_actions(&mut state, &hover, false, std::time::Instant::now()),
        Vec::<MouseTerminalAction>::new(),
        "bare hover with no app mouse capture must not be echoed into the PTY"
    );
}

#[test]
fn hover_motion_with_app_capture_is_forwarded() {
    let mut state = MouseSessionState::default();
    let hover = mouse_event(3, 9, 4, true, true);

    assert_eq!(
        mouse_terminal_actions(&mut state, &hover, true, std::time::Instant::now()),
        vec![MouseTerminalAction::ForwardInput(sgr_mouse_sequence(
            35, 9, 4, 0, true,
        ))]
    );
}

#[test]
fn shell_prompt_ready_only_once_cursor_is_past_column_zero() {
    assert!(!shell_prompt_ready(false, 0), "no output yet");
    assert!(
        !shell_prompt_ready(true, 0),
        "banner line ends in a newline (cursor at column 0)"
    );
    assert!(
        !shell_prompt_ready(true, 0),
        "further banner lines are still column 0"
    );
    assert!(
        shell_prompt_ready(true, 3),
        "drawn prompt leaves the cursor after the prompt string"
    );
}

#[test]
fn vim_visual_keys_map_to_copy_mode_actions() {
    use vmux_service::protocol::CopyModeKey as K;

    assert_eq!(
        map_copy_mode_key(&Key::Character("v".into()), false),
        Some(K::StartSelection)
    );
    assert_eq!(
        map_copy_mode_key(&Key::Character("V".into()), false),
        Some(K::StartLineSelection)
    );
    assert_eq!(
        map_copy_mode_key(&Key::Character("e".into()), true),
        Some(K::Down)
    );
    assert_eq!(
        map_copy_mode_key(&Key::Character("y".into()), true),
        Some(K::Up)
    );
    assert_eq!(
        map_copy_mode_key(&Key::Character("y".into()), false),
        Some(K::Copy)
    );
    assert_eq!(
        map_copy_mode_key(&Key::Character("c".into()), true),
        Some(K::Exit)
    );
}

#[test]
fn vim_g_ends_visual_selection_at_last_non_blank() {
    use vmux_service::protocol::CopyModeKey as K;

    let process_id = ProcessId::new();
    let mut local_copy_mode = LocalCopyModeState::default();

    assert_eq!(
        map_copy_mode_key_with_state(
            &mut local_copy_mode,
            process_id,
            &Key::Character("g".into()),
            false
        ),
        None
    );
    assert_eq!(
        map_copy_mode_key_with_state(
            &mut local_copy_mode,
            process_id,
            &Key::Character("_".into()),
            false
        ),
        Some(K::LastNonBlank)
    );
}

#[test]
fn vim_visual_motion_keys_map_to_copy_mode_actions() {
    use vmux_service::protocol::CopyModeKey as K;

    let process_id = ProcessId::new();
    let mut local_copy_mode = LocalCopyModeState::default();

    assert_eq!(
        map_copy_mode_keys_with_state(
            &mut local_copy_mode,
            process_id,
            CopyModeKeyInput::new(&Key::Character("w".into()), KeyCode::KeyW)
        ),
        vec![K::WordForward]
    );
    assert_eq!(
        map_copy_mode_keys_with_state(
            &mut local_copy_mode,
            process_id,
            CopyModeKeyInput::shift(&Key::Character("W".into()), KeyCode::KeyW)
        ),
        vec![K::BigWordForward]
    );
    assert_eq!(
        map_copy_mode_keys_with_state(
            &mut local_copy_mode,
            process_id,
            CopyModeKeyInput::new(&Key::Character("b".into()), KeyCode::KeyB)
        ),
        vec![K::WordBackward]
    );
    assert_eq!(
        map_copy_mode_keys_with_state(
            &mut local_copy_mode,
            process_id,
            CopyModeKeyInput::new(&Key::Character("e".into()), KeyCode::KeyE)
        ),
        vec![K::WordEndForward]
    );

    assert_eq!(
        map_copy_mode_keys_with_state(
            &mut local_copy_mode,
            process_id,
            CopyModeKeyInput::new(&Key::Character("g".into()), KeyCode::KeyG)
        ),
        Vec::<K>::new()
    );
    assert_eq!(
        map_copy_mode_keys_with_state(
            &mut local_copy_mode,
            process_id,
            CopyModeKeyInput::new(&Key::Character("e".into()), KeyCode::KeyE)
        ),
        vec![K::WordEndBackward]
    );

    assert_eq!(
        map_copy_mode_keys_with_state(
            &mut local_copy_mode,
            process_id,
            CopyModeKeyInput::new(&Key::Character("3".into()), KeyCode::Digit3)
        ),
        Vec::<K>::new()
    );
    assert_eq!(
        map_copy_mode_keys_with_state(
            &mut local_copy_mode,
            process_id,
            CopyModeKeyInput::new(&Key::Character("w".into()), KeyCode::KeyW)
        ),
        vec![K::WordForward, K::WordForward, K::WordForward]
    );
}

#[test]
fn shifted_minus_resolves_g_() {
    use vmux_service::protocol::CopyModeKey as K;

    let process_id = ProcessId::new();
    let mut local_copy_mode = LocalCopyModeState::default();

    assert_eq!(
        map_copy_mode_keys_with_state(
            &mut local_copy_mode,
            process_id,
            CopyModeKeyInput::new(&Key::Character("g".into()), KeyCode::KeyG)
        ),
        Vec::<K>::new()
    );
    assert_eq!(
        map_copy_mode_keys_with_state(
            &mut local_copy_mode,
            process_id,
            CopyModeKeyInput::shift(&Key::Character("-".into()), KeyCode::Minus)
        ),
        vec![K::LastNonBlank]
    );
}

#[test]
fn local_copy_mode_is_active_before_service_broadcast() {
    let process_id = ProcessId::new();
    let mode_map = TerminalModeMap::default();
    let mut local_copy_mode = LocalCopyModeState::default();

    assert!(!is_copy_mode_active(
        &mode_map,
        &local_copy_mode,
        process_id
    ));

    set_local_copy_mode(&mut local_copy_mode, process_id, true);

    assert!(is_copy_mode_active(&mode_map, &local_copy_mode, process_id));
}

#[test]
fn service_copy_mode_broadcast_reconciles_local_latch() {
    let process_id = ProcessId::new();
    let mut mode_map = TerminalModeMap::default();
    let mut local_copy_mode = LocalCopyModeState::default();

    set_local_copy_mode(&mut local_copy_mode, process_id, true);
    mode_map.modes.insert(
        process_id,
        TerminalModeFlags {
            mouse_capture: false,
            copy_mode: false,
            alt_screen: false,
            focus_reporting: false,
        },
    );
    set_local_copy_mode(&mut local_copy_mode, process_id, false);

    assert!(!is_copy_mode_active(
        &mode_map,
        &local_copy_mode,
        process_id
    ));
}

#[test]
fn exiting_copy_mode_clears_local_latch() {
    use vmux_service::protocol::CopyModeKey as K;

    let process_id = ProcessId::new();
    let mut local_copy_mode = LocalCopyModeState::default();
    set_local_copy_mode(&mut local_copy_mode, process_id, true);

    if copy_mode_key_exits(K::Exit) {
        set_local_copy_mode(&mut local_copy_mode, process_id, false);
    }

    assert!(!local_copy_mode.active.contains(&process_id));
}

#[test]
fn restart_state_clears_shell_output_seen_and_preserves_pending_input() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let entity = app
        .world_mut()
        .spawn((
            Terminal,
            ShellOutputSeen,
            PendingTerminalInput {
                data: b"queued\r".to_vec(),
            },
        ))
        .id();

    app.world_mut()
        .run_system_cached_with(
            |In(entity): In<Entity>, mut commands: Commands| {
                mark_terminal_restarting(&mut commands, entity);
            },
            entity,
        )
        .unwrap();

    assert!(app.world().get::<ShellOutputSeen>(entity).is_none());
    assert!(app.world().get::<AwaitingProcessCreated>(entity).is_some());
    assert_eq!(
        app.world()
            .get::<PendingTerminalInput>(entity)
            .unwrap()
            .data,
        b"queued\r"
    );
}

#[test]
fn process_created_matches_by_id_not_by_position() {
    use crate::launch::{TerminalKind, TerminalLaunch};

    let mut app = bevy::prelude::App::new();
    let id1 = ProcessId::new();
    let id2 = ProcessId::new();
    let id3 = ProcessId::new();
    let e1 = app
        .world_mut()
        .spawn((
            Terminal,
            id1,
            PendingServiceCreate,
            AwaitingProcessCreated,
            TerminalLaunch {
                command: "/bin/sh".into(),
                args: vec![],
                cwd: "/tmp/1".into(),
                env: vec![],
                kind: TerminalKind::Plain,
            },
        ))
        .id();
    let e2 = app
        .world_mut()
        .spawn((
            Terminal,
            id2,
            AwaitingProcessCreated,
            TerminalLaunch {
                command: "/bin/sh".into(),
                args: vec![],
                cwd: "/tmp/2".into(),
                env: vec![],
                kind: TerminalKind::Plain,
            },
        ))
        .id();
    let e3 = app
        .world_mut()
        .spawn((
            Terminal,
            id3,
            AwaitingProcessCreated,
            TerminalLaunch {
                command: "/bin/sh".into(),
                args: vec![],
                cwd: "/tmp/3".into(),
                env: vec![],
                kind: TerminalKind::Plain,
            },
        ))
        .id();

    for (process_id, pid) in [(id3, 333u32), (id1, 111), (id2, 222)] {
        let entity = app
            .world_mut()
            .query_filtered::<(bevy::prelude::Entity, &ProcessId), With<AwaitingProcessCreated>>()
            .iter(app.world())
            .find(|(_, pid_c)| **pid_c == process_id)
            .map(|(e, _)| e)
            .expect("matching entity for process_id");
        app.world_mut()
            .run_system_cached_with(
                |In((entity, process_id, pid)): In<(Entity, ProcessId, u32)>,
                 mut commands: Commands| {
                    apply_process_created(&mut commands, entity, process_id, pid);
                },
                (entity, process_id, pid),
            )
            .unwrap();
    }

    let world = app.world();
    assert_eq!(world.get::<crate::pid::Pid>(e1).map(|p| p.0), Some(111));
    assert_eq!(world.get::<crate::pid::Pid>(e2).map(|p| p.0), Some(222));
    assert_eq!(world.get::<crate::pid::Pid>(e3).map(|p| p.0), Some(333));
}

#[test]
fn apply_process_created_stamps_pid_and_process_id() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let entity = app
        .world_mut()
        .spawn((Terminal, AwaitingProcessCreated))
        .id();
    let id = process_id(7);
    let pid_val = 4242u32;
    app.world_mut()
        .run_system_cached_with(
            |In((entity, id, pid_val)): In<(Entity, ProcessId, u32)>, mut commands: Commands| {
                apply_process_created(&mut commands, entity, id, pid_val);
            },
            (entity, id, pid_val),
        )
        .unwrap();
    let stored_pid = app.world().get::<pid::Pid>(entity).unwrap();
    assert_eq!(stored_pid.0, pid_val);
    assert!(app.world().get::<AwaitingProcessCreated>(entity).is_none());
    let stored_process_id = app.world().get::<ProcessId>(entity).unwrap();
    assert_eq!(*stored_process_id, id);
}

#[test]
fn apply_process_create_failed_despawns_terminal() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let entity = app
        .world_mut()
        .spawn((Terminal, AwaitingProcessCreated))
        .id();
    app.world_mut()
        .run_system_cached_with(
            |In(entity): In<Entity>, mut commands: Commands| {
                apply_process_create_failed(&mut commands, entity);
            },
            entity,
        )
        .unwrap();
    assert!(
        !app.world().entities().contains(entity),
        "failed create must despawn the orphaned terminal so no system is left to drive or reap it"
    );
}

#[test]
fn agent_terminal_armed_loading_on_page_ready() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, arm_agent_loading);
    let e = app
        .world_mut()
        .spawn((
            Terminal,
            AgentSession {
                kind: AgentKind::Vibe,
            },
            PageReady {},
        ))
        .id();
    app.update();
    assert!(app.world().get::<AgentLoading>(e).is_some());
}

#[test]
fn agent_loading_preserves_initial_prompt_capture() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, arm_agent_loading);
    let e = app
        .world_mut()
        .spawn((
            Terminal,
            AgentSession {
                kind: AgentKind::Vibe,
            },
            PromptCapture {
                draft: "@asdfas".to_string(),
                skipped: false,
            },
            PageReady {},
        ))
        .id();

    app.update();

    let capture = app.world().get::<PromptCapture>(e).unwrap();
    assert_eq!(capture.draft, "@asdfas");
    assert!(!capture.skipped);
}

#[test]
fn agent_loading_armed_on_pty_restart() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, arm_agent_loading_on_restart);
    let e = app
        .world_mut()
        .spawn((
            Terminal,
            AgentSession {
                kind: AgentKind::Vibe,
            },
            ProcessId::new(),
        ))
        .id();

    // ProcessId added before the page is ready must not arm.
    app.update();
    assert!(app.world().get::<AgentLoading>(e).is_none());

    // Page becomes ready without a pid change: this system must not arm
    // (first launch is handled by arm_agent_loading).
    app.world_mut().entity_mut(e).insert(PageReady {});
    app.update();
    assert!(app.world().get::<AgentLoading>(e).is_none());

    // A restart mutates ProcessId while the page is ready: must arm.
    *app.world_mut().get_mut::<ProcessId>(e).unwrap() = ProcessId::new();
    app.update();
    assert!(app.world().get::<AgentLoading>(e).is_some());
}

#[test]
fn agent_loading_cleared_when_alt_screen_active() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<TerminalModeMap>()
        .add_systems(Update, clear_agent_loading);
    let pid = ProcessId::new();
    let e = app
        .world_mut()
        .spawn((
            Terminal,
            AgentSession {
                kind: AgentKind::Vibe,
            },
            pid,
            AgentLoading {
                since: Instant::now(),
            },
        ))
        .id();
    app.world_mut()
        .resource_mut::<TerminalModeMap>()
        .modes
        .insert(
            pid,
            TerminalModeFlags {
                mouse_capture: false,
                copy_mode: false,
                alt_screen: true,
                focus_reporting: false,
            },
        );
    app.update();
    assert!(app.world().get::<AgentLoading>(e).is_none());
}

fn clear_with_capture(capture: PromptCapture) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<TerminalModeMap>()
        .add_systems(Update, clear_agent_loading);
    let pid = ProcessId::new();
    let e = app
        .world_mut()
        .spawn((
            Terminal,
            AgentSession {
                kind: AgentKind::Claude,
            },
            pid,
            AgentLoading {
                since: Instant::now(),
            },
            capture,
        ))
        .id();
    app.world_mut()
        .resource_mut::<TerminalModeMap>()
        .modes
        .insert(
            pid,
            TerminalModeFlags {
                mouse_capture: false,
                copy_mode: false,
                alt_screen: true,
                focus_reporting: false,
            },
        );
    app.update();
    (app, e)
}

#[test]
fn ready_flips_capture_into_buffered_prompt() {
    let (app, e) = clear_with_capture(PromptCapture {
        draft: "find me a hotel".to_string(),
        skipped: false,
    });
    assert!(app.world().get::<PromptCapture>(e).is_none());
    let buffered = app.world().get::<BufferedAgentPrompt>(e).unwrap();
    assert_eq!(buffered.text, "find me a hotel");
    assert!(buffered.submit);
}

#[test]
fn ready_with_skipped_capture_delivers_nothing() {
    let (app, e) = clear_with_capture(PromptCapture {
        draft: "ignored".to_string(),
        skipped: true,
    });
    assert!(app.world().get::<PromptCapture>(e).is_none());
    assert!(app.world().get::<BufferedAgentPrompt>(e).is_none());
}

#[test]
fn agent_loading_cleared_after_timeout() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<TerminalModeMap>()
        .add_systems(Update, clear_agent_loading);
    let pid = ProcessId::new();
    let e = app
        .world_mut()
        .spawn((
            Terminal,
            AgentSession {
                kind: AgentKind::Vibe,
            },
            pid,
            AgentLoading {
                since: Instant::now() - AGENT_LOADING_TIMEOUT - Duration::from_secs(1),
            },
        ))
        .id();
    app.update();
    assert!(app.world().get::<AgentLoading>(e).is_none());
}

#[test]
fn agent_loading_retained_while_starting() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<TerminalModeMap>()
        .add_systems(Update, clear_agent_loading);
    let pid = ProcessId::new();
    let e = app
        .world_mut()
        .spawn((
            Terminal,
            AgentSession {
                kind: AgentKind::Vibe,
            },
            pid,
            AgentLoading {
                since: Instant::now(),
            },
        ))
        .id();
    app.update();
    assert!(app.world().get::<AgentLoading>(e).is_some());
}

#[test]
fn arm_loading_arms_plain_terminal() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, arm_agent_loading);
    let e = app.world_mut().spawn((Terminal, PageReady {})).id();
    app.update();
    assert!(app.world().get::<AgentLoading>(e).is_some());
}

#[test]
fn plain_terminal_loading_retained_before_min_display() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<TerminalModeMap>()
        .add_systems(Update, clear_agent_loading);
    let e = app
        .world_mut()
        .spawn((
            Terminal,
            ProcessId::new(),
            AgentLoading {
                since: Instant::now(),
            },
        ))
        .id();
    app.update();
    assert!(app.world().get::<AgentLoading>(e).is_some());
}

#[test]
fn plain_terminal_loading_cleared_after_min_display() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<TerminalModeMap>()
        .add_systems(Update, clear_agent_loading);
    let e = app
        .world_mut()
        .spawn((
            Terminal,
            ProcessId::new(),
            AgentLoading {
                since: Instant::now() - TERMINAL_LOADING_MIN_DISPLAY - Duration::from_millis(1),
            },
        ))
        .id();
    app.update();
    assert!(app.world().get::<AgentLoading>(e).is_none());
}

#[test]
fn terminal_title_resets_to_plain_when_agent_session_removed() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, reset_terminal_title_on_agent_removed);
    let pid = ProcessId::new();
    let e = app
        .world_mut()
        .spawn((
            Terminal,
            pid,
            PageMetadata {
                title: "Vibe (abc12345)".to_string(),
                url: "vmux://agent/vibe/abc12345".to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            AgentSession {
                kind: AgentKind::Vibe,
            },
        ))
        .id();
    app.update();
    app.world_mut().entity_mut(e).remove::<AgentSession>();
    app.update();
    let expected = format!("Terminal ({})", &pid.to_string()[..8]);
    let title = app.world().get::<PageMetadata>(e).unwrap().title.clone();
    assert_eq!(title, expected);
}

#[test]
fn apply_osc_title_sets_and_clears() {
    use bevy::ecs::message::Messages;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<OscTitleChanged>()
        .add_systems(Update, apply_osc_title);
    let pid = ProcessId::new();
    let e = app.world_mut().spawn((Terminal, pid)).id();

    app.world_mut()
        .resource_mut::<Messages<OscTitleChanged>>()
        .write(OscTitleChanged {
            process_id: pid,
            title: "claude — repo".to_string(),
        });
    app.update();
    assert_eq!(
        app.world()
            .get::<vmux_core::OscTitle>(e)
            .map(|o| o.0.clone()),
        Some("claude — repo".to_string())
    );

    app.world_mut()
        .resource_mut::<Messages<OscTitleChanged>>()
        .write(OscTitleChanged {
            process_id: pid,
            title: String::new(),
        });
    app.update();
    assert!(app.world().get::<vmux_core::OscTitle>(e).is_none());
}

#[test]
fn clear_osc_title_on_exit_removes_override() {
    use bevy::ecs::message::Messages;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<ProcessExitedEvent>()
        .add_systems(Update, clear_osc_title_on_exit);
    let pid = ProcessId::new();
    let e = app
        .world_mut()
        .spawn((Terminal, pid, vmux_core::OscTitle("working".to_string())))
        .id();

    app.world_mut()
        .resource_mut::<Messages<ProcessExitedEvent>>()
        .write(ProcessExitedEvent { process_id: pid });
    app.update();
    assert!(app.world().get::<vmux_core::OscTitle>(e).is_none());
}

#[test]
fn retained_terminal_stays_in_service_query_after_exit() {
    let mut world = World::new();
    let entity = world
        .spawn((Terminal, ProcessExited, RetainOnProcessExit))
        .id();
    let mut query = world.query_filtered::<Entity, ServiceTerminalFilter>();

    assert!(query.get(&world, entity).is_ok());
}

#[test]
fn retained_terminal_does_not_close_stack_on_exit() {
    assert!(!should_close_terminal_stack_on_exit(false, true));
}

#[test]
fn agent_run_terminal_inherits_login_shell_environment() {
    assert!(should_merge_login_shell_env(false, true));
    assert!(should_merge_login_shell_env(true, false));
    assert!(!should_merge_login_shell_env(false, false));
}

fn term_theme(font_size: f32) -> vmux_setting::TerminalTheme {
    vmux_setting::TerminalTheme {
        name: "default".to_string(),
        color_scheme: "catppuccin-mocha".to_string(),
        font_family: "JetBrainsMono Nerd Font".to_string(),
        font_size,
        line_height: 1.2,
        padding: 4.0,
        cursor_style: "block".to_string(),
        cursor_blink: true,
        shell: "/bin/sh".to_string(),
    }
}

fn settings_with_font(font_size: f32) -> AppSettings {
    let mut s = test_settings();
    s.terminal = Some(vmux_setting::TerminalSettings {
        default_theme: "default".to_string(),
        themes: vec![term_theme(font_size)],
        ..Default::default()
    });
    s
}

fn run_font_size_command(start: f32, cmd: TerminalFontSizeCommand) -> (f32, usize) {
    use bevy::ecs::message::Messages;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings_with_font(start))
        .add_message::<TerminalFontSizeCommand>()
        .add_message::<SettingsSaveRequest>()
        .add_systems(Update, handle_terminal_font_size);
    app.world_mut()
        .resource_mut::<Messages<TerminalFontSizeCommand>>()
        .write(cmd);
    app.update();
    let size = app
        .world()
        .resource::<AppSettings>()
        .terminal
        .as_ref()
        .unwrap()
        .themes[0]
        .font_size;
    let saves = app
        .world_mut()
        .resource_mut::<Messages<SettingsSaveRequest>>()
        .drain()
        .count();
    (size, saves)
}

#[test]
fn font_size_materializes_missing_default_theme() {
    use bevy::ecs::message::Messages;
    let mut settings = test_settings();
    settings.terminal = Some(vmux_setting::TerminalSettings {
        default_theme: "default".to_string(),
        themes: Vec::new(),
        ..Default::default()
    });
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings)
        .add_message::<TerminalFontSizeCommand>()
        .add_message::<SettingsSaveRequest>()
        .add_systems(Update, handle_terminal_font_size);
    app.world_mut()
        .resource_mut::<Messages<TerminalFontSizeCommand>>()
        .write(TerminalFontSizeCommand::Increase);
    app.update();

    let terminal = app
        .world()
        .resource::<AppSettings>()
        .terminal
        .clone()
        .unwrap();
    let theme = terminal
        .themes
        .iter()
        .find(|t| t.name == "default")
        .expect("missing default theme must be materialized so zoom persists");
    assert_eq!(theme.font_size, 15.0);
    let saves = app
        .world_mut()
        .resource_mut::<Messages<SettingsSaveRequest>>()
        .drain()
        .count();
    assert_eq!(saves, 1);
}

#[test]
fn font_size_increase_steps_up_and_persists() {
    let (size, writes) = run_font_size_command(14.0, TerminalFontSizeCommand::Increase);
    assert_eq!(size, 15.0);
    assert_eq!(writes, 1);
}

#[test]
fn font_size_decrease_steps_down_and_persists() {
    let (size, writes) = run_font_size_command(14.0, TerminalFontSizeCommand::Decrease);
    assert_eq!(size, 13.0);
    assert_eq!(writes, 1);
}

#[test]
fn font_size_increase_clamps_at_40() {
    let (size, _) = run_font_size_command(40.0, TerminalFontSizeCommand::Increase);
    assert_eq!(size, 40.0);
}

#[test]
fn font_size_decrease_clamps_at_6() {
    let (size, _) = run_font_size_command(6.0, TerminalFontSizeCommand::Decrease);
    assert_eq!(size, 6.0);
}

#[test]
fn font_size_reset_returns_to_14() {
    let (size, writes) = run_font_size_command(20.0, TerminalFontSizeCommand::Reset);
    assert_eq!(size, 14.0);
    assert_eq!(writes, 1);
}

#[test]
fn theme_signature_changes_with_font_size() {
    let colors = vmux_setting::themes::resolve_theme("catppuccin-mocha", &[]);
    let small = term_theme(14.0);
    let large = term_theme(15.0);
    assert_ne!(
        theme_signature(&small, &colors),
        theme_signature(&large, &colors)
    );
}
