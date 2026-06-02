#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::{
    ecs::{message::Messages, relationship::Relationship},
    input::{
        ButtonState, InputSystems,
        mouse::{MouseButton, MouseButtonInput},
    },
    picking::pointer::PointerButton,
    prelude::*,
    ui::{UiGlobalTransform, UiSystems},
    window::{CursorMoved, PrimaryWindow, WindowResized},
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::{CefEmbeddedHosts, RenderTextureMessage, webview_debug_log};
use vmux_command::{
    AppCommand, BrowserBarCommand, BrowserCommand, BrowserNavigationCommand, BrowserViewCommand,
    ReadAppCommands, open::OpenCommand,
};
use vmux_core::{
    CefPageAttachRequest, PageMetadata, PageOpenError, PageOpenHandled, PageOpenId,
    PageOpenRequest, PageOpenSet, PageOpenTarget, PageOpenTask,
    page::{PageManifest, PageReady},
};
use vmux_history::{CreatedAt, LastActivatedAt, Visit};
use vmux_layout::command_bar::handler::{CommandBarNativeSize, PendingCommandBarReveal};
use vmux_layout::event::SideSheetCommandEvent;
pub use vmux_layout::{Browser, Loading};
use vmux_layout::{
    Header, LayoutCef, NavigationState, Open, PendingWebviewReveal,
    event::{
        HEADER_HEIGHT_PX, HeaderCommandEvent, LAYOUT_STATE_EVENT, LayoutStateEvent,
        PANE_TREE_EVENT, PaneNode, PaneTreeEvent, RELOAD_EVENT, ReloadEvent, STACKS_EVENT,
        StackNode, StackRow, StacksHostEvent, TABS_EVENT, TabRow, TabsHostEvent,
    },
    pane::{Pane, PaneHoverIntent, PaneSplit, first_leaf_descendant, first_stack_in_pane},
    side_sheet::{SideSheet, SideSheetPosition, SideSheetWidth},
    stack::{
        CloseConfirmed, PendingStackClose, Stack, active_stack_in_pane, collect_leaf_panes,
        focused_stack, stack_bundle,
    },
    tab::Tab,
    window::{
        Modal, VmuxWindow, WEBVIEW_Z_HEADER, WEBVIEW_Z_MAIN, WEBVIEW_Z_MODAL, WEBVIEW_Z_SIDE_SHEET,
    },
};
use vmux_setting::AppSettings;
use vmux_terminal::{self as terminal, PtyExited, RestartPty, Terminal};
use vmux_ui::theme::{THEME_EVENT, ThemeEvent};

pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        let mut manifests = app.world_mut().query::<&PageManifest>();
        let embedded_hosts = CefEmbeddedHosts(
            manifests
                .iter(app.world())
                .map(PageManifest::embedded_host)
                .collect(),
        );
        webview_debug_log(format!("BrowserPlugin embedded_hosts={embedded_hosts:?}"));
        app.add_message::<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>()
            .add_message::<PageOpenRequest>()
            .add_message::<CefPageAttachRequest>()
            .configure_sets(Update, CefSystems::CreateAndResize.after(ReadAppCommands))
            .configure_sets(
                Update,
                (
                    PageOpenSet::ResolveTarget,
                    PageOpenSet::HandleKnownPages,
                    PageOpenSet::Fallback,
                    PageOpenSet::Respond,
                )
                    .chain()
                    .after(ReadAppCommands),
            )
            .add_plugins((
                CefPlugin {
                    root_cache_path: cef_root_cache_path(),
                    embedded_hosts,
                    ..default()
                },
                BinEventEmitterPlugin::<(HeaderCommandEvent, SideSheetCommandEvent)>::default(),
            ))
            .add_observer(on_webview_ready_send_theme)
            .add_observer(on_header_command_emit)
            .add_observer(on_side_sheet_command_emit)
            .add_observer(on_reload_notify_header)
            .add_observer(on_hard_reload_notify_header)
            .add_systems(
                Update,
                sync_cef_backend_for_interaction_mode
                    .after(PageOpenSet::Fallback)
                    .after(spawn_popup_stacks)
                    .before(CefSystems::CreateAndResize),
            )
            .add_systems(
                PreUpdate,
                (
                    sync_layout_cef_pointer_target,
                    forward_layout_cef_cursor_move.run_if(on_message::<CursorMoved>),
                    forward_layout_cef_mouse_button.run_if(on_message::<MouseButtonInput>),
                )
                    .chain()
                    .after(InputSystems),
            )
            .add_systems(
                Update,
                (
                    handle_browser_commands.in_set(ReadAppCommands),
                    vmux_layout::apply_cef_state_from_webview,
                    drain_loading_state,
                    drain_committed_navigation,
                    spawn_popup_stacks,
                    handle_page_open_requests.in_set(PageOpenSet::ResolveTarget),
                    attach_cef_page_requests.in_set(PageOpenSet::Fallback),
                    handle_unclaimed_page_open_tasks.in_set(PageOpenSet::Fallback),
                    respond_page_open_tasks.in_set(PageOpenSet::Respond),
                    handle_browser_navigate_requests.after(vmux_terminal::ServiceMessageSet),
                    handle_browser_go_back_requests,
                    handle_browser_go_forward_requests,
                    handle_open_in_new_stack_requests,
                    handle_browser_open_history.in_set(ReadAppCommands),
                ),
            )
            .add_systems(
                Update,
                (sync_page_metadata_to_tab, spawn_visit_on_navigation)
                    .chain()
                    .after(vmux_layout::apply_cef_state_from_webview),
            )
            .add_systems(
                Update,
                vmux_layout::mirror_metadata_to_url
                    .after(vmux_layout::apply_cef_state_from_webview),
            )
            .add_systems(
                Update,
                (
                    push_layout_state_emit,
                    push_stacks_host_emit,
                    push_pane_tree_emit,
                    push_tabs_host_emit,
                )
                    .after(vmux_layout::apply_cef_state_from_webview)
                    .after(vmux_layout::stack::ComputeFocusSet),
            )
            .add_systems(
                PostUpdate,
                (
                    sync_keyboard_target,
                    sync_children_to_ui,
                    sync_windowed_frames,
                    sync_windowed_chrome,
                    sync_windowed_command_bar,
                    apply_repaint_nudge,
                    sync_cef_webview_resize_after_ui,
                    sync_webview_pane_corner_clip,
                    sync_osr_webview_focus,
                    flush_pending_osr_textures,
                )
                    .chain()
                    .after(UiSystems::Layout)
                    .before(render_standard_materials),
            );
    }
}

fn on_webview_ready_send_theme(
    trigger: On<Add, PageReady>,
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    cef_q: Query<(), With<LayoutCef>>,
    modal_q: Query<(), With<Modal>>,
    mut zoom_q: Query<&mut bevy_cef::prelude::ZoomLevel>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    webview_debug_log(format!("on_webview_ready_send_theme entity={entity:?}"));
    if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
        let payload = ThemeEvent {
            radius: settings.layout.radius,
        };
        commands.trigger(BinHostEmitEvent::from_rkyv(entity, THEME_EVENT, &payload));
    }
    // CEF / modal must never carry a stale zoom (e.g. from a previous
    // session where pinch-zoom was allowed); force them to 0 once the
    // webview is ready, both on the component and on the CEF host.
    if cef_q.get(entity).is_ok() || modal_q.get(entity).is_ok() {
        if let Ok(mut zoom) = zoom_q.get_mut(entity) {
            zoom.0 = 0.0;
        }
        browsers.set_zoom_level(&entity, 0.0);
    }
}

type CefPointerRegionQuery<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Header>,
        Option<&'static SideSheet>,
        &'static Node,
        &'static ComputedNode,
        &'static UiGlobalTransform,
        Option<&'static Visibility>,
        Has<Open>,
    ),
    Or<(With<Header>, With<SideSheet>)>,
>;

#[derive(Clone, Copy)]
struct CefPointerHitRect {
    center: Vec2,
    size: Vec2,
    interactive: bool,
}

fn cef_pointer_hit_rect_contains(rect: CefPointerHitRect, point: Vec2) -> bool {
    if !rect.interactive {
        return false;
    }
    let half = rect.size * 0.5;
    let min = rect.center - half;
    let max = rect.center + half;
    point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
}

fn cef_pointer_hit_rect(
    header: Option<&Header>,
    side_sheet: Option<&SideSheet>,
    node: &Node,
    computed: &ComputedNode,
    transform: &UiGlobalTransform,
    visibility: Option<&Visibility>,
    open: bool,
) -> CefPointerHitRect {
    let interactive = (header.is_some() || side_sheet.is_some())
        && open
        && node.display != Display::None
        && !matches!(visibility, Some(Visibility::Hidden))
        && computed.size.x > 0.0
        && computed.size.y > 0.0;
    CefPointerHitRect {
        center: transform.transform_point2(Vec2::ZERO),
        size: computed.size,
        interactive,
    }
}

fn cef_pointer_regions_contains(
    cursor_pos: Vec2,
    cef_regions: &CefPointerRegionQuery<'_, '_>,
) -> bool {
    cef_regions
        .iter()
        .map(
            |(header, side_sheet, node, computed, transform, visibility, open)| {
                cef_pointer_hit_rect(
                    header, side_sheet, node, computed, transform, visibility, open,
                )
            },
        )
        .any(|rect| cef_pointer_hit_rect_contains(rect, cursor_pos))
}

fn sync_layout_cef_pointer_target(
    windows: Query<&Window, With<PrimaryWindow>>,
    layout_q: Query<(Entity, Has<CefPointerTarget>), With<LayoutCef>>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    modal_pointer_targets: Query<(), (With<Modal>, With<CefPointerTarget>)>,
    mut commands: Commands,
) {
    let Ok((layout, has_target)) = layout_q.single() else {
        return;
    };
    let should_target = modal_pointer_targets.is_empty()
        && windows
            .single()
            .ok()
            .and_then(Window::cursor_position)
            .is_some_and(|pos| cef_pointer_regions_contains(pos, &cef_regions));
    if should_target && !has_target {
        commands.entity(layout).insert(CefPointerTarget);
    } else if !should_target && has_target {
        commands.entity(layout).remove::<CefPointerTarget>();
    }
}

fn forward_layout_cef_cursor_move(
    mut events: MessageReader<CursorMoved>,
    buttons: Res<ButtonInput<MouseButton>>,
    suppress: Res<CefSuppressPointerInput>,
    browsers: NonSend<Browsers>,
    layout_q: Query<Entity, With<LayoutCef>>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    modal_pointer_targets: Query<(), (With<Modal>, With<CefPointerTarget>)>,
) {
    if suppress.0 || !modal_pointer_targets.is_empty() {
        for _ in events.read() {}
        return;
    }
    let Ok(layout) = layout_q.single() else {
        for _ in events.read() {}
        return;
    };
    for event in events.read() {
        if cef_pointer_regions_contains(event.position, &cef_regions) {
            browsers.send_mouse_move(&layout, buttons.get_pressed(), event.position, false);
        }
    }
}

fn forward_layout_cef_mouse_button(
    mut events: MessageReader<MouseButtonInput>,
    windows: Query<&Window>,
    suppress: Res<CefSuppressPointerInput>,
    browsers: NonSend<Browsers>,
    layout_q: Query<Entity, With<LayoutCef>>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    modal_pointer_targets: Query<(), (With<Modal>, With<CefPointerTarget>)>,
    mut captured: Local<bool>,
) {
    if suppress.0 || !modal_pointer_targets.is_empty() {
        for _ in events.read() {}
        *captured = false;
        return;
    }
    let Ok(layout) = layout_q.single() else {
        for _ in events.read() {}
        *captured = false;
        return;
    };
    for event in events.read() {
        let Some(button) = pointer_button_from_mouse_button(event.button) else {
            continue;
        };
        let Ok(window) = windows.get(event.window) else {
            continue;
        };
        let Some(position) = window.cursor_position() else {
            continue;
        };
        let inside = cef_pointer_regions_contains(position, &cef_regions);
        if event.state == ButtonState::Pressed && inside {
            *captured = true;
        }
        if inside || *captured {
            browsers.send_mouse_click(
                &layout,
                position,
                button,
                event.state == ButtonState::Released,
            );
        }
        if event.state == ButtonState::Released {
            *captured = false;
        }
    }
}

fn pointer_button_from_mouse_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

fn sync_keyboard_target(
    mode: Res<vmux_layout::scene::InteractionMode>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    child_of_q: Query<&ChildOf>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    modal_q: Query<(&Node, Has<CefKeyboardTarget>), With<Modal>>,
    content_q: Query<(Entity, Has<CefKeyboardTarget>), With<Browser>>,
    terminal_q: Query<(), With<vmux_terminal::Terminal>>,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
    mut commands: Commands,
) {
    if vmux_layout::command_bar::handler::is_command_bar_open(&modal_q) {
        return;
    }

    // In Player mode, only sync when a pane has been clicked (Focused sub-state).
    // In Roaming (no CefKeyboardTarget on any pane browser), skip sync to prevent
    // re-assigning the target to the previously active pane.
    if *mode == vmux_layout::scene::InteractionMode::Player {
        let has_pane_target = content_q
            .iter()
            .any(|(e, has_kb)| has_kb && !status_q.contains(e) && !side_sheet_q.contains(e));
        if !has_pane_target {
            return;
        }
    }
    let active_stack_opt = focus.stack;
    let Some(active_stack_entity) = active_stack_opt else {
        return;
    };
    for (browser_e, has_kb) in &content_q {
        if status_q.contains(browser_e) || side_sheet_q.contains(browser_e) {
            continue;
        }

        let in_active = child_of_q
            .get(browser_e)
            .ok()
            .map(|co| co.get() == active_stack_entity)
            .unwrap_or(false);

        if in_active {
            if !has_kb {
                commands.entity(browser_e).insert(CefKeyboardTarget);
            }
            // Suppress CEF keyboard forwarding when a terminal is focused —
            // terminals receive input via the service, not CEF key events.
            suppress.0 = terminal_q.contains(browser_e);
        } else if has_kb {
            commands.entity(browser_e).remove::<CefKeyboardTarget>();
        }
    }
}

fn tab_ancestor(
    start: Entity,
    child_of_q: &Query<&ChildOf>,
    tabs_q: &Query<(Entity, &LastActivatedAt), With<Tab>>,
) -> Option<Entity> {
    let mut e = start;
    loop {
        if tabs_q.contains(e) {
            return Some(e);
        }
        match child_of_q.get(e) {
            Ok(co) => e = co.get(),
            Err(_) => return None,
        }
    }
}

fn sync_children_to_ui(
    mut browser_q: Query<
        (
            &mut Transform,
            &ComputedNode,
            &bevy::ui::ComputedStackIndex,
            &UiGlobalTransform,
            &ChildOf,
            &mut WebviewSize,
            Option<&Header>,
            Option<&SideSheet>,
            Option<&Modal>,
            Option<&Visibility>,
            Option<&HistorySwipeVisualOffset>,
            Has<PendingWebviewReveal>,
            Has<PendingCommandBarReveal>,
        ),
        With<Browser>,
    >,
    child_of_q: Query<&ChildOf>,
    pane_rect: Query<(&ComputedNode, &UiGlobalTransform), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    tabs_q: Query<(Entity, &LastActivatedAt), With<Tab>>,
    new_stack_ctx: Res<vmux_layout::NewStackContext>,
    glass: Single<(Entity, &ComputedNode, &UiGlobalTransform), With<VmuxWindow>>,
) {
    let &(glass_entity, glass_node, glass_ui_gt) = &*glass;
    let pad = glass_node.padding;
    let glass_size_px = glass_node.size + pad.min_inset + pad.max_inset;

    let active_tab = tabs_q.iter().max_by_key(|(_, ts)| ts.0).map(|(e, _)| e);

    for (
        mut tf,
        self_computed,
        self_stack_index,
        self_ui_gt,
        child_of,
        mut webview_size,
        status,
        side_sheet,
        modal,
        visibility,
        history_swipe_visual,
        pending_webview_reveal,
        pending_command_bar_reveal,
    ) in browser_q.iter_mut()
    {
        let parent = child_of.get();
        let pane_entity = child_of_q.get(parent).map(|co| co.get()).unwrap_or(parent);
        let (computed, ui_gt) = match pane_rect.get(pane_entity) {
            Ok((cn, gt)) => (cn, gt),
            Err(_) => (self_computed, self_ui_gt),
        };

        if glass_size_px.x <= 0.0 || glass_size_px.y <= 0.0 {
            continue;
        }

        let size_px = computed.size;
        if !webview_layout_is_renderable(
            size_px,
            visibility,
            pending_webview_reveal || pending_command_bar_reveal,
        ) {
            tf.scale = Vec3::splat(1.0e-6);
            if webview_size.0 != Vec2::ONE {
                webview_size.0 = Vec2::ONE;
            }
            continue;
        }

        let is_cef_ui = status.is_some() || side_sheet.is_some() || modal.is_some();

        // Check if this browser's parent tab is the active tab in its pane
        let is_active_stack = if parent != glass_entity && !is_cef_ui {
            active_stack_in_pane(pane_entity, &pane_children, &tab_ts) == Some(parent)
        } else {
            true
        };

        // Keep rendering the previous tab behind while a new empty tab
        // (without CEF content) is pending in the command bar flow.
        let is_previous_stack =
            new_stack_ctx.stack.is_some() && new_stack_ctx.previous_stack == Some(parent);

        let is_inactive_stack =
            parent != glass_entity && !is_cef_ui && !is_active_stack && !is_previous_stack;

        let is_inactive_tab = parent != glass_entity
            && !is_cef_ui
            && match tab_ancestor(parent, &child_of_q, &tabs_q) {
                Some(tab) => Some(tab) != active_tab,
                None => false,
            };

        let sx = size_px.x / glass_size_px.x;
        let sy = size_px.y / glass_size_px.y;
        let new_scale = if is_inactive_stack || is_inactive_tab {
            Vec3::splat(1e-6)
        } else {
            Vec3::new(sx, sy, 1.0)
        };
        tf.scale = new_scale;

        let center_ui = ui_gt.transform_point2(Vec2::ZERO);
        let glass_center_ui = glass_ui_gt.transform_point2(Vec2::ZERO);
        let delta_px = center_ui - glass_center_ui;

        let tx = delta_px.x / glass_size_px.x;
        let ty = -delta_px.y / glass_size_px.y;
        let z = if modal.is_some() {
            WEBVIEW_Z_MODAL
        } else if status.is_some() {
            WEBVIEW_Z_HEADER
        } else if side_sheet.is_some() {
            WEBVIEW_Z_SIDE_SHEET
        } else if parent != glass_entity {
            if is_active_stack {
                WEBVIEW_Z_MAIN
            } else {
                WEBVIEW_Z_MAIN - 0.01
            }
        } else {
            0.01 + self_stack_index.0 as f32 * 0.001
        };
        let history_swipe_tx = if parent != glass_entity && !is_cef_ui {
            history_swipe_visual
                .map(|visual| visual.offset_px / glass_size_px.x)
                .unwrap_or(0.0)
        } else {
            0.0
        };
        tf.translation = Vec3::new(tx + history_swipe_tx, ty, z);

        let dip = (size_px * computed.inverse_scale_factor).max(Vec2::splat(1.0));
        if webview_size.0 != dip {
            webview_size.0 = dip;
        }
    }
}

fn webview_should_use_windowed(mode: vmux_layout::scene::InteractionMode) -> bool {
    cfg!(target_os = "macos") && mode == vmux_layout::scene::InteractionMode::User
}

fn sync_cef_backend_for_interaction_mode(world: &mut World) {
    let mode = world
        .get_resource::<vmux_layout::scene::InteractionMode>()
        .copied()
        .unwrap_or_default();
    let should_windowed = webview_should_use_windowed(mode);
    let mut query = world.query_filtered::<Entity, (With<Browser>, With<WebviewSource>)>();
    let entities: Vec<Entity> = query.iter(world).collect();
    let mut recreate = Vec::new();
    {
        let browsers = world.non_send::<Browsers>();
        for &entity in &entities {
            if browsers
                .is_windowed(&entity)
                .is_some_and(|actual| actual != should_windowed)
            {
                recreate.push(entity);
            }
        }
    }
    if !recreate.is_empty() {
        let mut browsers = world.non_send_mut::<Browsers>();
        for entity in &recreate {
            browsers.close(entity);
        }
    }
    for entity in entities {
        let marker_matches = world.get::<WebviewWindowed>(entity).is_some() == should_windowed;
        let needs_recreate = recreate.contains(&entity);
        if marker_matches && !needs_recreate {
            continue;
        }
        let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
            continue;
        };
        if should_windowed {
            entity_mut.insert(WebviewWindowed);
        } else {
            entity_mut.remove::<WebviewWindowed>();
        }
        if needs_recreate {
            entity_mut
                .remove::<PageReady>()
                .remove::<PendingWebviewReveal>()
                .remove::<PendingCommandBarReveal>();
        }
    }
}

/// Position windowed (native) content webviews to match their pane rect. Reads the mesh scale set
/// by `sync_children_to_ui` (visible active pane has a real scale; inactive panes ~1e-6) to pick
/// which native view to show. No-op for OSR webviews / non-macOS (`set_windowed_*` are no-ops).
fn sync_windowed_frames(
    browsers: NonSend<Browsers>,
    browser_q: Query<
        (
            Entity,
            &Transform,
            &ComputedNode,
            &UiGlobalTransform,
            &ChildOf,
        ),
        (
            With<Browser>,
            With<WebviewWindowed>,
            Without<LayoutCef>,
            Without<Modal>,
        ),
    >,
    child_of_q: Query<&ChildOf>,
    pane_rect: Query<(&ComputedNode, &UiGlobalTransform), With<Pane>>,
) {
    for (entity, tf, self_computed, self_ui_gt, child_of) in &browser_q {
        if tf.scale.x <= 1.0e-3 {
            browsers.set_windowed_hidden(&entity, true);
            continue;
        }
        let parent = child_of.get();
        let pane_entity = child_of_q.get(parent).map(|co| co.get()).unwrap_or(parent);
        let (computed, ui_gt) = pane_rect
            .get(pane_entity)
            .unwrap_or((self_computed, self_ui_gt));
        let size_px = computed.size;
        let center = ui_gt.transform_point2(Vec2::ZERO);
        let left = center.x - size_px.x * 0.5;
        let top = center.y - size_px.y * 0.5;
        let scale = 1.0 / computed.inverse_scale_factor.max(1.0e-6);
        browsers.set_windowed_hidden(&entity, false);
        browsers.set_windowed_frame(&entity, left, top, size_px.x, size_px.y, scale);
        browsers.raise_windowed_to_front(&entity);
    }
}

/// Position the native layout as a full-window view behind the native page(s). The layout is created
/// before any page, so it stays the backmost sibling — no reorder needed. Its opaque header/sidebar
/// show wherever the (inset) page doesn't cover; the page covers the content region on top.
fn sync_windowed_chrome(
    browsers: NonSend<Browsers>,
    chrome_q: Query<(Entity, Option<&HostWindow>), (With<LayoutCef>, With<WebviewWindowed>)>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    for (entity, host_window) in &chrome_q {
        let window_entity = host_window
            .map(|h| h.0)
            .or_else(|| primary_window.single().ok());
        let Some(window_entity) = window_entity else {
            continue;
        };
        let Ok(window) = windows.get(window_entity) else {
            continue;
        };
        let scale = window.resolution.scale_factor();
        let w = window.resolution.physical_width() as f32;
        let h = window.resolution.physical_height() as f32;
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        browsers.set_windowed_hidden(&entity, false);
        browsers.set_windowed_frame(&entity, 0.0, 0.0, w, h, scale);
        browsers.lower_windowed_to_back(&entity);
    }
}

#[derive(Clone, Copy, Debug)]
struct CommandBarWindowedFrame {
    left_px: f32,
    top_px: f32,
    width_px: f32,
    height_px: f32,
}

fn command_bar_windowed_frame(
    window_width_px: f32,
    window_height_px: f32,
    scale: f32,
    measured_size: Option<Vec2>,
) -> Option<CommandBarWindowedFrame> {
    if !window_width_px.is_finite()
        || !window_height_px.is_finite()
        || !scale.is_finite()
        || window_width_px <= 0.0
        || window_height_px <= 0.0
        || scale <= 0.0
    {
        return None;
    }

    const MARGIN: f32 = 16.0;
    const MAX_W: f32 = 576.0;
    const MIN_W: f32 = 240.0;
    const MIN_H: f32 = 56.0;
    const FALLBACK_H: f32 = 360.0;

    let win_w = window_width_px / scale;
    let win_h = window_height_px / scale;
    let top = win_h * 0.15;
    let available_w = (win_w - MARGIN * 2.0).max(1.0);
    let min_w = MIN_W.min(available_w);
    let box_w = available_w.min(MAX_W).max(min_w);
    let available_h = (win_h - top - MARGIN).max(1.0);
    let min_h = MIN_H.min(available_h);
    let measured_h = measured_size
        .map(|size| size.y)
        .filter(|height| height.is_finite() && *height > 0.0)
        .unwrap_or(FALLBACK_H);
    let box_h = measured_h.min(available_h).max(min_h);
    let box_x = ((win_w - box_w) * 0.5).max(0.0);

    Some(CommandBarWindowedFrame {
        left_px: box_x * scale,
        top_px: top * scale,
        width_px: box_w * scale,
        height_px: box_h * scale,
    })
}

fn command_bar_hidden_windowed_frame() -> CommandBarWindowedFrame {
    CommandBarWindowedFrame {
        left_px: 0.0,
        top_px: 0.0,
        width_px: 1.0,
        height_px: 1.0,
    }
}

fn hide_windowed_command_bar(browsers: &Browsers, entity: Entity) {
    let frame = command_bar_hidden_windowed_frame();
    browsers.set_windowed_hidden(&entity, true);
    browsers.resize(&entity, Vec2::new(frame.width_px, frame.height_px), 1.0);
    browsers.set_windowed_frame(
        &entity,
        frame.left_px,
        frame.top_px,
        frame.width_px,
        frame.height_px,
        1.0,
    );
}

fn command_bar_windowed_view_should_show(
    display: Display,
    visibility: Visibility,
    has_keyboard_target: bool,
) -> bool {
    display != Display::None && visibility != Visibility::Hidden && has_keyboard_target
}

fn sync_windowed_command_bar(
    browsers: NonSend<Browsers>,
    modal_q: Query<
        (
            Entity,
            &Node,
            &Visibility,
            Has<CefKeyboardTarget>,
            Option<&HostWindow>,
            Option<&CommandBarNativeSize>,
        ),
        (With<Modal>, With<WebviewWindowed>),
    >,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    mut was_open: Local<bool>,
) {
    let Ok((entity, node, visibility, has_keyboard_target, host_window, native_size)) =
        modal_q.single()
    else {
        *was_open = false;
        return;
    };
    let open =
        command_bar_windowed_view_should_show(node.display, *visibility, has_keyboard_target);
    if !open {
        hide_windowed_command_bar(&browsers, entity);
        *was_open = false;
        return;
    }
    if !browsers.has_browser(entity) {
        return;
    }
    let window_entity = host_window
        .map(|h| h.0)
        .or_else(|| primary_window.single().ok());
    let Some(window_entity) = window_entity else {
        hide_windowed_command_bar(&browsers, entity);
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        hide_windowed_command_bar(&browsers, entity);
        return;
    };
    let scale = window.resolution.scale_factor();
    let measured = native_size.map(|size| Vec2::new(size.width, size.height));
    let Some(frame) = command_bar_windowed_frame(
        window.resolution.physical_width() as f32,
        window.resolution.physical_height() as f32,
        scale,
        measured,
    ) else {
        hide_windowed_command_bar(&browsers, entity);
        return;
    };

    browsers.set_windowed_frame(
        &entity,
        frame.left_px,
        frame.top_px,
        frame.width_px,
        frame.height_px,
        scale,
    );
    browsers.resize(
        &entity,
        Vec2::new(frame.width_px / scale, frame.height_px / scale),
        scale,
    );
    browsers.set_windowed_hidden(&entity, false);
    if !*was_open {
        browsers.raise_windowed_to_front(&entity);
        browsers.nudge_windowed_repaint(&entity);
        *was_open = true;
    }
}

fn apply_repaint_nudge(browsers: NonSend<Browsers>, ready: Query<Entity, Added<PageReady>>) {
    for entity in &ready {
        let nudged = browsers.nudge_windowed_repaint(&entity);
        info!("[repaint-nudge] e={entity:?} nudged={nudged}");
    }
}

fn sync_cef_webview_resize_after_ui(
    browsers: NonSend<Browsers>,
    webviews: Query<(Entity, &WebviewSize), (With<Browser>, Without<Modal>)>,
    host_window: Query<&HostWindow>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    mut last_entries: Local<Vec<(u64, Vec2, f32)>>,
    mut window_resized: MessageReader<WindowResized>,
) {
    // Force-resize all CEF browsers (tabs, terminals, side sheets, modals) on
    // window resize so backgrounded surfaces also repaint at the new size
    // instead of showing a stale frame until they become active.
    let force = window_resized.read().count() > 0;
    if force {
        last_entries.clear();
    }
    for (entity, size) in webviews.iter() {
        if !browsers.has_browser(entity) {
            continue;
        }
        let key = entity.to_bits();
        let window_entity = host_window
            .get(entity)
            .ok()
            .map(|h| h.0)
            .or_else(|| primary_window.single().ok());
        let device_scale_factor = window_entity
            .and_then(|e| windows.get(e).ok())
            .map(|w| w.resolution.scale_factor())
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(1.0);
        if last_entries
            .iter()
            .any(|(k, s, sf)| *k == key && *s == size.0 && (*sf - device_scale_factor).abs() < 0.01)
        {
            continue;
        }
        browsers.resize(&entity, size.0, device_scale_factor);
        webview_debug_log(format!(
            "resize entity={entity:?} size={:?} scale={device_scale_factor} force={force}",
            size.0
        ));
        if let Some(entry) = last_entries.iter_mut().find(|(k, _, _)| *k == key) {
            entry.1 = size.0;
            entry.2 = device_scale_factor;
        } else {
            last_entries.push((key, size.0, device_scale_factor));
        }
    }
}

/// Walks up from a browser entity to find its enclosing Tab, then counts
/// leaf panes under that tab. Returns None if the parent chain doesn't
/// reach a Tab.
fn pane_count_for_browser(
    browser_e: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<(), With<Tab>>,
    _pane_q: &Query<(), With<Pane>>,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Option<usize> {
    let mut cur = browser_e;
    let tab = loop {
        let parent = child_of_q.get(cur).ok()?.get();
        if tab_q.get(parent).is_ok() {
            break parent;
        }
        cur = parent;
    };
    let mut leaves = Vec::new();
    collect_leaf_panes(tab, all_children, leaf_panes, &mut leaves);
    Some(leaves.len())
}

fn sync_webview_pane_corner_clip(
    settings: Res<AppSettings>,
    layout_hidden: Res<vmux_layout::toggle::LayoutHidden>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    tabs: Query<
        (
            Entity,
            &WebviewSize,
            &MeshMaterial3d<WebviewExtendStandardMaterial>,
        ),
        With<Browser>,
    >,
    status: Query<(&WebviewSize, &MeshMaterial3d<WebviewExtendStandardMaterial>), With<Header>>,
    side_sheet: Query<
        (&WebviewSize, &MeshMaterial3d<WebviewExtendStandardMaterial>),
        With<SideSheet>,
    >,
    child_of_q: Query<&ChildOf>,
    tab_q: Query<(), With<Tab>>,
    pane_q: Query<(), With<Pane>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) {
    let r = settings.layout.radius;
    for (browser_e, size, mat_h) in &tabs {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        // corner_mode = 1.0 → round bottom corners only, so the pane top
        // sits flush against the url row above it. Switch to 0.0 (all
        // corners) when the active tab is split (each pane floats as a
        // card) or when the CEF shell is hidden (no url row above to merge
        // with).
        let pane_count = pane_count_for_browser(
            browser_e,
            &child_of_q,
            &tab_q,
            &pane_q,
            &all_children,
            &leaf_panes,
        )
        .unwrap_or(1);
        let mode = if layout_hidden.0 || pane_count > 1 {
            0.0
        } else {
            1.0
        };
        if let Some(mut mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, mode);
        }
    }
    for (size, mat_h) in &status {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mut mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 0.0);
        }
    }
    for (size, mat_h) in &side_sheet {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mut mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 0.0);
        }
    }
}

fn sync_osr_webview_focus(
    browsers: NonSend<Browsers>,
    webviews: Query<
        (
            Entity,
            Option<&Visibility>,
            Option<&ComputedNode>,
            Has<PendingWebviewReveal>,
            Has<PendingCommandBarReveal>,
            Has<Modal>,
            Has<CefKeyboardTarget>,
        ),
        With<WebviewSource>,
    >,
    primary_window: Single<&Window, With<PrimaryWindow>>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    new_stack_ctx: Res<vmux_layout::NewStackContext>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children_q: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    child_of_q: Query<&ChildOf>,

    mut ready: Local<Vec<Entity>>,
    mut auxiliary: Local<Vec<Entity>>,
    mut last_active: Local<Option<Entity>>,
    mut last_ready_set: Local<Vec<Entity>>,
) {
    ready.clear();
    let mut modal_keyboard_target = None;
    for (
        entity,
        visibility,
        computed,
        pending_reveal,
        pending_command_bar_reveal,
        is_modal,
        has_keyboard_target,
    ) in webviews.iter()
    {
        if !browsers.has_browser(entity) {
            continue;
        }
        let size = computed.map(|node| node.size).unwrap_or(Vec2::ONE);
        if webview_osr_should_run(
            size,
            visibility,
            pending_reveal || pending_command_bar_reveal,
        ) {
            ready.push(entity);
            if is_modal && has_keyboard_target {
                modal_keyboard_target = Some(entity);
            }
        } else {
            browsers.set_osr_hidden(&entity);
        }
    }
    if ready.is_empty() {
        return;
    }
    ready.sort_by_key(|e| e.to_bits());
    let window_visible = primary_window.visible;
    let window_focused = primary_window.focused;

    let active_stack_opt = focus.stack;
    let active_stack = active_stack_opt.and_then(|tab| {
        ready
            .iter()
            .copied()
            .find(|&b| child_of_q.get(b).ok().map(|co| co.get()) == Some(tab))
    });
    let active = choose_osr_active_webview(modal_keyboard_target, active_stack, ready[0]);

    if !window_visible {
        if last_active.is_some() || *last_ready_set != *ready {
            webview_debug_log(format!("osr focus window_hidden ready={ready:?}"));
            browsers.sync_osr_focus_to_active_pane(None, &[]);
            *last_active = None;
            last_ready_set.clone_from(&ready);
        }
    } else if !window_focused {
        if last_active.is_some() || *last_ready_set != *ready {
            webview_debug_log(format!("osr focus window_unfocused ready={ready:?}"));
            browsers.sync_osr_focus_to_active_pane(None, &[]);
            *last_active = None;
            last_ready_set.clone_from(&ready);
        }
    } else if *last_active == Some(active) && *last_ready_set == *ready {
    } else {
        auxiliary.clear();
        auxiliary.extend(ready.iter().copied().filter(|&e| e != active));
        webview_debug_log(format!(
            "osr focus active={active:?} auxiliary={:?} ready={ready:?}",
            auxiliary.as_slice()
        ));
        browsers.sync_osr_focus_to_active_pane(Some(active), auxiliary.as_slice());
        *last_active = Some(active);
        last_ready_set.clone_from(&ready);
    }
    for &e in ready.iter() {
        let mut parent_is_stack = false;
        let mut pane_is_leaf = false;
        let mut is_active = false;
        let mut is_prev = false;

        if let Ok(parent) = child_of_q.get(e).map(|co| co.get()) {
            parent_is_stack = tab_ts.get(parent).is_ok();
            if parent_is_stack && let Ok(pane) = child_of_q.get(parent).map(|co| co.get()) {
                pane_is_leaf = leaf_panes.contains(pane);
                if pane_is_leaf {
                    is_active =
                        active_stack_in_pane(pane, &pane_children_q, &tab_ts) == Some(parent);
                    // Keep previous tab's webview visible while an empty new tab is
                    // pending (user is picking content in the command bar).
                    is_prev = new_stack_ctx.stack.is_some()
                        && new_stack_ctx.previous_stack == Some(parent);
                }
            }
        }

        if should_show_osr_webview(
            window_visible,
            parent_is_stack,
            pane_is_leaf,
            is_active,
            is_prev,
        ) {
            browsers.set_osr_not_hidden(&e);
        } else {
            browsers.set_osr_hidden(&e);
        }
    }
}

fn webview_layout_is_renderable(
    size_px: Vec2,
    visibility: Option<&Visibility>,
    pending_reveal: bool,
) -> bool {
    (pending_reveal || !matches!(visibility, Some(Visibility::Hidden)))
        && size_px.x > 0.0
        && size_px.y > 0.0
}

fn webview_osr_should_run(
    size_px: Vec2,
    visibility: Option<&Visibility>,
    pending_reveal: bool,
) -> bool {
    pending_reveal || webview_layout_is_renderable(size_px, visibility, false)
}

fn choose_osr_active_webview(
    modal_keyboard_target: Option<Entity>,
    active_stack: Option<Entity>,
    fallback: Entity,
) -> Entity {
    modal_keyboard_target.or(active_stack).unwrap_or(fallback)
}

fn should_show_osr_webview(
    window_visible: bool,
    parent_is_stack: bool,
    pane_is_leaf: bool,
    stack_is_active: bool,
    stack_is_previous_new_stack: bool,
) -> bool {
    if !window_visible {
        return false;
    }
    if !parent_is_stack || !pane_is_leaf {
        return true;
    }
    stack_is_active || stack_is_previous_new_stack
}

fn drain_loading_state(receiver: Res<WebviewLoadingStateReceiver>, mut commands: Commands) {
    while let Ok(ev) = receiver.0.try_recv() {
        let Ok(mut ecmds) = commands.get_entity(ev.webview) else {
            continue;
        };
        if ev.is_loading {
            ecmds.insert(Loading);
        } else {
            ecmds.remove::<Loading>();
        }
        ecmds.insert(NavigationState {
            can_go_back: ev.can_go_back,
            can_go_forward: ev.can_go_forward,
        });
    }
}

fn drain_committed_navigation(
    receiver: Res<WebviewCommittedNavigationReceiver>,
    mut writer: MessageWriter<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>,
) {
    while let Ok(ev) = receiver.0.try_recv() {
        writer.write(ev);
    }
}

fn spawn_popup_stacks(
    popup_rx: Res<WebviewPopupReceiver>,
    child_of_q: Query<&ChildOf>,
    stack_q: Query<(), With<Stack>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    while let Ok(ev) = popup_rx.0.try_recv() {
        if ev.target_url.is_empty() {
            continue;
        }
        let Ok(stack_co) = child_of_q.get(ev.webview) else {
            continue;
        };
        let stack = stack_co.get();
        if !stack_q.contains(stack) {
            continue;
        }
        let Ok(pane_co) = child_of_q.get(stack) else {
            continue;
        };
        let pane = pane_co.get();
        if !leaf_panes.contains(pane) {
            continue;
        }
        let new_stack = commands
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        commands.spawn((
            Browser::new(&mut meshes, &mut webview_mt, &ev.target_url),
            ChildOf(new_stack),
        ));
    }
}

fn flush_pending_osr_textures(
    mut ew: MessageWriter<RenderTextureMessage>,
    browsers: NonSend<Browsers>,
) {
    while let Ok(texture) = browsers.try_receive_texture() {
        ew.write(texture);
    }
}

fn push_layout_state_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    header_q: Query<Has<Open>, With<Header>>,
    side_sheet_q: Query<(&SideSheetPosition, Has<Open>), With<SideSheet>>,
    side_sheet_width: Res<SideSheetWidth>,
    settings: Res<AppSettings>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }

    let payload = LayoutStateEvent {
        header_open: header_q.iter().any(|is_open| is_open),
        side_sheet_open: side_sheet_q
            .iter()
            .any(|(pos, is_open)| *pos == SideSheetPosition::Left && is_open),
        header_height: HEADER_HEIGHT_PX,
        side_sheet_width: side_sheet_width.0,
        pane_gap: vmux_layout::event::PANE_GAP_PX,
        radius: settings.layout.radius,
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !should_emit_cached_payload(&body, &last, page_ready.is_changed()) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        LAYOUT_STATE_EVENT,
        &payload,
    ));
    *last = body;
}

fn should_emit_new_stack_placeholder(
    pending_stack: Option<Entity>,
    active_stack: Option<Entity>,
    rows: &[StackRow],
) -> bool {
    let Some(pending_stack) = pending_stack else {
        return false;
    };
    if active_stack != Some(pending_stack) {
        return false;
    }
    !rows
        .iter()
        .any(|row| row.is_active && !row.url.is_empty() && row.url != "about:blank")
}

fn should_emit_cached_payload(body: &str, last: &str, page_ready_changed: bool) -> bool {
    page_ready_changed || body != last
}

fn push_stacks_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    browser_q: Query<(&PageMetadata, &ChildOf, Option<&NavigationState>), With<Browser>>,
    stack_q: Query<(), With<Stack>>,
    zoomed_q: Query<(), With<vmux_layout::pane::Zoomed>>,
    new_stack_ctx: Res<vmux_layout::NewStackContext>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    child_of_q: Query<&ChildOf>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }
    let active_pane = focus.pane;
    let active_stack_opt = focus.stack;
    if let Some(active_stack_entity) = active_stack_opt
        && !stack_q.contains(active_stack_entity)
    {
        return;
    }
    let mut rows: Vec<StackRow> = Vec::new();
    let mut can_go_back = false;
    let mut can_go_forward = false;
    let _ = active_stack_opt.is_none();
    if let Some(active_stack_entity) = active_stack_opt {
        for (meta, child_of, nav_state) in &browser_q {
            let stack_entity = child_of.get();
            let stack_pane = child_of_q.get(stack_entity).ok().map(|co| co.get());
            if stack_pane != active_pane {
                continue;
            }
            let is_active = stack_entity == active_stack_entity;
            if is_active && let Some(ns) = nav_state {
                can_go_back = ns.can_go_back;
                can_go_forward = ns.can_go_forward;
            }
            rows.push(StackRow {
                title: meta.title.clone(),
                url: meta.url.clone(),
                favicon_url: meta.favicon_url.clone(),
                is_active,
                bg_color: meta.bg_color.clone(),
            });
        }
    }
    if should_emit_new_stack_placeholder(new_stack_ctx.stack, active_stack_opt, &rows) {
        rows.retain(|r| !r.is_active);
        rows.push(StackRow {
            title: "New Stack".to_string(),
            url: String::new(),
            favicon_url: String::new(),
            is_active: true,
            bg_color: None,
        });
    }
    if active_stack_opt.is_some() && rows.is_empty() {
        return;
    }
    let is_zoomed = focus.tab.map(|t| zoomed_q.get(t).is_ok()).unwrap_or(false);
    let payload = StacksHostEvent {
        stacks: rows,
        can_go_back,
        can_go_forward,
        is_zoomed,
    };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !should_emit_cached_payload(&ron_body, &last, page_ready.is_changed()) {
        return;
    }
    warn!(
        "[stacks-debug] emitting StacksHostEvent: {} stacks, ron_len={}",
        payload.stacks.len(),
        ron_body.len()
    );
    commands.trigger(BinHostEmitEvent::from_rkyv(cef_e, STACKS_EVENT, &payload));
    *last = ron_body;
}

fn push_pane_tree_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    new_stack_ctx: Res<vmux_layout::NewStackContext>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    tab_q: Query<(), With<Tab>>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_q: Query<Entity, With<Stack>>,
    stack_children: Query<&Children>,
    browser_meta: Query<(&PageMetadata, Has<Loading>), With<Browser>>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }

    let active_pane = focus.pane;

    let Some(tab_e) = focus.tab else {
        return;
    };
    if !tab_q.contains(tab_e) {
        return;
    }
    let mut tab_leaf_panes = Vec::new();
    collect_leaf_panes(tab_e, &all_children, &leaf_pane_q, &mut tab_leaf_panes);

    let mut panes: Vec<PaneNode> = Vec::new();
    for &pane_entity in &tab_leaf_panes {
        let is_active = active_pane == Some(pane_entity);
        let active_stack = active_stack_in_pane(pane_entity, &pane_children, &stack_ts);
        let mut stacks: Vec<StackNode> = Vec::new();
        let mut stack_index: usize = 0;
        if let Ok(children) = pane_children.get(pane_entity) {
            for child in children.iter() {
                if !stack_q.contains(child) {
                    continue;
                }
                let stack_is_active = active_stack == Some(child);
                let mut found_browser = false;
                if let Ok(stack_kids) = stack_children.get(child) {
                    for browser_e in stack_kids.iter() {
                        if let Ok((meta, loading)) = browser_meta.get(browser_e) {
                            let is_new_stack = new_stack_ctx.stack == Some(child)
                                && (meta.url.is_empty() || meta.url == "about:blank");
                            stacks.push(StackNode {
                                title: if is_new_stack {
                                    "New Stack".to_string()
                                } else {
                                    meta.title.clone()
                                },
                                url: if is_new_stack {
                                    String::new()
                                } else {
                                    meta.url.clone()
                                },
                                favicon_url: if is_new_stack {
                                    String::new()
                                } else {
                                    meta.favicon_url.clone()
                                },
                                is_active: stack_is_active,
                                stack_index: stack_index as u32,
                                is_loading: loading,
                                bg_color: meta.bg_color.clone(),
                            });
                            found_browser = true;
                        }
                    }
                }
                if !found_browser {
                    stacks.push(StackNode {
                        title: "New Stack".to_string(),
                        url: String::new(),
                        favicon_url: String::new(),
                        is_active: stack_is_active,
                        stack_index: stack_index as u32,
                        is_loading: false,
                        bg_color: None,
                    });
                }
                stack_index += 1;
            }
        }
        panes.push(PaneNode {
            id: pane_entity.to_bits(),
            is_active,
            stacks,
        });
    }
    let payload = PaneTreeEvent { panes };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !should_emit_cached_payload(&ron_body, &last, page_ready.is_changed()) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        PANE_TREE_EVENT,
        &payload,
    ));
    *last = ron_body;
}

#[allow(clippy::too_many_arguments)]
fn push_tabs_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    tabs: Query<(Entity, &Tab, &LastActivatedAt)>,
    tab_q: Query<Entity, With<Tab>>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_children: Query<&Children>,
    browser_meta: Query<&PageMetadata, With<Browser>>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }

    let active_tab = tabs.iter().max_by_key(|(_, _, ts)| ts.0).map(|t| t.0);

    let ordered = if let Some(any) = tabs.iter().next() {
        vmux_layout::tab::active_tab_siblings(any.0, &child_of_q, &all_children, &tab_q)
    } else {
        Vec::new()
    };

    let rows: Vec<TabRow> = ordered
        .iter()
        .filter_map(|e| tabs.get(*e).ok())
        .map(|(entity, tab, _)| {
            let active_stack = active_stack_in_tab(
                entity,
                &all_children,
                &leaf_pane_q,
                &pane_children,
                &stack_ts,
            );
            let meta = active_stack
                .and_then(|s| first_browser_meta(s, &stack_children, &browser_meta))
                .cloned()
                .unwrap_or_default();
            let name = if tab.name.is_empty() {
                "Tab".to_string()
            } else {
                tab.name.clone()
            };
            TabRow {
                id: entity.to_bits().to_string(),
                name,
                is_active: Some(entity) == active_tab,
                bg_color: meta.bg_color.clone(),
                title: meta.title.clone(),
                url: meta.url.clone(),
                favicon_url: meta.favicon_url.clone(),
            }
        })
        .collect();

    let payload = TabsHostEvent { tabs: rows };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !page_ready.is_changed() && body == *last {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(cef_e, TABS_EVENT, &payload));
    *last = body;
}

fn active_stack_in_tab(
    tab_e: Entity,
    all_children: &Query<&Children>,
    leaf_pane_q: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
) -> Option<Entity> {
    let mut leaves = Vec::new();
    collect_leaf_panes(tab_e, all_children, leaf_pane_q, &mut leaves);
    leaves
        .into_iter()
        .filter_map(|p| active_stack_in_pane(p, pane_children, stack_ts).map(|s| (s, p)))
        .filter_map(|(s, _)| stack_ts.get(s).ok())
        .max_by_key(|(_, ts)| ts.0)
        .map(|(e, _)| e)
}

fn first_browser_meta<'a>(
    stack: Entity,
    stack_children: &Query<&Children>,
    browser_meta: &'a Query<&PageMetadata, With<Browser>>,
) -> Option<&'a PageMetadata> {
    let kids = stack_children.get(stack).ok()?;
    kids.iter().find_map(|c| browser_meta.get(c).ok())
}

fn handle_browser_commands(
    mut reader: MessageReader<AppCommand>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut zoom_q: Query<&mut ZoomLevel, With<Browser>>,
    mut meta_q: Query<&mut PageMetadata, With<Browser>>,
    terminal_q: Query<(), With<Terminal>>,
    effective_startup_url: Option<Res<vmux_layout::settings::EffectiveStartupUrl>>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Browser(browser_cmd) = cmd else {
            continue;
        };
        let (_, _, active_stack_opt) = focused_stack(
            &tabs,
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );
        let Some(active) = active_stack_opt else {
            continue;
        };
        let Some(webview) = browsers
            .iter()
            .find(|(_, co)| co.get() == active)
            .map(|(e, _)| e)
        else {
            continue;
        };
        let is_terminal = terminal_q.contains(webview);
        match browser_cmd {
            BrowserCommand::Navigation(nav) => match nav {
                BrowserNavigationCommand::PrevPage => {
                    if !is_terminal {
                        commands.trigger(RequestGoBack { webview });
                    }
                }
                BrowserNavigationCommand::NextPage => {
                    if !is_terminal {
                        commands.trigger(RequestGoForward { webview });
                    }
                }
                BrowserNavigationCommand::Reload => {
                    if is_terminal {
                        commands.trigger(RestartPty { entity: webview });
                    } else {
                        commands.trigger(RequestReload { webview });
                    }
                }
                BrowserNavigationCommand::HardReload => {
                    if is_terminal {
                        commands.trigger(RestartPty { entity: webview });
                    } else {
                        commands.trigger(RequestReloadIgnoreCache { webview });
                    }
                }
                BrowserNavigationCommand::Stop => {}
            },
            #[allow(clippy::single_match)]
            BrowserCommand::Open(open_cmd) => match open_cmd {
                OpenCommand::InPlace { url } => {
                    let resolved = vmux_command::open::handler::resolve_url(
                        url.as_deref(),
                        effective_startup_url.as_ref().map(|s| s.0.as_str()),
                    );
                    if resolved.is_empty() {
                        continue;
                    }
                    let on_native_view = meta_q
                        .get(webview)
                        .map(|m| m.url.starts_with("vmux://"))
                        .unwrap_or(false);
                    if is_terminal || on_native_view || resolved.starts_with("vmux://") {
                        page_open_requests.write(PageOpenRequest {
                            target: PageOpenTarget::Stack(active),
                            url: resolved,
                            request_id: None,
                        });
                        continue;
                    }
                    if is_terminal {
                        commands
                            .entity(webview)
                            .remove::<Terminal>()
                            .remove::<vmux_service::protocol::ProcessId>();
                    }
                    if let Ok(mut meta) = meta_q.get_mut(webview) {
                        meta.url = resolved.clone();
                        meta.title = resolved.clone();
                        meta.favicon_url.clear();
                    }
                    commands
                        .entity(webview)
                        .insert(WebviewSource::new(&resolved));
                    commands.trigger(RequestNavigate {
                        webview,
                        url: resolved,
                    });
                }
                _ => {}
            },
            BrowserCommand::View(view) => match view {
                BrowserViewCommand::ZoomIn => {
                    if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 += 0.5;
                    }
                }
                BrowserViewCommand::ZoomOut => {
                    if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 -= 0.5;
                    }
                }
                BrowserViewCommand::ZoomReset => {
                    if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 = 0.0;
                    }
                }
                BrowserViewCommand::DevTools => {
                    commands.trigger(RequestShowDevTool { webview });
                }
                BrowserViewCommand::ViewSource => {}
                BrowserViewCommand::Print => {}
            },
            BrowserCommand::Bar(_) => {}
        }
    }
}

fn on_header_command_emit(
    trigger: On<BinReceive<HeaderCommandEvent>>,
    mut messages: ResMut<Messages<AppCommand>>,
) {
    let cmd = match trigger.event().payload.header_command.as_str() {
        "prev_page" => BrowserCommand::Navigation(BrowserNavigationCommand::PrevPage),
        "next_page" => BrowserCommand::Navigation(BrowserNavigationCommand::NextPage),
        "reload" => BrowserCommand::Navigation(BrowserNavigationCommand::Reload),
        "focus_address_bar" => BrowserCommand::Bar(BrowserBarCommand::OpenPageInCommandBar),
        _ => return,
    };
    messages.write(AppCommand::Browser(cmd));
}

fn on_reload_notify_header(
    _trigger: On<RequestReload>,
    cef: Option<Single<Entity, (With<LayoutCef>, With<PageReady>)>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(cef) = cef else { return };
    let cef_e = *cef;
    if browsers.has_browser(cef_e) && browsers.host_emit_ready(&cef_e) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            RELOAD_EVENT,
            &ReloadEvent,
        ));
    }
}

fn on_hard_reload_notify_header(
    _trigger: On<RequestReloadIgnoreCache>,
    cef: Option<Single<Entity, (With<LayoutCef>, With<PageReady>)>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(cef) = cef else { return };
    let cef_e = *cef;
    if browsers.has_browser(cef_e) && browsers.host_emit_ready(&cef_e) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            RELOAD_EVENT,
            &ReloadEvent,
        ));
    }
}

fn on_side_sheet_command_emit(
    trigger: On<BinReceive<SideSheetCommandEvent>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    tab_query: Query<(Entity, &LastActivatedAt), With<Tab>>,
    all_children: Query<&Children>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_q: Query<Entity, With<Stack>>,
    child_of_q: Query<&ChildOf>,
    split_q: Query<(), With<PaneSplit>>,
    mut close_extra: ParamSet<(
        Query<'static, 'static, &'static mut Window, With<PrimaryWindow>>,
        Query<'static, 'static, (), (With<Terminal>, Without<PtyExited>)>,
        Query<'static, 'static, (), With<CloseConfirmed>>,
        Query<'static, 'static, (), With<PendingStackClose>>,
    )>,
    mut hover_intent: ResMut<PaneHoverIntent>,
    settings: Res<AppSettings>,
    mut messages: ResMut<Messages<AppCommand>>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;
    let Ok(pane_id) = evt.pane_id.parse::<u64>() else {
        return;
    };
    let Some(target_pane) = leaf_panes.iter().find(|e| e.to_bits() == pane_id) else {
        return;
    };
    let Ok(children) = pane_children.get(target_pane) else {
        return;
    };
    let stack_entities: Vec<Entity> = children.iter().filter(|&e| stack_q.contains(e)).collect();

    match evt.command.as_str() {
        "activate_stack" => {
            let Some(&target_stack) = stack_entities.get(evt.stack_index as usize) else {
                return;
            };
            commands.entity(target_pane).insert(LastActivatedAt::now());
            commands.entity(target_stack).insert(LastActivatedAt::now());

            hover_intent.target = None;
            hover_intent.last_activation = Some(std::time::Instant::now());
        }
        "close_stack" => {
            let Some(&target_stack) = stack_entities.get(evt.stack_index as usize) else {
                return;
            };

            // Confirm close if terminal is still running
            let needs_confirm = terminal::should_confirm_close(&settings)
                && terminal::has_live_terminal(target_stack, &all_children, &close_extra.p1());
            if needs_confirm {
                if close_extra.p2().contains(target_stack) {
                    commands.entity(target_stack).remove::<CloseConfirmed>();
                } else {
                    if !close_extra.p3().contains(target_stack) {
                        commands.entity(target_stack).insert(PendingStackClose);
                    }
                    return;
                }
            }

            if stack_entities.len() > 1 {
                let is_active = active_stack_in_pane(target_pane, &pane_children, &stack_ts)
                    == Some(target_stack);
                commands.entity(target_stack).despawn();
                if is_active {
                    let next = stack_entities
                        .iter()
                        .copied()
                        .find(|&e| e != target_stack)
                        .unwrap();
                    commands.entity(next).insert(LastActivatedAt::now());
                }
            } else if leaf_panes.iter().count() <= 1 {
                if let Ok(mut window) = close_extra.p0().single_mut() {
                    window.visible = false;
                }
            } else {
                let Ok(pane_co) = child_of_q.get(target_pane) else {
                    return;
                };
                let parent = pane_co.get();
                let Ok(parent_children) = pane_children.get(parent) else {
                    return;
                };
                let is_pane = |e: Entity| leaf_panes.contains(e) || split_q.contains(e);
                let Some(sibling) = parent_children
                    .iter()
                    .find(|&e| e != target_pane && is_pane(e))
                else {
                    return;
                };

                let (_, current_active_pane, _) = focused_stack(
                    &tab_query,
                    &all_children,
                    &leaf_panes,
                    &pane_ts,
                    &pane_children,
                    &stack_ts,
                );
                let target_pane_is_active = current_active_pane == Some(target_pane);
                let sibling_is_active = current_active_pane == Some(sibling);

                let sibling_children: Vec<Entity> = pane_children
                    .get(sibling)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();
                for &child in &sibling_children {
                    commands.entity(child).insert(ChildOf(parent));
                }

                let (new_active_pane, tab_to_activate);
                if split_q.contains(sibling) {
                    new_active_pane = first_leaf_descendant(sibling, &pane_children, &leaf_panes);
                    tab_to_activate =
                        active_stack_in_pane(new_active_pane, &pane_children, &stack_ts).or_else(
                            || first_stack_in_pane(new_active_pane, &pane_children, &stack_q),
                        );
                    commands.entity(sibling).remove::<ChildOf>();
                    commands.queue(move |world: &mut World| {
                        world.despawn(sibling);
                    });
                } else {
                    new_active_pane = parent;
                    tab_to_activate = sibling_children
                        .iter()
                        .copied()
                        .find(|&e| stack_ts.get(e).is_ok())
                        .or_else(|| {
                            sibling_children
                                .iter()
                                .copied()
                                .find(|&e| stack_q.contains(e))
                        });
                    commands.entity(parent).remove::<PaneSplit>();
                    commands.entity(parent).insert(Node {
                        flex_grow: 1.0,
                        flex_basis: Val::Px(0.0),
                        align_items: AlignItems::Stretch,
                        justify_content: JustifyContent::Stretch,
                        ..default()
                    });
                    commands.entity(sibling).despawn();
                }

                commands.entity(target_pane).despawn();

                if target_pane_is_active || sibling_is_active {
                    commands
                        .entity(new_active_pane)
                        .insert(LastActivatedAt::now());
                    if let Some(tab) = tab_to_activate {
                        commands.entity(tab).insert(LastActivatedAt::now());
                    }
                }
            }

            hover_intent.target = None;
            hover_intent.last_activation = Some(std::time::Instant::now());
        }
        "new_stack" => {
            commands.entity(target_pane).insert(LastActivatedAt::now());
            messages.write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewStack { url: None },
            )));
        }
        _ => {}
    }
}

fn spawn_visit_on_navigation(
    changed_tabs: Query<(Entity, &PageMetadata), (With<Stack>, Changed<PageMetadata>)>,
    mut last_urls: Local<std::collections::HashMap<u64, String>>,
    mut commands: Commands,
) {
    for (entity, meta) in &changed_tabs {
        if meta.url.is_empty() || meta.url == "about:blank" {
            continue;
        }

        let key = entity.to_bits();
        let is_new = last_urls
            .get(&key)
            .map(|prev| prev != &meta.url)
            .unwrap_or(true);

        if is_new {
            last_urls.insert(key, meta.url.clone());
            commands.spawn((Visit, meta.clone(), CreatedAt::now()));
        }
    }
}

fn sync_page_metadata_to_tab(
    browser_q: Query<(&PageMetadata, &ChildOf), (With<Browser>, Changed<PageMetadata>)>,
    tab_q: Query<Option<&PageMetadata>, With<Stack>>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    mut commands: Commands,
) {
    for (meta, child_of) in &browser_q {
        let parent = child_of.get();
        let Ok(parent_meta) = tab_q.get(parent) else {
            continue;
        };
        if status_q.contains(parent) || side_sheet_q.contains(parent) {
            continue;
        }
        if parent_meta
            .as_ref()
            .is_some_and(|m| m.url.starts_with("vmux://agent/"))
        {
            continue;
        }
        if let Some(parent_url) = parent_meta.as_ref().map(|m| m.url.as_str())
            && parent_url.starts_with("vmux://")
            && (meta.url.starts_with("data:") || meta.url.is_empty())
        {
            continue;
        }
        if let Ok(mut ecmds) = commands.get_entity(parent) {
            ecmds.insert(meta.clone());
        }
    }
}

pub fn handle_browser_go_back_requests(
    mut reader: MessageReader<vmux_layout::BrowserGoBackRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<terminal::ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let target = match request.pane.as_deref() {
            Some(s) => vmux_layout::target::parse_pane_target(s, &panes),
            None => focus.pane.filter(|p| panes.contains(*p)),
        };
        let Some(pane_entity) = target else { continue };
        let Some(webview) = vmux_layout::target::active_webview_for_tab(
            active_stack_in_pane(pane_entity, &pane_children, &stack_ts),
            &browsers,
            &terminals,
        ) else {
            continue;
        };
        commands.trigger(bevy_cef::prelude::RequestGoBack { webview });
    }
}

pub fn handle_browser_go_forward_requests(
    mut reader: MessageReader<vmux_layout::BrowserGoForwardRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<terminal::ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let target = match request.pane.as_deref() {
            Some(s) => vmux_layout::target::parse_pane_target(s, &panes),
            None => focus.pane.filter(|p| panes.contains(*p)),
        };
        let Some(pane_entity) = target else { continue };
        let Some(webview) = vmux_layout::target::active_webview_for_tab(
            active_stack_in_pane(pane_entity, &pane_children, &stack_ts),
            &browsers,
            &terminals,
        ) else {
            continue;
        };
        commands.trigger(bevy_cef::prelude::RequestGoForward { webview });
    }
}

pub fn handle_browser_open_history(
    mut reader: MessageReader<AppCommand>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    mut writer: MessageWriter<PageOpenRequest>,
) {
    for cmd in reader.read() {
        if matches!(
            cmd,
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenHistory))
        ) {
            let Some(pane) = focus.pane else {
                continue;
            };
            writer.write(PageOpenRequest {
                target: PageOpenTarget::NewStackInPane(pane),
                url: "vmux://history/".to_string(),
                request_id: None,
            });
        }
    }
}

fn handle_page_open_requests(
    mut reader: MessageReader<PageOpenRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_filter: Query<Entity, With<Stack>>,
    service: Option<Res<vmux_service::client::ServiceClient>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let stack = match resolve_page_open_target(
            &request.target,
            &focus,
            &panes,
            &pane_children,
            &stack_ts,
            &stack_filter,
            &mut commands,
        ) {
            Ok(stack) => stack,
            Err(message) => {
                send_page_open_response(&service, request.request_id, Err(message));
                continue;
            }
        };
        commands.spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: request.url.clone(),
            request_id: request.request_id,
        });
    }
}

fn resolve_page_open_target(
    target: &PageOpenTarget,
    focus: &vmux_layout::stack::FocusedStack,
    panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_filter: &Query<Entity, With<Stack>>,
    commands: &mut Commands,
) -> Result<Entity, String> {
    match *target {
        PageOpenTarget::ActiveStack => focus
            .stack
            .or_else(|| {
                focus.pane.filter(|pane| panes.contains(*pane)).map(|pane| {
                    commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id()
                })
            })
            .ok_or_else(|| "page_open: no focused stack or pane".to_string()),
        PageOpenTarget::Stack(stack) => {
            if stack_filter.contains(stack) {
                Ok(stack)
            } else {
                Err("page_open: target stack does not exist".to_string())
            }
        }
        PageOpenTarget::ActiveStackInPane(pane) => {
            if !panes.contains(pane) {
                return Err("page_open: target pane does not exist".to_string());
            }
            Ok(active_stack_in_pane(pane, pane_children, stack_ts)
                .or_else(|| first_stack_in_pane(pane, pane_children, stack_filter))
                .unwrap_or_else(|| {
                    commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id()
                }))
        }
        PageOpenTarget::NewStackInPane(pane) => {
            if panes.contains(pane) {
                Ok(commands
                    .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                    .id())
            } else {
                Err("page_open: target pane does not exist".to_string())
            }
        }
    }
}

fn attach_cef_page_requests(
    mut reader: MessageReader<CefPageAttachRequest>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for request in reader.read() {
        attach_cef_page_to_stack(
            request.stack,
            &request.url,
            &request.title,
            request.bg_color.clone(),
            &children_q,
            &mut commands,
            &mut meshes,
            &mut webview_mt,
        );
    }
}

fn handle_unclaimed_page_open_tasks(
    mut tasks: Query<(Entity, &PageOpenTask, Option<&PageOpenError>), Without<PageOpenHandled>>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for (entity, task, error) in &mut tasks {
        if let Some(error) = error {
            attach_error_page_to_stack(
                task.stack,
                "vmux://error/page-open/",
                "Page failed to load",
                &error.message,
                &children_q,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            commands.entity(entity).insert(PageOpenHandled);
        } else if task.url.starts_with("vmux://error/") {
            attach_error_page_to_stack(
                task.stack,
                &task.url,
                "Page failed to load",
                &task.url,
                &children_q,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            commands.entity(entity).insert(PageOpenHandled);
        } else if task.url.starts_with("vmux://") {
            let message = format!("Page not found: {}", task.url);
            attach_error_page_to_stack(
                task.stack,
                "vmux://error/not-found/",
                "Page not found",
                &message,
                &children_q,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            commands.entity(entity).insert((
                PageOpenHandled,
                PageOpenError {
                    message: format!("unknown vmux URL '{}'", task.url),
                },
            ));
        } else {
            attach_cef_page_to_stack(
                task.stack,
                &task.url,
                &task.url,
                None,
                &children_q,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            commands.entity(entity).insert(PageOpenHandled);
        }
    }
}

fn respond_page_open_tasks(
    tasks: Query<(Entity, &PageOpenTask, Option<&PageOpenError>), With<PageOpenHandled>>,
    service: Option<Res<vmux_service::client::ServiceClient>>,
    mut commands: Commands,
) {
    for (entity, task, error) in &tasks {
        let result = match error {
            Some(error) => Err(error.message.clone()),
            None => Ok(()),
        };
        send_page_open_response(&service, task.request_id, result);
        commands.entity(entity).despawn();
    }
}

fn send_page_open_response(
    service: &Option<Res<vmux_service::client::ServiceClient>>,
    request_id: Option<[u8; 16]>,
    result: Result<(), String>,
) {
    use vmux_service::protocol::{AgentCommandResult, AgentRequestId, ClientMessage};
    let (Some(service), Some(request_id)) = (service.as_ref(), request_id) else {
        return;
    };
    let result = match result {
        Ok(()) => AgentCommandResult::Ok,
        Err(message) => AgentCommandResult::Error(message),
    };
    service.0.send(ClientMessage::AgentCommandResponse {
        request_id: AgentRequestId(request_id),
        result,
    });
}

fn attach_cef_page_to_stack(
    stack: Entity,
    url: &str,
    title: &str,
    bg_color: Option<String>,
    children_q: &Query<&Children>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    clear_stack_children(stack, children_q, commands);
    commands.entity(stack).insert(PageMetadata {
        url: url.to_string(),
        title: title.to_string(),
        bg_color,
        ..default()
    });
    let browser = commands
        .spawn((
            Browser::new_with_title(meshes, webview_mt, url, title),
            ChildOf(stack),
        ))
        .id();
    commands.entity(browser).insert(CefKeyboardTarget);
}

fn attach_error_page_to_stack(
    stack: Entity,
    url: &str,
    title: &str,
    message: &str,
    children_q: &Query<&Children>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let html = format!(
        "<!doctype html><html><head><meta charset='utf-8'><title>{title}</title><style>html,body{{height:100%;margin:0;background:#101114;color:#e8e8ea;font-family:-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif}}main{{height:100%;display:flex;align-items:center;justify-content:center;padding:40px;box-sizing:border-box}}section{{max-width:640px}}h1{{font-size:28px;line-height:1.15;margin:0 0 12px;font-weight:650}}p{{font-size:14px;line-height:1.55;margin:0;color:#a9abb2}}code{{display:block;margin-top:18px;padding:12px;border-radius:6px;background:#1a1c22;color:#d7d8dd;white-space:pre-wrap;word-break:break-word}}</style></head><body><main><section><h1>{title}</h1><p>{message}</p><code>{url}</code></section></main></body></html>"
    );
    attach_cef_page_to_stack(
        stack,
        &data_url_for_html(&html),
        title,
        Some("#101114".to_string()),
        children_q,
        commands,
        meshes,
        webview_mt,
    );
}

fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
}

fn data_url_for_html(html: &str) -> String {
    let mut encoded = String::with_capacity(html.len() * 3);
    for byte in html.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    format!("data:text/html;charset=utf-8,{encoded}")
}

pub fn handle_open_in_new_stack_requests(
    mut reader: MessageReader<vmux_layout::OpenInNewStackRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut page_open_writer: MessageWriter<PageOpenRequest>,
) {
    for request in reader.read() {
        let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) else {
            continue;
        };
        page_open_writer.write(PageOpenRequest {
            target: PageOpenTarget::NewStackInPane(pane),
            url: request.url.clone(),
            request_id: None,
        });
    }
}

pub fn handle_browser_navigate_requests(
    mut reader: MessageReader<vmux_layout::BrowserNavigateRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<terminal::ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    service: Option<Res<vmux_service::client::ServiceClient>>,
    mut commands: Commands,
    mut page_open_writer: MessageWriter<PageOpenRequest>,
) {
    for request in reader.read() {
        let vmux_layout::BrowserNavigateRequest {
            url,
            pane,
            request_id,
        } = request.clone();

        if let Some(s) = pane.as_deref() {
            if let Some(target) = vmux_layout::target::parse_pane_target(s, &panes) {
                page_open_writer.write(PageOpenRequest {
                    target: PageOpenTarget::NewStackInPane(target),
                    url,
                    request_id,
                });
            } else {
                send_page_open_response(
                    &service,
                    request_id,
                    Err(format!("browser_navigate: invalid pane id '{s}'")),
                );
            }
        } else if let Some(webview) =
            vmux_layout::target::active_webview_for_tab(focus.stack, &browsers, &terminals)
        {
            if url.starts_with("vmux://") {
                let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) else {
                    send_page_open_response(
                        &service,
                        request_id,
                        Err("browser_navigate: no focused pane for vmux URL".to_string()),
                    );
                    continue;
                };
                page_open_writer.write(PageOpenRequest {
                    target: PageOpenTarget::NewStackInPane(pane),
                    url,
                    request_id,
                });
            } else {
                commands.trigger(RequestNavigate {
                    webview,
                    url: url.clone(),
                });
                send_page_open_response(&service, request_id, Ok(()));
            }
        } else if let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) {
            page_open_writer.write(PageOpenRequest {
                target: PageOpenTarget::NewStackInPane(pane),
                url,
                request_id,
            });
        } else {
            send_page_open_response(
                &service,
                request_id,
                Err("browser_navigate: no focused pane".to_string()),
            );
        }
    }
}

fn cef_root_cache_path() -> Option<String> {
    vmux_core::profile::cef_cache_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osr_webview_hides_when_window_is_hidden() {
        assert!(!should_show_osr_webview(true, true, true, false, false));
        assert!(!should_show_osr_webview(false, true, true, true, false));
        assert!(!should_show_osr_webview(false, false, true, false, false));
        assert!(should_show_osr_webview(true, true, true, true, false));
    }

    #[test]
    fn auxiliary_osr_webviews_remain_visible_when_window_is_focused() {
        assert!(should_show_osr_webview(true, false, true, false, false));
        assert!(should_show_osr_webview(true, true, false, false, false));
        assert!(should_show_osr_webview(true, true, true, false, true));
    }

    #[test]
    fn cef_pointer_hit_rect_contains_edges() {
        let rect = CefPointerHitRect {
            center: Vec2::new(50.0, 20.0),
            size: Vec2::new(100.0, 40.0),
            interactive: true,
        };

        assert!(cef_pointer_hit_rect_contains(rect, Vec2::new(0.0, 0.0)));
        assert!(cef_pointer_hit_rect_contains(rect, Vec2::new(100.0, 40.0)));
        assert!(!cef_pointer_hit_rect_contains(rect, Vec2::new(100.1, 20.0)));
    }

    #[test]
    fn cef_pointer_ignores_inactive_regions() {
        let rect = CefPointerHitRect {
            center: Vec2::new(50.0, 20.0),
            size: Vec2::new(100.0, 40.0),
            interactive: false,
        };

        assert!(!cef_pointer_hit_rect_contains(rect, Vec2::new(50.0, 20.0)));
    }

    #[test]
    fn hidden_or_collapsed_webviews_do_not_render() {
        assert!(!webview_layout_is_renderable(
            Vec2::ZERO,
            Some(&Visibility::Inherited),
            false
        ));
        assert!(!webview_layout_is_renderable(
            Vec2::new(100.0, 0.0),
            Some(&Visibility::Inherited),
            false
        ));
        assert!(!webview_layout_is_renderable(
            Vec2::new(100.0, 20.0),
            Some(&Visibility::Hidden),
            false
        ));
        assert!(webview_layout_is_renderable(
            Vec2::new(100.0, 20.0),
            Some(&Visibility::Inherited),
            false
        ));
    }

    #[test]
    fn hidden_pending_reveal_webviews_resize_before_reveal() {
        assert!(webview_layout_is_renderable(
            Vec2::new(100.0, 20.0),
            Some(&Visibility::Hidden),
            true
        ));
    }

    #[test]
    fn pending_reveal_webviews_keep_cef_running() {
        assert!(webview_osr_should_run(
            Vec2::ZERO,
            Some(&Visibility::Hidden),
            true
        ));
    }

    #[test]
    fn command_bar_modal_wins_osr_focus_for_keyboard_input() {
        let pane = Entity::from_bits(1);
        let modal = Entity::from_bits(2);

        assert_eq!(
            choose_osr_active_webview(Some(modal), Some(pane), pane),
            modal
        );
    }

    #[test]
    fn windowed_chrome_sync_sends_layout_behind_pages() {
        let source = include_str!("lib.rs");
        let sync_fn = source
            .split("fn sync_windowed_chrome")
            .nth(1)
            .and_then(|tail| tail.split("fn apply_repaint_nudge").next())
            .unwrap_or_default();

        assert!(sync_fn.contains("browsers.lower_windowed_to_back"));
    }

    #[test]
    fn windowed_page_sync_sends_pages_above_layout() {
        let source = include_str!("lib.rs");
        let sync_fn = source
            .split("fn sync_windowed_frames")
            .nth(1)
            .and_then(|tail| tail.split("fn sync_windowed_chrome").next())
            .unwrap_or_default();

        assert!(sync_fn.contains("browsers.raise_windowed_to_front"));
    }

    #[test]
    fn browser_mode_uses_windowed_webviews_on_macos() {
        assert_eq!(
            webview_should_use_windowed(vmux_layout::scene::InteractionMode::User),
            cfg!(target_os = "macos")
        );
    }

    #[test]
    fn player_mode_uses_osr_webviews() {
        assert!(!webview_should_use_windowed(
            vmux_layout::scene::InteractionMode::Player
        ));
    }

    #[test]
    fn browser_mode_marks_every_cef_surface_windowed_on_macos() {
        let mut app = App::new();
        app.world_mut().insert_non_send(Browsers::default());
        app.insert_resource(vmux_layout::scene::InteractionMode::User);

        let layout = app
            .world_mut()
            .spawn((Browser, LayoutCef, WebviewSource::new("vmux://layout/")))
            .id();
        let modal = app
            .world_mut()
            .spawn((Browser, Modal, WebviewSource::new("vmux://command-bar/")))
            .id();
        let page = app
            .world_mut()
            .spawn((Browser, WebviewSource::new("https://example.com/")))
            .id();

        sync_cef_backend_for_interaction_mode(app.world_mut());

        assert_eq!(
            app.world().get::<WebviewWindowed>(layout).is_some(),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            app.world().get::<WebviewWindowed>(modal).is_some(),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            app.world().get::<WebviewWindowed>(page).is_some(),
            cfg!(target_os = "macos")
        );
    }

    #[test]
    fn player_mode_marks_every_cef_surface_osr() {
        let mut app = App::new();
        app.world_mut().insert_non_send(Browsers::default());
        app.insert_resource(vmux_layout::scene::InteractionMode::Player);

        let layout = app
            .world_mut()
            .spawn((
                Browser,
                LayoutCef,
                WebviewWindowed,
                WebviewSource::new("vmux://layout/"),
            ))
            .id();
        let modal = app
            .world_mut()
            .spawn((
                Browser,
                Modal,
                WebviewWindowed,
                WebviewSource::new("vmux://command-bar/"),
            ))
            .id();
        let page = app
            .world_mut()
            .spawn((
                Browser,
                WebviewWindowed,
                WebviewSource::new("https://example.com/"),
            ))
            .id();

        sync_cef_backend_for_interaction_mode(app.world_mut());

        assert!(app.world().get::<WebviewWindowed>(layout).is_none());
        assert!(app.world().get::<WebviewWindowed>(modal).is_none());
        assert!(app.world().get::<WebviewWindowed>(page).is_none());
    }

    #[test]
    fn backend_sync_runs_after_page_spawners_before_cef_create() {
        let source = include_str!("lib.rs");
        let plugin_build = source
            .split("impl Plugin for BrowserPlugin")
            .nth(1)
            .and_then(|tail| tail.split("fn cef_root_cache_path").next())
            .unwrap_or_default();

        assert!(plugin_build.contains(".after(PageOpenSet::Fallback)"));
        assert!(plugin_build.contains(".after(spawn_popup_stacks)"));
        assert!(plugin_build.contains(".before(CefSystems::CreateAndResize)"));
    }

    #[test]
    fn command_bar_windowed_frame_uses_measured_height() {
        let frame =
            command_bar_windowed_frame(1600.0, 1000.0, 2.0, Some(Vec2::new(500.0, 220.0))).unwrap();

        assert!((frame.left_px - 224.0).abs() < 0.01);
        assert!((frame.top_px - 150.0).abs() < 0.01);
        assert!((frame.width_px - 1152.0).abs() < 0.01);
        assert!((frame.height_px - 440.0).abs() < 0.01);
    }

    #[test]
    fn command_bar_windowed_frame_clamps_height_to_window() {
        let frame =
            command_bar_windowed_frame(800.0, 500.0, 1.0, Some(Vec2::new(500.0, 1000.0))).unwrap();

        assert!((frame.top_px - 75.0).abs() < 0.01);
        assert!((frame.height_px - 409.0).abs() < 0.01);
    }

    #[test]
    fn command_bar_hidden_windowed_frame_collapses_native_view() {
        let frame = command_bar_hidden_windowed_frame();

        assert_eq!(frame.left_px, 0.0);
        assert_eq!(frame.top_px, 0.0);
        assert_eq!(frame.width_px, 1.0);
        assert_eq!(frame.height_px, 1.0);
    }

    #[test]
    fn command_bar_windowed_view_waits_for_reveal() {
        assert!(!command_bar_windowed_view_should_show(
            Display::Flex,
            Visibility::Hidden,
            true
        ));
        assert!(command_bar_windowed_view_should_show(
            Display::Flex,
            Visibility::Inherited,
            true
        ));
    }

    #[test]
    fn generic_webview_resize_excludes_command_bar_modal() {
        let source = include_str!("lib.rs");
        let resize_fn = source
            .split("fn sync_cef_webview_resize_after_ui")
            .nth(1)
            .and_then(|tail| tail.split("fn pane_count_for_browser").next())
            .unwrap_or_default();

        assert!(resize_fn.contains("Without<Modal>"));
    }

    #[test]
    fn command_bar_windowed_sync_resizes_cef_to_native_frame() {
        let source = include_str!("lib.rs");
        let sync_fn = source
            .split("fn sync_windowed_command_bar")
            .nth(1)
            .and_then(|tail| tail.split("fn apply_repaint_nudge").next())
            .unwrap_or_default();

        assert!(sync_fn.contains("browsers.resize"));
    }

    #[test]
    fn active_browser_url_wins_over_stale_new_stack_placeholder() {
        let stack = Entity::from_bits(1);
        let rows = [StackRow {
            title: "Google".to_string(),
            url: "https://www.google.com".to_string(),
            favicon_url: String::new(),
            is_active: true,
            bg_color: None,
        }];

        assert!(!should_emit_new_stack_placeholder(
            Some(stack),
            Some(stack),
            &rows
        ));
    }

    #[test]
    fn host_payload_emits_again_when_page_ready_changes() {
        assert!(should_emit_cached_payload("tabs", "tabs", true));
        assert!(should_emit_cached_payload("tabs-2", "tabs", false));
        assert!(!should_emit_cached_payload("tabs", "tabs", false));
    }

    mod browser_navigate_flow {
        use bevy::ecs::relationship::Relationship;
        use bevy::prelude::*;
        use bevy_cef::prelude::WebviewExtendStandardMaterial;
        use vmux_agent::events::AgentCommandRequest;
        use vmux_agent::plugin::AgentPlugin;
        use vmux_agent::strategy::AgentStrategies;
        use vmux_core::{
            CefPageAttachRequest, PageMetadata, PageOpenError, PageOpenHandled, PageOpenId,
            PageOpenRequest, PageOpenSet, PageOpenTask,
        };
        use vmux_layout::pane::Pane;
        use vmux_layout::settings::{
            FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
        };
        use vmux_layout::stack::FocusedStack;
        use vmux_service::protocol::{AgentCommand as ServiceAgentCommand, AgentRequestId};
        use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};
        use vmux_terminal::Terminal;

        fn test_settings() -> AppSettings {
            AppSettings {
                browser: BrowserSettings {
                    startup_url: "about:blank".to_string(),
                },
                layout: LayoutSettings {
                    radius: 0.0,
                    window: WindowSettings {
                        padding: 0.0,
                        padding_top: None,
                        padding_right: None,
                        padding_bottom: None,
                        padding_left: None,
                    },
                    pane: PaneSettings { gap: 0.0 },
                    side_sheet: SideSheetSettings::default(),
                    focus_ring: FocusRingSettings::default(),
                },
                shortcuts: ShortcutSettings::default(),
                terminal: None,
                auto_update: false,
                agent: vmux_setting::AgentSettings::default(),
            }
        }

        fn add_consumer_systems(app: &mut App) {
            app.add_message::<vmux_layout::BrowserNavigateRequest>()
                .add_message::<vmux_layout::BrowserGoBackRequest>()
                .add_message::<vmux_layout::BrowserGoForwardRequest>()
                .add_message::<vmux_layout::OpenInNewStackRequest>()
                .add_message::<PageOpenRequest>()
                .add_message::<CefPageAttachRequest>()
                .add_message::<vmux_layout::reconcile::LayoutApplyRequest>()
                .add_message::<vmux_layout::reconcile::LayoutApplyResponse>()
                .add_message::<vmux_layout::reconcile::LayoutSnapshotRequest>()
                .add_message::<vmux_layout::reconcile::LayoutSnapshotResponse>()
                .add_message::<vmux_terminal::TerminalSendRequest>()
                .add_message::<vmux_terminal::RunShellRequest>()
                .add_message::<vmux_setting::SettingsWriteRequest>()
                .add_message::<vmux_history::query::HistoryOpenIntent>()
                .configure_sets(
                    Update,
                    (
                        PageOpenSet::ResolveTarget,
                        PageOpenSet::HandleKnownPages,
                        PageOpenSet::Fallback,
                        PageOpenSet::Respond,
                    )
                        .chain(),
                )
                .add_systems(
                    Update,
                    (
                        crate::handle_browser_navigate_requests,
                        crate::handle_page_open_requests.in_set(PageOpenSet::ResolveTarget),
                        handle_test_known_page_open.in_set(PageOpenSet::HandleKnownPages),
                        crate::attach_cef_page_requests.in_set(PageOpenSet::Fallback),
                        crate::handle_unclaimed_page_open_tasks.in_set(PageOpenSet::Fallback),
                        crate::respond_page_open_tasks.in_set(PageOpenSet::Respond),
                        vmux_terminal::handle_terminal_send_requests,
                        vmux_terminal::handle_run_shell_requests,
                    ),
                );
        }

        type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

        fn handle_test_known_page_open(
            tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
            children_q: Query<&Children>,
            mut commands: Commands,
        ) {
            for (entity, task) in &tasks {
                if task.url.starts_with("vmux://terminal/") {
                    crate::clear_stack_children(task.stack, &children_q, &mut commands);
                    commands.spawn((Terminal, ChildOf(task.stack)));
                    commands.entity(entity).insert(PageOpenHandled);
                } else if task.url.starts_with("vmux://agent/") {
                    crate::clear_stack_children(task.stack, &children_q, &mut commands);
                    commands.entity(entity).insert(PageOpenHandled);
                }
            }
        }

        #[derive(Resource, Default)]
        struct CapturedNavigateUrls(Vec<String>);

        #[test]
        fn browser_navigate_triggers_request_navigate_with_url() {
            use bevy_cef::prelude::RequestNavigate;
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin));
            add_consumer_systems(&mut app);
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>()
                .init_resource::<CapturedNavigateUrls>();

            let pane = app.world_mut().spawn(Pane).id();
            let stack = app
                .world_mut()
                .spawn(vmux_layout::stack::stack_bundle())
                .insert(ChildOf(pane))
                .id();
            app.world_mut().spawn(Browser).insert(ChildOf(stack));

            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
            app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);

            app.add_observer(
                |trigger: On<RequestNavigate>, mut captured: ResMut<CapturedNavigateUrls>| {
                    captured.0.push(trigger.url.clone());
                },
            );

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "https://example.com".to_string(),
                        pane: None,
                    },
                });

            app.update();
            app.update();

            let captured = app.world().resource::<CapturedNavigateUrls>();
            assert_eq!(captured.0, vec!["https://example.com".to_string()]);
        }

        #[test]
        fn browser_navigate_auto_spawns_tab_when_pane_is_empty() {
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin));
            add_consumer_systems(&mut app);
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>();

            let pane = app.world_mut().spawn(Pane).id();

            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
            app.world_mut().resource_mut::<FocusedStack>().stack = None;

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "https://example.com".to_string(),
                        pane: None,
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let mut tabs = world.query_filtered::<&ChildOf, With<vmux_layout::stack::Stack>>();
            let tab_count_under_pane = tabs
                .iter(world)
                .filter(|child_of| child_of.get() == pane)
                .count();
            assert_eq!(
                tab_count_under_pane, 1,
                "browser_navigate should have spawned exactly one tab in the focused pane"
            );

            let mut tab_metadata =
                world.query_filtered::<&PageMetadata, With<vmux_layout::stack::Stack>>();
            let tab_urls: Vec<String> = tab_metadata.iter(world).map(|p| p.url.clone()).collect();
            assert!(
                tab_urls.contains(&"https://example.com".to_string()),
                "tab entity should have PageMetadata with the URL; found {tab_urls:?}"
            );

            let mut browsers = world.query::<(&Browser, &PageMetadata)>();
            let urls: Vec<String> = browsers.iter(world).map(|(_, p)| p.url.clone()).collect();
            assert!(
                urls.contains(&"https://example.com".to_string()),
                "browser entity with the URL should exist; found {urls:?}"
            );
        }

        #[test]
        fn browser_navigate_targets_specific_pane_when_id_provided() {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin));
            add_consumer_systems(&mut app);
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>();

            let pane_a = app.world_mut().spawn(Pane).id();
            let pane_b = app.world_mut().spawn(Pane).id();

            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane_a);

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "https://example.com".to_string(),
                        pane: Some(pane_b.to_bits().to_string()),
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let mut tabs = world.query_filtered::<&ChildOf, With<vmux_layout::stack::Stack>>();
            let tabs_in_b = tabs
                .iter(world)
                .filter(|child_of| child_of.get() == pane_b)
                .count();
            let tabs_in_a = tabs
                .iter(world)
                .filter(|child_of| child_of.get() == pane_a)
                .count();
            assert_eq!(tabs_in_b, 1, "tab should be spawned in target pane B");
            assert_eq!(tabs_in_a, 0, "no tab should be spawned in focused pane A");
        }

        #[test]
        fn browser_navigate_with_terminal_url_spawns_terminal_in_focused_pane() {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin));
            add_consumer_systems(&mut app);
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>();

            let pane = app.world_mut().spawn(Pane).id();
            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "vmux://terminal/".to_string(),
                        pane: None,
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let terminal_count = world.query::<&Terminal>().iter(world).count();
            assert!(
                terminal_count >= 1,
                "terminal should be spawned in focused pane"
            );
        }

        #[test]
        fn browser_navigate_with_terminal_url_and_target_pane_uses_target() {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin));
            add_consumer_systems(&mut app);
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>();

            let pane_a = app.world_mut().spawn(Pane).id();
            let pane_b = app.world_mut().spawn(Pane).id();
            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane_a);

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "vmux://terminal/".to_string(),
                        pane: Some(pane_b.to_bits().to_string()),
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let mut terminals = world.query_filtered::<&ChildOf, With<Terminal>>();
            let term_parents: Vec<Entity> = terminals.iter(world).map(|c| c.get()).collect();
            let mut found_in_b = 0;
            let mut found_in_a = 0;
            for tab in &term_parents {
                if let Some(co) = world.get::<ChildOf>(*tab) {
                    if co.get() == pane_b {
                        found_in_b += 1;
                    } else if co.get() == pane_a {
                        found_in_a += 1;
                    }
                }
            }
            assert_eq!(found_in_b, 1, "terminal should be in target pane B");
            assert_eq!(found_in_a, 0, "no terminal in focused pane A");
        }

        #[test]
        fn browser_navigate_with_unknown_vmux_url_errors() {
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin));
            add_consumer_systems(&mut app);
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>();

            let pane = app.world_mut().spawn(Pane).id();
            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "vmux://nonsense/".to_string(),
                        pane: None,
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let mut browsers = world.query_filtered::<&PageMetadata, With<Browser>>();
            let browser_titles: Vec<String> = browsers
                .iter(world)
                .map(|meta| meta.title.clone())
                .collect();
            let terminal_count = world.query::<&Terminal>().iter(world).count();
            assert_eq!(
                browser_titles,
                vec!["Page not found".to_string()],
                "unknown vmux URL should render an error page"
            );
            assert_eq!(
                terminal_count, 0,
                "no terminal should be spawned for unknown vmux URL"
            );
        }

        #[test]
        fn page_open_error_renders_error_page() {
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin));
            add_consumer_systems(&mut app);
            app.insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>();

            let pane = app.world_mut().spawn(Pane).id();
            let stack = app
                .world_mut()
                .spawn((
                    vmux_layout::stack::stack_bundle(),
                    vmux_history::LastActivatedAt::now(),
                    ChildOf(pane),
                ))
                .id();

            app.world_mut().spawn((
                PageOpenTask {
                    id: PageOpenId::new(),
                    stack,
                    url: "vmux://terminal/bad".to_string(),
                    request_id: None,
                },
                PageOpenError {
                    message: "malformed terminal URL".to_string(),
                },
            ));

            app.update();
            app.update();

            let world = app.world_mut();
            let mut browsers = world.query_filtered::<&PageMetadata, With<Browser>>();
            let browser_titles: Vec<String> = browsers
                .iter(world)
                .map(|meta| meta.title.clone())
                .collect();
            assert_eq!(
                browser_titles,
                vec!["Page failed to load".to_string()],
                "page handler errors should render an error page"
            );
        }

        #[test]
        fn browser_navigate_with_claude_url_does_not_spawn_standalone_browser() {
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin));
            add_consumer_systems(&mut app);
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>();

            let pane = app.world_mut().spawn(Pane).id();
            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "vmux://agent/claude/cli/".into(),
                        pane: None,
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let standalone_browser_count = world
                .query_filtered::<&Browser, Without<Terminal>>()
                .iter(world)
                .count();
            assert_eq!(
                standalone_browser_count, 0,
                "claude URL should never spawn a standalone browser tab"
            );
        }

        #[test]
        fn browser_navigate_with_codex_url_does_not_spawn_standalone_browser() {
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin));
            add_consumer_systems(&mut app);
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>();

            let pane = app.world_mut().spawn(Pane).id();
            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "vmux://agent/codex/cli/".into(),
                        pane: None,
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let standalone_browser_count = world
                .query_filtered::<&Browser, Without<Terminal>>()
                .iter(world)
                .count();
            assert_eq!(
                standalone_browser_count, 0,
                "codex URL should never spawn a standalone browser tab"
            );
        }
    }

    mod open_in_place_flow {
        use bevy::ecs::message::Messages;
        use bevy::prelude::*;
        use bevy_cef::prelude::{RequestNavigate, WebviewExtendStandardMaterial};
        use vmux_command::open::OpenCommand;
        use vmux_command::{AppCommand, BrowserCommand};
        use vmux_core::{PageOpenRequest, PageOpenTarget};
        use vmux_history::LastActivatedAt;
        use vmux_layout::Browser;
        use vmux_layout::pane::Pane;
        use vmux_layout::stack::stack_bundle;
        use vmux_layout::tab::Tab;
        use vmux_terminal::Terminal;

        #[derive(Resource, Default)]
        struct CapturedNavigateUrls(Vec<String>);

        #[derive(Resource, Default)]
        struct CapturedPageOpenRequests(Vec<PageOpenRequest>);

        fn build_app() -> App {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin))
                .add_message::<PageOpenRequest>()
                .add_systems(
                    Update,
                    (
                        super::super::handle_browser_commands.in_set(vmux_command::ReadAppCommands),
                        capture_page_open_requests.after(vmux_command::ReadAppCommands),
                    ),
                )
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>()
                .init_resource::<CapturedNavigateUrls>()
                .init_resource::<CapturedPageOpenRequests>()
                .add_observer(
                    |trigger: On<RequestNavigate>, mut captured: ResMut<CapturedNavigateUrls>| {
                        captured.0.push(trigger.url.clone());
                    },
                );
            app
        }

        fn capture_page_open_requests(
            mut reader: MessageReader<PageOpenRequest>,
            mut captured: ResMut<CapturedPageOpenRequests>,
        ) {
            captured.0.extend(reader.read().cloned());
        }

        fn build_focused_stack(app: &mut App) {
            let space = app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt(1)))
                .id();
            let pane = app
                .world_mut()
                .spawn((Pane, LastActivatedAt(1), ChildOf(space)))
                .id();
            let stack = app
                .world_mut()
                .spawn(stack_bundle())
                .insert((ChildOf(pane), LastActivatedAt(1)))
                .id();
            app.world_mut().spawn(Browser).insert(ChildOf(stack));
        }

        fn build_focused_terminal_stack(app: &mut App) {
            let space = app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt(1)))
                .id();
            let pane = app
                .world_mut()
                .spawn((Pane, LastActivatedAt(1), ChildOf(space)))
                .id();
            let stack = app
                .world_mut()
                .spawn(stack_bundle())
                .insert((ChildOf(pane), LastActivatedAt(1)))
                .id();
            app.world_mut()
                .spawn((Browser, Terminal))
                .insert(ChildOf(stack));
        }

        fn build_focused_native_stack(app: &mut App, native_url: &str) {
            let space = app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt(1)))
                .id();
            let pane = app
                .world_mut()
                .spawn((Pane, LastActivatedAt(1), ChildOf(space)))
                .id();
            let stack = app
                .world_mut()
                .spawn(stack_bundle())
                .insert((ChildOf(pane), LastActivatedAt(1)))
                .id();
            app.world_mut()
                .spawn((
                    Browser,
                    vmux_core::PageMetadata {
                        url: native_url.to_string(),
                        title: native_url.to_string(),
                        favicon_url: String::new(),
                        bg_color: None,
                    },
                ))
                .insert(ChildOf(stack));
        }

        #[test]
        fn in_place_with_explicit_url_triggers_request_navigate() {
            let mut app = build_app();
            build_focused_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("https://example.com".into()),
                    },
                )));

            app.update();

            let captured = app.world().resource::<CapturedNavigateUrls>();
            assert_eq!(captured.0, vec!["https://example.com".to_string()]);
        }

        #[test]
        fn in_place_with_vmux_url_routes_through_page_open() {
            let mut app = build_app();
            build_focused_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("vmux://agent/vibe".into()),
                    },
                )));

            app.update();

            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert!(navigates.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert_eq!(page_opens.0.len(), 1);
            assert_eq!(page_opens.0[0].url, "vmux://agent/vibe");
            assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
        }

        #[test]
        fn in_place_from_native_view_to_web_routes_through_page_open() {
            let mut app = build_app();
            build_focused_native_stack(&mut app, "vmux://spaces/");

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("https://mistral.ai".into()),
                    },
                )));

            app.update();

            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert!(navigates.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert_eq!(page_opens.0.len(), 1);
            assert_eq!(page_opens.0[0].url, "https://mistral.ai");
            assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
        }

        #[test]
        fn in_place_from_terminal_to_web_routes_through_page_open() {
            let mut app = build_app();
            build_focused_terminal_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("https://google.com".into()),
                    },
                )));

            app.update();

            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert!(navigates.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert_eq!(page_opens.0.len(), 1);
            assert_eq!(page_opens.0[0].url, "https://google.com");
            assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
        }

        #[test]
        fn in_place_with_none_url_uses_startup_setting() {
            let mut app = build_app();
            app.insert_resource(vmux_layout::settings::EffectiveStartupUrl(
                "https://startup.example".into(),
            ));
            build_focused_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace { url: None },
                )));

            app.update();

            let captured = app.world().resource::<CapturedNavigateUrls>();
            assert_eq!(captured.0, vec!["https://startup.example".to_string()]);
        }

        #[test]
        fn in_place_with_none_url_and_no_startup_does_not_navigate() {
            let mut app = build_app();
            build_focused_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace { url: None },
                )));

            app.update();

            let captured = app.world().resource::<CapturedNavigateUrls>();
            assert!(captured.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert!(page_opens.0.is_empty());
        }
    }
}
