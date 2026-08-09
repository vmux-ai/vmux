use super::*;
use bevy_cef::prelude::BinReceive;
use vmux_core::PageOpenId;
use vmux_core::page::PageManifest;

#[derive(Resource, Default)]
struct EmittedIds(Vec<String>);

fn capture_emit(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<EmittedIds>) {
    emitted.0.push(trigger.id.clone());
}

fn start_ready_app() -> App {
    let mut app = App::new();
    app.init_resource::<CommandBarSpacesSnapshot>()
        .init_resource::<CommandBarContributions>()
        .init_resource::<CommandBarPagesSnapshot>()
        .init_resource::<CommandBarWorkSnapshot>()
        .init_resource::<EmittedIds>()
        .add_observer(on_start_data_request)
        .add_observer(capture_emit);
    app
}

fn emit_start_ready(app: &mut App, webview: Entity) {
    app.world_mut().trigger(BinReceive {
        webview,
        payload: StartDataRequest,
    });
    app.update();
}

#[test]
fn start_plugin_spawns_manifest() {
    let mut app = App::new();
    app.add_plugins(StartPlugin);
    let mut q = app.world_mut().query::<&PageManifest>();
    assert!(q.iter(app.world()).any(|m| m.host == "start"));
}

#[test]
fn inline_transition_only_supports_page_agents() {
    assert!(crate::start::supports_inline_agent_transition(
        "vmux://agent/codex"
    ));
    assert!(crate::start::supports_inline_agent_transition(
        "vmux://agent/openai/gpt-5/session"
    ));
    assert!(!crate::start::supports_inline_agent_transition(
        "vmux://agent/codex/cli"
    ));
    assert!(!crate::start::supports_inline_agent_transition(
        "vmux://agent/vibe/setup"
    ));
    assert!(crate::start::supports_inline_agent_transition(
        "vmux://agent/cliff"
    ));
    assert!(crate::start::supports_inline_agent_transition(
        "vmux://agent/setupwizard"
    ));
}

#[test]
fn page_mount_does_not_start_focus_retry() {
    let source = include_str!("page.rs");
    let setup_effect = source
        .split_once("use_effect(|| {")
        .expect("start page setup effect")
        .1
        .split_once("});")
        .expect("end of start page setup effect")
        .0;

    assert!(setup_effect.contains("install_window_focus_refocus();"));
    assert!(setup_effect.contains("install_keep_input_focused_on_click();"));
    assert!(!setup_effect.contains("focus_start_input();"));
}

#[test]
fn cold_start_focuses_after_page_ready() {
    let mut app = start_ready_app();
    let webview = app.world_mut().spawn(CefKeyboardTarget).id();

    emit_start_ready(&mut app, webview);

    let emitted = &app.world().resource::<EmittedIds>().0;
    assert_eq!(
        emitted,
        &[START_COMMAND_BAR_OPEN_EVENT, START_FOCUS_INPUT_EVENT]
    );
}

#[test]
fn warm_start_waits_for_reveal_before_focusing() {
    let mut app = start_ready_app();
    let webview = app.world_mut().spawn(WarmStartSpare).id();

    emit_start_ready(&mut app, webview);

    assert!(app.world().get::<WarmStartReady>(webview).is_some());
    let emitted = &app.world().resource::<EmittedIds>().0;
    assert_eq!(emitted, &[START_COMMAND_BAR_OPEN_EVENT]);
}

#[test]
fn inactive_cold_start_waits_for_activation_before_focusing() {
    let mut app = start_ready_app();
    let webview = app.world_mut().spawn_empty().id();

    emit_start_ready(&mut app, webview);

    let emitted = &app.world().resource::<EmittedIds>().0;
    assert_eq!(emitted, &[START_COMMAND_BAR_OPEN_EVENT]);
}

#[test]
fn start_sync_focuses_only_active_pages_on_first_sync_or_activation() {
    assert!(!should_focus_start_sync(false, false, false, false));
    assert!(should_focus_start_sync(false, true, false, false));
    assert!(should_focus_start_sync(true, true, true, false));
    assert!(should_focus_start_sync(true, true, false, true));
    assert!(!should_focus_start_sync(true, true, false, false));
}

#[test]
fn start_sync_refreshes_when_agent_recency_changes() {
    assert!(should_refresh_start_payload(
        false, true, false, false, false
    ));
    assert!(!should_refresh_start_payload(
        false, false, false, false, false
    ));
}

fn start_task(stack: Entity) -> PageOpenTask {
    PageOpenTask {
        id: PageOpenId::new(),
        stack,
        url: START_PAGE_URL.to_string(),
        request_id: None,
    }
}

#[test]
fn warm_claim_reuses_spare() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<CefPageAttachRequest>()
        .add_message::<StartSpareRevealed>()
        .add_systems(Update, handle_start_page_open);
    let stack = app.world_mut().spawn_empty().id();
    let spare = app.world_mut().spawn((WarmStartSpare, WarmStartReady)).id();
    let task = app.world_mut().spawn(start_task(stack)).id();
    app.update();

    assert_eq!(
        app.world().get::<ChildOf>(spare).map(|c| c.parent()),
        Some(stack),
        "spare reparented into the target stack"
    );
    assert!(
        app.world().get::<WarmStartSpare>(spare).is_none(),
        "spare marker removed on claim"
    );
    let meta = app
        .world()
        .get::<PageMetadata>(stack)
        .expect("stack received start metadata");
    assert_eq!(meta.url, START_PAGE_URL);
    assert!(app.world().get::<PageOpenHandled>(task).is_some());

    let attaches = app
        .world_mut()
        .resource_mut::<Messages<CefPageAttachRequest>>()
        .drain()
        .count();
    assert_eq!(attaches, 0, "warm claim must not spawn a cold webview");
    let reveals: Vec<StartSpareRevealed> = app
        .world_mut()
        .resource_mut::<Messages<StartSpareRevealed>>()
        .drain()
        .collect();
    assert_eq!(reveals.len(), 1);
    assert_eq!(reveals[0].webview, spare);
}

#[test]
fn not_ready_spare_is_not_claimed() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<CefPageAttachRequest>()
        .add_message::<StartSpareRevealed>()
        .add_systems(Update, handle_start_page_open);
    let stack = app.world_mut().spawn_empty().id();
    let spare = app.world_mut().spawn(WarmStartSpare).id();
    let task = app.world_mut().spawn(start_task(stack)).id();
    app.update();

    assert!(
        app.world().get::<ChildOf>(spare).is_none(),
        "an unready spare must not be reparented"
    );
    assert!(
        app.world().get::<WarmStartSpare>(spare).is_some(),
        "an unready spare stays in the pool"
    );
    assert!(app.world().get::<PageOpenHandled>(task).is_some());
    let attaches = app
        .world_mut()
        .resource_mut::<Messages<CefPageAttachRequest>>()
        .drain()
        .count();
    assert_eq!(attaches, 1, "unready spare falls back to the cold path");
}

#[test]
fn cold_fallback_when_pool_empty() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<CefPageAttachRequest>()
        .add_message::<StartSpareRevealed>()
        .add_systems(Update, handle_start_page_open);
    let stack = app.world_mut().spawn_empty().id();
    let task = app.world_mut().spawn(start_task(stack)).id();
    app.update();

    assert!(app.world().get::<PageOpenHandled>(task).is_some());
    let attaches: Vec<CefPageAttachRequest> = app
        .world_mut()
        .resource_mut::<Messages<CefPageAttachRequest>>()
        .drain()
        .collect();
    assert_eq!(attaches.len(), 1);
    assert_eq!(attaches[0].url, START_PAGE_URL);
    let reveals = app
        .world_mut()
        .resource_mut::<Messages<StartSpareRevealed>>()
        .drain()
        .count();
    assert_eq!(reveals, 0, "cold fallback emits no reveal");
}

#[test]
fn start_pool_fills_one_ready_slot() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, maintain_warm_start_pool);
    app.world_mut().spawn(VmuxWindow);
    app.update();
    app.update();

    let count = app
        .world_mut()
        .query_filtered::<(), With<WarmStartSpare>>()
        .iter(app.world())
        .count();
    assert_eq!(count, WARM_START_POOL_SIZE);
}
