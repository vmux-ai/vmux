use bevy::prelude::*;
use bevy_cef::prelude::{
    BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, CefKeyboardTarget,
    WebviewExtendStandardMaterial,
};
use vmux_command::event::{COMMAND_BAR_OPEN_EVENT, CommandBarOpenEvent};
use vmux_command::open_target::OpenTarget;
use vmux_command::snapshot::{
    CommandBarAgentsSnapshot, CommandBarPagesSnapshot, CommandBarSpacesSnapshot,
};
use vmux_core::{
    CefPageAttachRequest, PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask,
};

use crate::cef::Browser;
use crate::command_bar::handler::{
    TabGatherParams, build_command_bar_open_payload, gather_command_bar_tabs,
};
use crate::start::START_PAGE_URL;
use crate::start::event::{START_FOCUS_INPUT_EVENT, StartDataRequest, StartFocusInput};
use crate::window::VmuxWindow;

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

/// How many prewarmed `vmux://start/` webviews to keep ready. Each new tab / stack /
/// pane / space is an independent persistent start page, so a singleton (like the
/// command-bar modal) cannot serve them — a small pool is refilled after each claim.
const WARM_START_POOL_SIZE: usize = 1;

/// Marks a prewarmed, parked `vmux://start/` webview waiting to be claimed by the next
/// start open. Removed when the spare is reparented into a real stack.
#[derive(Component)]
struct WarmStartSpare;

/// Set on a warm spare once its page has actually mounted (it emitted [`StartDataRequest`]),
/// so a claim only reuses a spare that is genuinely warm — never one whose CEF browser or
/// WASM is still loading (which would defeat the near-instant path and fall to a cold paint).
#[derive(Component)]
struct WarmStartReady;

/// The hidden, zero-size holding node the warm spares are parked under so they keep their
/// CEF browser + WASM warm without compositing (a `Visibility::Hidden` ancestor makes them
/// non-renderable, so `sync_children_to_ui` collapses them and CEF hides the native view).
#[derive(Component)]
struct WarmStartPoolNode;

/// Host-internal signal that a warm spare was just revealed into a stack, so its launcher
/// data must be refreshed (it captured boot-time tabs/spaces) and its input refocused.
#[derive(Message)]
struct StartSpareRevealed {
    webview: Entity,
}

/// Bevy plugin for `vmux://start/`: spawns the page manifest, keeps a warm pool of
/// prewarmed launcher webviews, claims start page-open tasks (reusing a warm spare when
/// available), and answers [`StartDataRequest`] with the shared command-bar payload.
pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::start::PAGE_MANIFEST);
        app.add_plugins(BinEventEmitterPlugin::<(StartDataRequest,)>::for_hosts(&[
            "start",
        ]))
        .add_message::<StartSpareRevealed>()
        .add_observer(on_start_data_request)
        .add_systems(
            Update,
            (
                handle_start_page_open.in_set(PageOpenSet::HandleKnownPages),
                maintain_warm_start_pool,
                on_start_spare_revealed.after(PageOpenSet::HandleKnownPages),
            ),
        );
    }
}

/// Claim `vmux://start/` page-open tasks. When a warm spare is available it is reparented
/// into the target stack for a near-instant paint; otherwise it falls back to spawning a
/// cold launcher webview via [`CefPageAttachRequest`].
fn handle_start_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    spares: Query<Entity, (With<WarmStartSpare>, With<WarmStartReady>)>,
    children_q: Query<&Children>,
    mut attach: MessageWriter<CefPageAttachRequest>,
    mut revealed: MessageWriter<StartSpareRevealed>,
    mut commands: Commands,
) {
    let mut available: Vec<Entity> = spares.iter().collect();
    for (entity, task) in &tasks {
        if task.url != START_PAGE_URL {
            continue;
        }
        if let Some(spare) = available.pop() {
            clear_stack_children(task.stack, &children_q, &mut commands);
            commands.entity(task.stack).insert(PageMetadata {
                url: START_PAGE_URL.to_string(),
                title: "Start".to_string(),
                ..default()
            });
            commands
                .entity(spare)
                .insert((ChildOf(task.stack), CefKeyboardTarget))
                .remove::<(WarmStartSpare, WarmStartReady)>();
            revealed.write(StartSpareRevealed { webview: spare });
        } else {
            attach.write(CefPageAttachRequest {
                stack: task.stack,
                url: START_PAGE_URL.to_string(),
                title: "Start".to_string(),
                bg_color: None,
            });
        }
        commands.entity(entity).insert(PageOpenHandled);
    }
}

/// Keep the warm-start pool topped up to [`WARM_START_POOL_SIZE`]. Spares are parked under a
/// hidden holding node (created lazily once the window exists) so their CEF browser + WASM
/// load ahead of time without compositing.
fn maintain_warm_start_pool(
    pool_node: Query<Entity, With<WarmStartPoolNode>>,
    vmux_window: Query<Entity, With<VmuxWindow>>,
    spares: Query<(), With<WarmStartSpare>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let Ok(window) = vmux_window.single() else {
        return;
    };
    let node = match pool_node.single() {
        Ok(node) => node,
        Err(_) => commands
            .spawn((
                WarmStartPoolNode,
                Node {
                    width: Val::Px(0.0),
                    height: Val::Px(0.0),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Visibility::Hidden,
                ChildOf(window),
            ))
            .id(),
    };
    for _ in spares.iter().count()..WARM_START_POOL_SIZE {
        commands.spawn((
            Browser::new_with_title(&mut meshes, &mut webview_mt, START_PAGE_URL, "Start"),
            WarmStartSpare,
            ChildOf(node),
        ));
    }
}

/// Refresh a freshly-revealed warm spare: push current launcher data (the spare captured
/// boot-time state) and refocus its input, matching a cold open.
fn on_start_spare_revealed(
    mut revealed: MessageReader<StartSpareRevealed>,
    tab_gather: TabGatherParams,
    spaces_snapshot: Res<CommandBarSpacesSnapshot>,
    agents_snapshot: Res<CommandBarAgentsSnapshot>,
    pages_snapshot: Res<CommandBarPagesSnapshot>,
    mut commands: Commands,
) {
    for ev in revealed.read() {
        let payload = build_start_payload(
            &tab_gather,
            &spaces_snapshot,
            &agents_snapshot,
            &pages_snapshot,
        );
        commands.trigger(BinHostEmitEvent::from_rkyv(
            ev.webview,
            COMMAND_BAR_OPEN_EVENT,
            &payload,
        ));
        commands.trigger(BinHostEmitEvent::from_rkyv(
            ev.webview,
            START_FOCUS_INPUT_EVENT,
            &StartFocusInput,
        ));
    }
}

/// Answer the `vmux://start/` page's on-mount [`StartDataRequest`] with the shared
/// command-bar launcher payload (opening selections in place).
fn on_start_data_request(
    trigger: On<BinReceive<StartDataRequest>>,
    spares: Query<(), With<WarmStartSpare>>,
    tab_gather: TabGatherParams,
    spaces_snapshot: Res<CommandBarSpacesSnapshot>,
    agents_snapshot: Res<CommandBarAgentsSnapshot>,
    pages_snapshot: Res<CommandBarPagesSnapshot>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if spares.contains(webview) {
        commands.entity(webview).insert(WarmStartReady);
    }
    let payload = build_start_payload(
        &tab_gather,
        &spaces_snapshot,
        &agents_snapshot,
        &pages_snapshot,
    );
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        COMMAND_BAR_OPEN_EVENT,
        &payload,
    ));
}

/// Build the launcher payload shared by the on-mount data feed and warm-spare refresh.
fn build_start_payload(
    tab_gather: &TabGatherParams,
    spaces_snapshot: &CommandBarSpacesSnapshot,
    agents_snapshot: &CommandBarAgentsSnapshot,
    pages_snapshot: &CommandBarPagesSnapshot,
) -> CommandBarOpenEvent {
    let active_stack_count = tab_gather.stack_q.iter().count();
    let space_name = spaces_snapshot.active_space_name.clone();
    let tabs = gather_command_bar_tabs(
        tab_gather.active_tab.get(),
        &tab_gather.all_children,
        &tab_gather.leaf_panes,
        &tab_gather.pane_ts,
        &tab_gather.pane_children,
        &tab_gather.stack_ts,
        &tab_gather.stack_q,
        &tab_gather.browser_meta,
        &tab_gather.child_of_q,
    );
    build_command_bar_open_payload(
        0,
        false,
        space_name,
        String::new(),
        spaces_snapshot,
        agents_snapshot,
        pages_snapshot,
        active_stack_count,
        tabs,
        Some(OpenTarget::InPlace),
    )
}

/// Despawn a stack's existing webview children before attaching new content.
fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::PageOpenId;
    use vmux_core::page::PageManifest;

    #[test]
    fn start_plugin_spawns_manifest() {
        let mut app = App::new();
        app.add_plugins(StartPlugin);
        let mut q = app.world_mut().query::<&PageManifest>();
        assert!(q.iter(app.world()).any(|m| m.host == "start"));
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
    fn pool_fills_to_target() {
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
}
