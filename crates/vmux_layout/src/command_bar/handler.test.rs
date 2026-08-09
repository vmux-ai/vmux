use super::*;
use crate::command_bar::state::CommandBarState;
use bevy::ecs::schedule::{NodeId, Schedules, SystemSet};
use vmux_command::event::CommandBarSpace;
use vmux_command::{CommandPlugin, ReadAppCommands};

#[test]
fn build_payload_includes_commands_and_target() {
    let pages = CommandBarPagesSnapshot::default();
    let spaces = CommandBarSpacesSnapshot::default();
    let agents = CommandBarContributions::default();
    let work = vmux_command::snapshot::CommandBarWorkSnapshot::default();
    let payload = build_command_bar_open_payload(
        7,
        false,
        String::new(),
        String::new(),
        &spaces,
        &agents,
        &pages,
        &work,
        "en-US",
        0,
        Vec::new(),
        Some(OpenTarget::InPlace),
    );
    assert_eq!(payload.open_id, 7);
    assert_eq!(payload.target, Some(OpenTarget::InPlace));
    assert!(!payload.commands.is_empty());
}

#[test]
fn command_names_localize_every_hierarchy_segment() {
    assert_eq!(
        localized_command_name("ja", "browser_prev_page", "fallback".to_string()),
        "ブラウザ > ナビゲーション > 戻る"
    );
    assert_eq!(
        localized_command_name("ja", "close_pane", "fallback".to_string()),
        "レイアウト > ペイン > ペインを閉じる"
    );
}

#[test]
fn command_bar_open_payload_retries_until_rendered_ack() {
    assert!(should_retry_command_bar_open_payload(
        7,
        Some(b"payload"),
        None
    ));
    assert!(should_retry_command_bar_open_payload(
        7,
        Some(b"payload"),
        Some(6)
    ));
    assert!(!should_retry_command_bar_open_payload(
        7,
        Some(b"payload"),
        Some(7)
    ));
    assert!(!should_retry_command_bar_open_payload(
        0,
        Some(b"payload"),
        None
    ));
    assert!(!should_retry_command_bar_open_payload(7, None, None));
}

#[test]
fn command_bar_open_retry_uses_binary_host_emit() {
    let source = include_str!("handler.rs");
    let retry_fn = source
        .split("fn retry_pending_command_bar_open")
        .nth(1)
        .and_then(|tail| tail.split("fn mark_command_bar_painted").next())
        .unwrap_or_default();

    assert!(retry_fn.contains("BinHostEmitEvent::from_bytes"));
    assert!(!retry_fn.contains("HostEmitEvent::new"));
}

#[derive(Resource, Default)]
struct CapturedCommandBarOpen(bool);

fn capture_command_bar_open(
    modal_q: CommandBarStateQuery,
    mut captured: ResMut<CapturedCommandBarOpen>,
) {
    captured.0 = is_command_bar_open(&modal_q);
}

#[test]
fn hidden_prewarmed_modal_is_not_command_bar_open() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<CapturedCommandBarOpen>()
        .add_systems(Update, capture_command_bar_open);
    app.world_mut().spawn((
        Modal,
        Node {
            display: Display::Flex,
            ..default()
        },
        Visibility::Hidden,
    ));

    app.update();

    assert!(!app.world().resource::<CapturedCommandBarOpen>().0);
}

#[test]
fn closed_native_overlay_stays_renderable_without_being_open() {
    let mut node = Node::default();
    let mut visibility = Visibility::Hidden;

    close_command_bar_surface(&mut node, &mut visibility, true);

    assert_eq!(node.display, Display::Flex);
    assert_eq!(visibility, Visibility::Inherited);
    assert!(!CommandBarState::from_modal(node.display, visibility, false).owns_input());
}

#[test]
fn command_bar_modal_prewarms_hidden_and_renderable() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, prewarm_command_bar_modal);
    let modal = app
        .world_mut()
        .spawn((
            Modal,
            Node {
                display: Display::None,
                ..default()
            },
            Visibility::Hidden,
        ))
        .id();

    app.update();

    let node = app.world().get::<Node>(modal).unwrap();
    let visibility = app.world().get::<Visibility>(modal).unwrap();
    let reveal = app.world().get::<PendingCommandBarReveal>(modal).unwrap();

    assert_eq!(node.display, Display::Flex);
    assert_eq!(*visibility, Visibility::Hidden);
    assert_eq!(reveal.open_id, 0);
    assert!(app.world().get::<CefKeyboardTarget>(modal).is_none());
    assert_eq!(
        app.world().get::<bevy::picking::Pickable>(modal),
        Some(&bevy::picking::Pickable::IGNORE)
    );
}

#[test]
fn ready_command_bar_modal_still_prewarms_hidden_and_renderable() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, prewarm_command_bar_modal);
    let modal = app
        .world_mut()
        .spawn((
            Modal,
            CommandBarReady,
            Node {
                display: Display::None,
                ..default()
            },
            Visibility::Hidden,
        ))
        .id();

    app.update();

    let node = app.world().get::<Node>(modal).unwrap();
    let visibility = app.world().get::<Visibility>(modal).unwrap();
    let reveal = app.world().get::<PendingCommandBarReveal>(modal).unwrap();

    assert_eq!(node.display, Display::Flex);
    assert_eq!(*visibility, Visibility::Hidden);
    assert_eq!(reveal.open_id, 0);
}

#[test]
fn command_bar_reveal_waits_for_matching_open_id() {
    assert_eq!(next_command_bar_reveal_frames(1, 7, None, None), Some(2));
    assert_eq!(
        next_command_bar_reveal_frames(1, 7, Some(6), Some(7)),
        Some(2)
    );
    assert_eq!(
        next_command_bar_reveal_frames(0, 7, Some(7), Some(7)),
        Some(1)
    );
    assert_eq!(next_command_bar_reveal_frames(2, 7, Some(7), Some(7)), None);
}

#[test]
fn command_bar_reveal_falls_back_when_rendered_event_is_missing() {
    assert_eq!(next_command_bar_reveal_frames(0, 7, None, None), Some(1));
    assert_eq!(next_command_bar_reveal_frames(10, 7, None, None), None);
    assert_eq!(
        next_command_bar_reveal_frames(10, 7, Some(6), Some(7)),
        None
    );
}

#[test]
fn command_bar_reveal_does_not_require_texture_after_rendered_event() {
    assert_eq!(next_command_bar_reveal_frames(2, 7, Some(7), None), None);
    assert_eq!(next_command_bar_reveal_frames(2, 7, Some(7), Some(7)), None);
}

#[test]
fn native_command_bar_waits_for_size_and_rendered_ack() {
    assert_eq!(
        next_command_bar_reveal_frames_for_backend(true, false, 10, 7, None, None, true),
        Some(11)
    );
    assert_eq!(
        next_command_bar_reveal_frames_for_backend(true, false, 10, 7, Some(7), None, false,),
        Some(11)
    );
    assert_eq!(
        next_command_bar_reveal_frames_for_backend(true, false, 2, 7, Some(7), None, true,),
        None
    );
}

#[test]
fn native_command_bar_aborts_stalled_reveal() {
    assert!(!native_command_bar_reveal_timed_out(
        true,
        false,
        COMMAND_BAR_NATIVE_REVEAL_TIMEOUT - Duration::from_millis(1),
        7,
        None,
        false,
    ));
    assert!(native_command_bar_reveal_timed_out(
        true,
        false,
        COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
        7,
        None,
        false,
    ));
    assert!(native_command_bar_reveal_timed_out(
        true,
        false,
        COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
        7,
        Some(7),
        false,
    ));
    assert!(!native_command_bar_reveal_timed_out(
        true,
        false,
        COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
        7,
        Some(7),
        true,
    ));
    assert!(!native_command_bar_reveal_timed_out(
        false,
        false,
        COMMAND_BAR_NATIVE_REVEAL_TIMEOUT,
        7,
        None,
        false,
    ));
}

#[test]
fn native_overlay_waits_for_rendered_ack() {
    assert_eq!(
        next_command_bar_reveal_frames_for_backend(false, true, 10, 7, None, None, false,),
        Some(11)
    );
    assert_eq!(
        next_command_bar_reveal_frames_for_backend(false, true, 2, 7, Some(7), None, false,),
        None
    );
}

#[test]
fn native_command_bar_stalled_reveal_stays_hidden() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, reveal_command_bar);
    let modal = app
        .world_mut()
        .spawn((
            Modal,
            WebviewWindowed,
            Visibility::Hidden,
            PendingCommandBarReveal {
                frames: u8::MAX,
                open_id: 7,
                payload: Some(b"payload".to_vec()),
                started_at: Some(Instant::now() - COMMAND_BAR_NATIVE_REVEAL_TIMEOUT),
            },
        ))
        .id();

    app.update();

    assert!(app.world().get::<PendingCommandBarReveal>(modal).is_none());
    assert_eq!(
        app.world().get::<Visibility>(modal),
        Some(&Visibility::Hidden)
    );
}

#[test]
fn native_command_bar_does_not_timeout_from_rapid_updates() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, reveal_command_bar);
    let modal = app
        .world_mut()
        .spawn((
            Modal,
            WebviewWindowed,
            Visibility::Hidden,
            PendingCommandBarReveal {
                frames: 0,
                open_id: 7,
                payload: Some(b"payload".to_vec()),
                started_at: Some(Instant::now()),
            },
        ))
        .id();

    for _ in 0..256 {
        app.update();
    }

    assert!(app.world().get::<PendingCommandBarReveal>(modal).is_some());
    assert_eq!(
        app.world().get::<Visibility>(modal),
        Some(&Visibility::Hidden)
    );
}

#[test]
fn native_command_bar_ignores_hidden_prewarm_size() {
    assert!(!command_bar_size_should_apply(Visibility::Hidden, None));
    assert!(command_bar_size_should_apply(Visibility::Inherited, None));
}

#[test]
fn native_command_bar_accepts_hidden_open_size() {
    let pending = PendingCommandBarReveal {
        frames: 0,
        open_id: 7,
        payload: Some(Vec::new()),
        started_at: Some(Instant::now()),
    };

    assert!(command_bar_size_should_apply(
        Visibility::Hidden,
        Some(&pending)
    ));
    assert_eq!(
        next_command_bar_reveal_frames_for_backend(true, false, 0, 7, None, None, true),
        Some(1)
    );
}

#[test]
fn command_bar_paint_before_rendered_ack_still_allows_reveal() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<RenderTextureMessage>()
        .add_systems(
            Update,
            (mark_command_bar_painted, reveal_command_bar).chain(),
        );

    let modal = app
        .world_mut()
        .spawn((
            Modal,
            Visibility::Hidden,
            PendingCommandBarReveal {
                frames: 2,
                open_id: 7,
                payload: Some(b"payload".to_vec()),
                started_at: Some(Instant::now()),
            },
        ))
        .id();
    app.world_mut()
        .resource_mut::<Messages<RenderTextureMessage>>()
        .write(RenderTextureMessage {
            webview: modal,
            ty: bevy_cef_core::prelude::RenderPaintElementType::View,
            width: 1,
            height: 1,
            patches: std::sync::Arc::new(
                [bevy_cef_core::prelude::WebviewPaintPatch {
                    rect: bevy_cef_core::prelude::WebviewDirtyRect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    buffer: std::sync::Arc::new(vec![0, 0, 0, 255]),
                }]
                .into_iter()
                .collect(),
            ),
            dirty: Default::default(),
        });

    app.update();
    app.world_mut()
        .entity_mut(modal)
        .insert(CommandBarRenderedOpen(7));
    app.update();

    assert!(app.world().get::<CommandBarPaintedOpen>(modal).is_some());
    assert!(app.world().get::<PendingCommandBarReveal>(modal).is_none());
    assert_eq!(
        app.world().get::<Visibility>(modal),
        Some(&Visibility::Inherited)
    );
}

#[test]
fn command_bar_payload_includes_space_name() {
    let payload = command_bar_open_payload(
        7,
        false,
        "Work".to_string(),
        "https://example.com".to_string(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );

    assert_eq!(payload.space_name, "Work");
    assert_eq!(payload.open_id, 7);
}

#[test]
fn command_bar_payload_includes_spaces() {
    let spaces = vec![CommandBarSpace {
        id: "work".to_string(),
        name: "Work".to_string(),
        profile: "Personal".to_string(),
        is_active: true,
        tab_count: 2,
    }];

    let payload = command_bar_open_payload(
        8,
        true,
        "Work".to_string(),
        "vmux://spaces/".to_string(),
        spaces.clone(),
        Vec::new(),
        Vec::new(),
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );

    assert_eq!(payload.spaces, spaces);
    assert!(payload.native_windowed);
}

#[test]
fn space_open_command_opens_space_switch_mode() {
    let request =
        command_bar_open_request([AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open))]);

    assert!(request.should_toggle);
    assert!(request.space_switch);
    assert_eq!(request.url_override, Some(String::new()));
}

#[test]
fn command_bar_focuses_start_only_for_generic_open() {
    assert!(command_bar_should_focus_start(false, false, true, false));
    assert!(!command_bar_should_focus_start(false, true, true, false));
    assert!(!command_bar_should_focus_start(true, false, true, false));
    assert!(!command_bar_should_focus_start(false, false, false, false));
    assert!(!command_bar_should_focus_start(false, false, true, true));
}

#[test]
fn duplicate_open_is_ignored_while_command_bar_is_visible() {
    assert!(command_bar_toggle_should_open(false, false));
    assert!(!command_bar_toggle_should_open(true, false));
    assert!(command_bar_toggle_should_open(true, true));
    assert!(command_bar_toggle_should_open(false, true));
}

#[test]
fn open_in_new_stack_does_not_dismiss_command_bar() {
    let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Open(
        OpenCommand::InNewStack { url: None },
    ))]);

    assert!(!request.should_dismiss);
}

#[test]
fn open_command_bar_forces_empty_url_override() {
    let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Bar(
        BrowserBarCommand::OpenCommandBar,
    ))]);

    assert!(request.should_toggle);
    assert_eq!(request.url_override, Some(String::new()));
}

#[test]
fn open_page_in_command_bar_leaves_url_override_unset_so_current_url_is_prefilled() {
    let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Bar(
        BrowserBarCommand::OpenPageInCommandBar,
    ))]);

    assert!(request.should_toggle);
    assert_eq!(request.url_override, None);
}

#[derive(Resource, Default)]
struct EmittedToPage(Vec<(Entity, String, Vec<u8>)>);

fn capture_page_emit(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<EmittedToPage>) {
    emitted
        .0
        .push((trigger.webview, trigger.id.clone(), trigger.payload.clone()));
}

fn panel_app() -> App {
    let mut app = App::new();
    app.add_message::<AppCommand>()
        .add_message::<PageOpenRequest>()
        .init_resource::<CommandBarContributions>()
        .init_resource::<CommandBarSpacesSnapshot>()
        .init_resource::<CommandBarPagesSnapshot>()
        .init_resource::<vmux_command::snapshot::CommandBarWorkSnapshot>()
        .init_resource::<NewStackContext>()
        .init_resource::<EmittedToPage>()
        .add_observer(capture_page_emit)
        .add_systems(Update, handle_open_command_bar);
    app
}

fn emitted_to_page(app: &App) -> Vec<(Entity, String)> {
    app.world()
        .resource::<EmittedToPage>()
        .0
        .iter()
        .map(|(webview, id, _)| (*webview, id.clone()))
        .collect()
}

fn open_payload(app: &App) -> CommandBarOpenEvent {
    let (_, _, bytes) = app
        .world()
        .resource::<EmittedToPage>()
        .0
        .iter()
        .find(|(_, id, _)| id == LAYOUT_COMMAND_BAR_OPEN_EVENT)
        .expect("no open payload emitted");
    rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(bytes)
        .expect("open payload should round-trip")
}

fn send(app: &mut App, command: AppCommand) {
    app.world_mut().write_message(command);
    app.update();
}

/// The panel lives in the layout page, so the payload has to reach the layout webview under the
/// layout-specific id. Addressing the modal instead leaves the bar permanently empty.
#[test]
fn opening_the_command_bar_pushes_the_payload_to_the_layout_page() {
    let mut app = panel_app();
    let layout = app.world_mut().spawn(LayoutCef).id();

    send(
        &mut app,
        AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar)),
    );

    assert_eq!(
        emitted_to_page(&app),
        vec![(layout, LAYOUT_COMMAND_BAR_OPEN_EVENT.to_string())]
    );
}

/// `Cmd+K` while the panel is up must close it. The host cannot unmount the panel itself, so a
/// missing close event turns the toggle into a no-op and the bar can never be dismissed.
#[test]
fn toggling_an_open_command_bar_asks_the_page_to_close_it() {
    let mut app = panel_app();
    let layout = app
        .world_mut()
        .spawn((LayoutCef, CommandBarPanelActive))
        .id();

    send(
        &mut app,
        AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar)),
    );

    assert_eq!(
        emitted_to_page(&app),
        vec![(layout, LAYOUT_COMMAND_BAR_CLOSE_EVENT.to_string())]
    );
}

/// `Cmd+T` then `Cmd+K` must discard the empty stack the first command staged. Closing by
/// toggle used to return before the dismiss cleanup, orphaning the tab and leaving no browser
/// holding `CefKeyboardTarget`.
#[test]
fn toggling_closed_discards_the_stack_a_pending_new_tab_staged() {
    let mut app = panel_app();
    let layout = app
        .world_mut()
        .spawn((LayoutCef, CommandBarPanelActive))
        .id();
    let pending = app.world_mut().spawn_empty().id();
    app.insert_resource(NewStackContext {
        stack: Some(pending),
        previous_stack: None,
        needs_open: true,
        dismiss_modal: false,
    });

    send(
        &mut app,
        AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar)),
    );

    assert_eq!(
        emitted_to_page(&app),
        vec![(layout, LAYOUT_COMMAND_BAR_CLOSE_EVENT.to_string())]
    );
    assert!(app.world().get_entity(pending).is_err());
    let ctx = app.world().resource::<NewStackContext>();
    assert!(ctx.stack.is_none());
    assert!(!ctx.needs_open);
}

/// A space switch reopens rather than toggles, so the bar survives switching spaces from
/// inside it.
#[test]
fn space_switch_reopens_an_already_open_command_bar() {
    let mut app = panel_app();
    let layout = app
        .world_mut()
        .spawn((LayoutCef, CommandBarPanelActive))
        .id();

    send(
        &mut app,
        AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open)),
    );

    assert_eq!(
        emitted_to_page(&app),
        vec![(layout, LAYOUT_COMMAND_BAR_OPEN_EVENT.to_string())]
    );
    assert!(open_payload(&app).space_switch);
}

#[test]
fn open_page_in_command_bar_marks_payload_as_in_place_target() {
    let mut app = panel_app();
    app.world_mut().spawn(LayoutCef);

    send(
        &mut app,
        AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenPageInCommandBar)),
    );

    assert_eq!(open_payload(&app).target, Some(OpenTarget::InPlace));
}

#[test]
fn open_page_in_command_bar_cancels_pending_new_stack_context() {
    let pending_stack = Entity::from_bits(7);
    let previous_stack = Entity::from_bits(6);
    let request = command_bar_open_request([AppCommand::Browser(BrowserCommand::Bar(
        BrowserBarCommand::OpenPageInCommandBar,
    ))]);
    let mut ctx = NewStackContext {
        stack: Some(pending_stack),
        previous_stack: Some(previous_stack),
        needs_open: true,
        dismiss_modal: false,
    };

    assert!(request.replace_active_stack);
    assert!(!command_bar_should_open_pending_stack(&mut ctx, true));
    let canceled =
        command_bar_cancel_pending_stack_for_active_open(&mut ctx, request.replace_active_stack);

    assert_eq!(canceled, Some((pending_stack, Some(previous_stack))));
    assert_eq!(ctx.stack, None);
    assert_eq!(ctx.previous_stack, None);
    assert!(!ctx.needs_open);
}

#[test]
fn pending_stack_with_startup_url_dispatches_url_request() {
    let stack = Entity::from_bits(7);
    let previous_stack = Entity::from_bits(6);
    let mut ctx = NewStackContext {
        stack: Some(stack),
        previous_stack: Some(previous_stack),
        needs_open: true,
        dismiss_modal: false,
    };

    let request =
        pending_stack_startup_url_request(&mut ctx, Some("https://startup.test")).unwrap();

    match request.target {
        PageOpenTarget::Stack(target) => assert_eq!(target, stack),
        other => panic!("expected stack target, got {other:?}"),
    }
    assert_eq!(request.url, "https://startup.test");
    assert_eq!(ctx.stack, None);
    assert_eq!(ctx.previous_stack, None);
    assert!(!ctx.needs_open);
}

#[test]
fn pending_stack_without_startup_url_keeps_prompt_pending() {
    let stack = Entity::from_bits(7);
    let mut ctx = NewStackContext {
        stack: Some(stack),
        previous_stack: None,
        needs_open: true,
        dismiss_modal: false,
    };

    let request = pending_stack_startup_url_request(&mut ctx, Some(""));

    assert!(request.is_none());
    assert_eq!(ctx.stack, Some(stack));
    assert!(ctx.needs_open);
}

#[test]
fn dismiss_action_closes_command_bar_modal_in_one_pass() {
    use bevy::ecs::system::RunSystemOnce;

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .add_plugins(crate::stack::StackPlugin)
        .add_plugins(CommandBarInputPlugin)
        .add_message::<TerminalSpawnRequest>()
        .add_message::<vmux_core::terminal::ProcessesMonitorSpawnRequest>()
        .add_message::<crate::LayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
        .init_resource::<crate::pane::PendingCursorWarp>()
        .insert_resource(bevy_cef::prelude::CefSuppressKeyboardInput::default());

    let modal = app
        .world_mut()
        .spawn((
            Modal,
            Node {
                display: Display::Flex,
                ..default()
            },
            Visibility::Inherited,
            CefKeyboardTarget,
            CommandBarRenderedOpen(1),
        ))
        .id();

    app.world_mut()
        .trigger(BinReceive::<CommandBarActionEvent> {
            webview: modal,
            payload: CommandBarActionEvent {
                action: "dismiss".to_string(),
                value: String::new(),
                target: None,
                target_url: None,
                attachments: Vec::new(),
            },
        });
    app.world_mut().flush();

    let vis_after_close = *app.world().get::<Visibility>(modal).unwrap();
    let display_after_close = app.world().get::<Node>(modal).unwrap().display;
    let has_kb_after_close = app.world().get::<CefKeyboardTarget>(modal).is_some();
    let has_rendered_after_close = app.world().get::<CommandBarRenderedOpen>(modal).is_some();
    let has_painted_after_close = app.world().get::<CommandBarPaintedOpen>(modal).is_some();
    let has_pending_after_close = app.world().get::<PendingCommandBarReveal>(modal).is_some();

    assert_eq!(
        vis_after_close,
        Visibility::Hidden,
        "modal should be hidden after dismiss"
    );
    assert_eq!(
        display_after_close,
        Display::None,
        "modal should have display None after dismiss"
    );
    assert!(
        !has_kb_after_close,
        "CefKeyboardTarget should be removed after dismiss"
    );
    assert!(
        !has_rendered_after_close,
        "CommandBarRenderedOpen should be cleared after dismiss"
    );
    assert!(
        !has_painted_after_close,
        "CommandBarPaintedOpen should be cleared after dismiss"
    );
    assert!(
        !has_pending_after_close,
        "PendingCommandBarReveal should be cleared after dismiss"
    );

    app.world_mut()
        .run_system_once(prewarm_command_bar_modal)
        .unwrap();

    let vis_after_prewarm = *app.world().get::<Visibility>(modal).unwrap();
    let display_after_prewarm = app.world().get::<Node>(modal).unwrap().display;
    let has_kb_after_prewarm = app.world().get::<CefKeyboardTarget>(modal).is_some();
    let pending_open_id_after_prewarm = app
        .world()
        .get::<PendingCommandBarReveal>(modal)
        .map(|p| p.open_id);

    assert_eq!(
        vis_after_prewarm,
        Visibility::Hidden,
        "modal must stay hidden after prewarm"
    );
    assert!(
        !has_kb_after_prewarm,
        "CefKeyboardTarget must not return after prewarm"
    );
    assert!(
        !CommandBarState::from_modal(
            display_after_prewarm,
            Visibility::Hidden,
            has_kb_after_prewarm
        )
        .owns_input(),
        "is_command_bar_open must report false after dismiss + prewarm"
    );
    if let Some(open_id) = pending_open_id_after_prewarm {
        assert_eq!(
            open_id, 0,
            "prewarm should re-arm reveal at open_id=0 (which never fires until handle_open_command_bar bumps it)"
        );
    }
}

#[test]
fn command_bar_open_runs_after_tab_commands() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .add_plugins(crate::stack::StackPlugin)
        .add_plugins(CommandBarInputPlugin);

    let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
    let mut update = schedules.remove(Update).unwrap();
    update.initialize(app.world_mut()).unwrap();
    let graph = update.graph();
    let tab_command_set = graph
        .system_sets
        .get_key(crate::stack::StackCommandSet.intern())
        .unwrap();
    let read_command_systems = graph.systems_in_set(ReadAppCommands.intern()).unwrap();
    let tab_command_systems = graph
        .systems_in_set(crate::stack::StackCommandSet.intern())
        .unwrap();
    let command_bar_open_system = read_command_systems
        .iter()
        .copied()
        .find(|system| !tab_command_systems.contains(system))
        .unwrap();

    assert!(graph.dependency().graph().contains_edge(
        NodeId::Set(tab_command_set),
        NodeId::System(command_bar_open_system)
    ));
}

const TEST_TERMINAL_URL: &str = "vmux://terminal/";

#[test]
fn parse_pid_from_url_accepts_numeric() {
    assert_eq!(
        parse_pid_from_url("vmux://terminal/12345", TEST_TERMINAL_URL),
        Some(12345)
    );
    assert_eq!(
        parse_pid_from_url("vmux://terminal/0", TEST_TERMINAL_URL),
        Some(0)
    );
}

#[test]
fn parse_pid_from_url_rejects_uuid_form() {
    let uuid_url = "vmux://terminal/ae724a54-c387-5359-0687-ccfc155558b6";
    assert_eq!(parse_pid_from_url(uuid_url, TEST_TERMINAL_URL), None);
}

#[test]
fn parse_pid_from_url_rejects_empty_path() {
    assert_eq!(
        parse_pid_from_url("vmux://terminal/", TEST_TERMINAL_URL),
        None
    );
}

#[test]
fn parse_pid_from_url_rejects_overflow() {
    assert_eq!(
        parse_pid_from_url("vmux://terminal/99999999999999999", TEST_TERMINAL_URL),
        None
    );
}

#[test]
fn build_open_command_none_target_yields_in_place() {
    let cmd = build_open_command(None, "https://example.com".to_string());
    assert_eq!(
        cmd,
        OpenCommand::InPlace {
            url: Some("https://example.com".to_string())
        }
    );
}

#[test]
fn build_open_command_in_place_target_yields_in_place() {
    let cmd = build_open_command(Some(OpenTarget::InPlace), "https://example.com".to_string());
    assert_eq!(
        cmd,
        OpenCommand::InPlace {
            url: Some("https://example.com".to_string())
        }
    );
}

#[test]
fn build_open_command_in_new_stack_target() {
    let cmd = build_open_command(
        Some(OpenTarget::InNewStack),
        "https://example.com".to_string(),
    );
    assert_eq!(
        cmd,
        OpenCommand::InNewStack {
            url: Some("https://example.com".to_string())
        }
    );
}

#[test]
fn build_open_command_in_new_tab_target() {
    let cmd = build_open_command(
        Some(OpenTarget::InNewTab),
        "https://example.com".to_string(),
    );
    assert_eq!(
        cmd,
        OpenCommand::InNewTab {
            url: Some("https://example.com".to_string())
        }
    );
}

#[test]
fn build_open_command_in_new_space_target() {
    let cmd = build_open_command(
        Some(OpenTarget::InNewSpace),
        "https://example.com".to_string(),
    );
    assert_eq!(
        cmd,
        OpenCommand::InNewSpace {
            url: Some("https://example.com".to_string())
        }
    );
}

#[test]
fn build_open_command_in_pane_target() {
    use vmux_command::open_target::{PaneDirection, PaneOpenMode, PaneTarget};
    let cmd = build_open_command(
        Some(OpenTarget::InPane {
            direction: PaneDirection::Right,
            target: PaneTarget::NewSplit,
            mode: PaneOpenMode::NewStack,
        }),
        "https://example.com".to_string(),
    );
    assert_eq!(
        cmd,
        OpenCommand::InPane {
            direction: PaneDirection::Right,
            target: PaneTarget::NewSplit,
            mode: PaneOpenMode::NewStack,
            url: Some("https://example.com".to_string()),
        }
    );
}

#[test]
fn normalize_url_adds_https_for_domain() {
    assert_eq!(
        normalize_url("google.com", SearchEngine::Google),
        "https://google.com"
    );
}

#[test]
fn normalize_url_preserves_explicit_protocol() {
    assert_eq!(
        normalize_url("http://example.com", SearchEngine::Google),
        "http://example.com"
    );
    assert_eq!(
        normalize_url("https://example.com", SearchEngine::Google),
        "https://example.com"
    );
}

#[test]
fn normalize_url_search_query_becomes_google() {
    assert_eq!(
        normalize_url("hello world", SearchEngine::Google),
        "https://www.google.com/search?q=hello+world"
    );
}

#[test]
fn normalize_url_uses_selected_search_engine() {
    assert_eq!(
        normalize_url("hello world", SearchEngine::DuckDuckGo),
        "https://duckduckgo.com/?q=hello+world"
    );
}

#[test]
fn normalize_url_searches_multiline_text_with_embedded_url() {
    let prompt = "Continue DSK-627\nPR: https://github.com/example/repo/pull/1";
    assert_eq!(
        normalize_url(prompt, SearchEngine::Google),
        "https://www.google.com/search?q=Continue+DSK-627%0APR%3A+https%3A%2F%2Fgithub.com%2Fexample%2Frepo%2Fpull%2F1"
    );
}

#[test]
fn normalize_url_preserves_vmux_protocol() {
    assert_eq!(
        normalize_url("vmux://terminal/123", SearchEngine::Google),
        "vmux://terminal/123"
    );
}

#[test]
fn normalize_url_preserves_data_scheme() {
    let data = "data:text/html,<style>body{background:white}</style><h1>x</h1>";
    assert_eq!(normalize_url(data, SearchEngine::Google), data);
}

#[test]
fn pending_reveal_is_active_only_with_real_open_id() {
    assert!(
        !PendingCommandBarReveal {
            frames: 0,
            open_id: 0,
            payload: None,
            started_at: None,
        }
        .is_active()
    );
    assert!(
        PendingCommandBarReveal {
            frames: 0,
            open_id: 7,
            payload: None,
            started_at: Some(Instant::now()),
        }
        .is_active()
    );
}
