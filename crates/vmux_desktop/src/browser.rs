use bevy::{
    ecs::{message::Messages, relationship::Relationship},
    prelude::*,
    ui::{UiGlobalTransform, UiSystems},
    window::{PrimaryWindow, WindowResized},
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::{RenderTextureMessage, webview_debug_log};
use vmux_command::{
    AppCommand, BrowserBarCommand, BrowserCommand, BrowserNavigationCommand, BrowserViewCommand,
    ReadAppCommands, open::OpenCommand,
};
use vmux_core::PageMetadata;
use vmux_history::{CreatedAt, LastActivatedAt, Visit};
use vmux_layout::command_bar::handler::PendingCommandBarReveal;
use vmux_layout::event::SideSheetCommandEvent;
pub(crate) use vmux_layout::{Browser, Loading};
use vmux_layout::{
    Header, LayoutCef, NavigationState, Open, PendingWebviewReveal,
    event::{
        HEADER_HEIGHT_PX, HeaderCommandEvent, LAYOUT_STATE_EVENT, LayoutStateEvent,
        PANE_TREE_EVENT, PaneNode, PaneTreeEvent, RELOAD_EVENT, ReloadEvent, STACKS_EVENT,
        StackNode, StackRow, StacksHostEvent, TABS_EVENT, TabRow, TabsHostEvent,
    },
    pane::{Pane, PaneHoverIntent, PaneSplit, first_leaf_descendant, first_stack_in_pane},
    side_sheet::{SideSheet, SideSheetPosition, SideSheetWidth},
    space::Space,
    stack::{
        CloseConfirmed, PendingStackClose, Stack, active_stack_in_pane, collect_leaf_panes,
        focused_stack, stack_bundle,
    },
    window::{
        Modal, VmuxWindow, WEBVIEW_Z_HEADER, WEBVIEW_Z_MAIN, WEBVIEW_Z_MODAL, WEBVIEW_Z_SIDE_SHEET,
    },
};
use vmux_server::{PageReady, Server};
use vmux_setting::AppSettings;
use vmux_terminal::{self as terminal, PtyExited, RestartPty, Terminal};
use vmux_ui::theme::{THEME_EVENT, ThemeEvent};

pub(crate) struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        let embedded_hosts = app.world().resource::<Server>().embedded_hosts();
        webview_debug_log(format!("BrowserPlugin embedded_hosts={embedded_hosts:?}"));
        app.configure_sets(Update, CefSystems::CreateAndResize.after(ReadAppCommands))
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
                (
                    handle_browser_commands.in_set(ReadAppCommands),
                    vmux_layout::apply_chrome_state_from_cef,
                    drain_loading_state,
                    spawn_popup_tabs,
                    handle_browser_navigate_requests.after(vmux_terminal::ServiceMessageSet),
                ),
            )
            .add_systems(
                Update,
                (sync_page_metadata_to_tab, spawn_visit_on_navigation)
                    .chain()
                    .after(vmux_layout::apply_chrome_state_from_cef),
            )
            .add_systems(
                Update,
                (
                    push_layout_state_emit,
                    push_stacks_host_emit,
                    push_pane_tree_emit,
                    push_tabs_host_emit,
                )
                    .after(vmux_layout::apply_chrome_state_from_cef)
                    .after(vmux_layout::stack::ComputeFocusSet),
            )
            .add_systems(
                PostUpdate,
                (
                    sync_keyboard_target,
                    sync_children_to_ui,
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
    // Chrome / modal must never carry a stale zoom (e.g. from a previous
    // session where pinch-zoom was allowed); force them to 0 once the
    // webview is ready, both on the component and on the CEF host.
    if cef_q.get(entity).is_ok() || modal_q.get(entity).is_ok() {
        if let Ok(mut zoom) = zoom_q.get_mut(entity) {
            zoom.0 = 0.0;
        }
        browsers.set_zoom_level(&entity, 0.0);
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

fn sync_children_to_ui(
    mut browser_q: Query<
        (
            &mut Transform,
            &ComputedNode,
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
    new_stack_ctx: Res<vmux_layout::NewStackContext>,
    glass: Single<(Entity, &ComputedNode, &UiGlobalTransform), With<VmuxWindow>>,
) {
    let &(glass_entity, glass_node, glass_ui_gt) = &*glass;
    let pad = glass_node.padding;
    let glass_size_px = glass_node.size + pad.min_inset + pad.max_inset;

    for (
        mut tf,
        self_computed,
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

        let is_chrome = status.is_some() || side_sheet.is_some() || modal.is_some();

        // Check if this browser's parent tab is the active tab in its pane
        let is_active_stack = if parent != glass_entity && !is_chrome {
            active_stack_in_pane(pane_entity, &pane_children, &tab_ts) == Some(parent)
        } else {
            true
        };

        // Keep rendering the previous tab behind while a new empty tab
        // (without CEF content) is pending in the command bar flow.
        let is_previous_stack =
            new_stack_ctx.stack.is_some() && new_stack_ctx.previous_stack == Some(parent);

        let is_inactive_stack =
            parent != glass_entity && !is_chrome && !is_active_stack && !is_previous_stack;

        let sx = size_px.x / glass_size_px.x;
        let sy = size_px.y / glass_size_px.y;
        let new_scale = if is_inactive_stack {
            Vec3::splat(1e-6)
        } else {
            Vec3::new(sx, sy, 1.0)
        };
        if parent != glass_entity && !is_chrome && (tf.scale - new_scale).length() > 0.01 {
            info!(
                "[ui] browser child_of={:?} scale {:?} -> {:?} (inactive={})",
                parent, tf.scale, new_scale, is_inactive_stack
            );
        }
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
            0.01 + self_computed.stack_index as f32 * 0.001
        };
        let history_swipe_tx = if parent != glass_entity && !is_chrome {
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

fn sync_cef_webview_resize_after_ui(
    browsers: NonSend<Browsers>,
    webviews: Query<(Entity, &WebviewSize), With<Browser>>,
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

/// Walks up from a browser entity to find its enclosing Space, then counts
/// leaf panes under that space. Returns None if the parent chain doesn't
/// reach a Space.
fn pane_count_for_browser(
    browser_e: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<(), With<Space>>,
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
    tab_q: Query<(), With<Space>>,
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
        // card) or when the chrome is hidden (no url row above to merge
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
        if let Some(mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, mode);
        }
    }
    for (size, mat_h) in &status {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 0.0);
        }
    }
    for (size, mat_h) in &side_sheet {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mat) = materials.get_mut(mat_h.id()) {
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
    let window_focused = primary_window.focused;

    let active_stack_opt = focus.stack;
    let active_stack = active_stack_opt.and_then(|tab| {
        ready
            .iter()
            .copied()
            .find(|&b| child_of_q.get(b).ok().map(|co| co.get()) == Some(tab))
    });
    let active = choose_osr_active_webview(modal_keyboard_target, active_stack, ready[0]);

    if !window_focused {
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
            window_focused,
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
    _window_focused: bool,
    parent_is_stack: bool,
    pane_is_leaf: bool,
    stack_is_active: bool,
    stack_is_previous_new_stack: bool,
) -> bool {
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

fn spawn_popup_tabs(
    popup_rx: Res<WebviewPopupReceiver>,
    child_of_q: Query<&ChildOf>,
    tab_q: Query<(), With<Stack>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    while let Ok(ev) = popup_rx.0.try_recv() {
        if ev.target_url.is_empty() {
            continue;
        }
        let Ok(tab_co) = child_of_q.get(ev.webview) else {
            continue;
        };
        let tab = tab_co.get();
        if !tab_q.contains(tab) {
            continue;
        }
        let Ok(pane_co) = child_of_q.get(tab) else {
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
    tab_q: Query<(), With<Space>>,
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
    tabs: Query<(Entity, &Space, &LastActivatedAt)>,
    tab_q: Query<Entity, With<Space>>,
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
        vmux_layout::space::active_space_siblings(any.0, &child_of_q, &all_children, &tab_q)
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
    tabs: Query<(Entity, &LastActivatedAt), With<Space>>,
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
                    if is_terminal {
                        commands
                            .entity(webview)
                            .remove::<Terminal>()
                            .remove::<vmux_service::protocol::ProcessId>()
                            .remove::<vmux_agent::components::AgentSession>();
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
        "focus_address_bar" => BrowserCommand::Bar(BrowserBarCommand::OpenCommandBar),
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
    tab_query: Query<(Entity, &LastActivatedAt), With<Space>>,
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
    tab_q: Query<
        (
            Option<&PageMetadata>,
            Has<vmux_agent::components::AgentSession>,
        ),
        With<Stack>,
    >,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    mut commands: Commands,
) {
    for (meta, child_of) in &browser_q {
        let parent = child_of.get();
        let Ok((parent_meta, is_agent_session)) = tab_q.get(parent) else {
            continue;
        };
        if status_q.contains(parent) || side_sheet_q.contains(parent) {
            continue;
        }
        if is_agent_session {
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

pub(crate) fn handle_browser_navigate_requests(
    mut reader: MessageReader<vmux_layout::BrowserNavigateRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<terminal::ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    child_of_q: Query<&ChildOf>,
    lookups: vmux_agent::plugin::AgentLookups,
    strategies: Res<vmux_agent::strategy::AgentStrategies>,
    settings: Res<AppSettings>,
    service: Option<Res<vmux_service::client::ServiceClient>>,
    page_idx: Option<Res<vmux_agent::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&vmux_agent::client::page::strategy_components::StrategyKind>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    use vmux_service::protocol::{AgentCommandResult, ClientMessage};
    let pid_to_entity = lookups.pid_to_entity.as_deref();
    let agent_to_entity = lookups.agent_to_entity.as_deref();

    for request in reader.read() {
        let vmux_layout::BrowserNavigateRequest { url, pane } = request.clone();

        let result = if url.starts_with("vmux://") {
            let target = match pane.as_deref() {
                Some(s) => match vmux_layout::target::parse_pane_target(s, &panes) {
                    Some(t) => Some(t),
                    None => {
                        if let Some(service) = service.as_ref() {
                            service.0.send(ClientMessage::AgentCommandResponse {
                                request_id: vmux_service::protocol::AgentRequestId::new(),
                                result: AgentCommandResult::Error(format!(
                                    "browser_navigate: invalid pane id '{s}'"
                                )),
                            });
                        }
                        continue;
                    }
                },
                None => focus.pane.filter(|p| panes.contains(*p)),
            };

            if let Some(pane_entity) = target {
                let empty_idx =
                    vmux_agent::client::page::strategy_index::PageStrategyIndex::default();
                let idx_ref = page_idx.as_deref().unwrap_or(&empty_idx);
                match vmux_agent::plugin::spawn_vmux_tab(
                    &url,
                    pane_entity,
                    &mut commands,
                    &mut meshes,
                    &mut webview_mt,
                    &settings,
                    pid_to_entity,
                    agent_to_entity,
                    &strategies,
                    &child_of_q,
                    idx_ref,
                    &kind_q,
                ) {
                    Ok(()) => AgentCommandResult::Ok,
                    Err(message) => {
                        AgentCommandResult::Error(format!("browser_navigate: {message}"))
                    }
                }
            } else {
                AgentCommandResult::Error(
                    "browser_navigate: no focused pane for vmux URL".to_string(),
                )
            }
        } else if let Some(s) = pane.as_deref() {
            if let Some(target) = vmux_layout::target::parse_pane_target(s, &panes) {
                vmux_agent::plugin::spawn_browser_tab(
                    target,
                    &url,
                    &mut commands,
                    &mut meshes,
                    &mut webview_mt,
                );
                AgentCommandResult::Ok
            } else {
                AgentCommandResult::Error(format!("browser_navigate: invalid pane id '{s}'"))
            }
        } else if let Some(webview) =
            vmux_layout::target::active_webview_for_tab(focus.stack, &browsers, &terminals)
        {
            commands.trigger(RequestNavigate {
                webview,
                url: url.clone(),
            });
            AgentCommandResult::Ok
        } else if let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) {
            vmux_agent::plugin::spawn_browser_tab(
                pane,
                &url,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            AgentCommandResult::Ok
        } else {
            AgentCommandResult::Error("browser_navigate: no focused pane".to_string())
        };
        let _ = result;
    }
}

fn cef_root_cache_path() -> Option<String> {
    vmux_core::profile::cef_cache_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osr_webview_stays_visible_when_window_is_unfocused() {
        assert!(!should_show_osr_webview(true, true, true, false, false));
        assert!(should_show_osr_webview(true, true, true, true, false));
        assert!(should_show_osr_webview(false, true, true, true, false));
    }

    #[test]
    fn auxiliary_osr_webviews_remain_visible_when_window_is_focused() {
        assert!(should_show_osr_webview(true, false, true, false, false));
        assert!(should_show_osr_webview(true, true, false, false, false));
        assert!(should_show_osr_webview(true, true, true, false, true));
        assert!(should_show_osr_webview(false, false, true, false, false));
        assert!(should_show_osr_webview(false, true, false, false, false));
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
        use vmux_core::PageMetadata;
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
                startup_url: None,
                agent: vmux_setting::AgentSettings::default(),
            }
        }

        fn add_consumer_systems(app: &mut App) {
            app.add_message::<vmux_layout::BrowserNavigateRequest>();
            app.add_message::<vmux_layout::reconcile::LayoutApplyRequest>();
            app.add_message::<vmux_layout::reconcile::LayoutApplyResponse>();
            app.add_message::<vmux_layout::reconcile::LayoutSnapshotRequest>();
            app.add_message::<vmux_layout::reconcile::LayoutSnapshotResponse>();
            app.add_message::<vmux_terminal::TerminalSendRequest>();
            app.add_message::<vmux_terminal::RunShellRequest>();
            app.add_message::<vmux_setting::SettingsWriteRequest>();
            app.add_systems(
                Update,
                (
                    crate::browser::handle_browser_navigate_requests,
                    vmux_terminal::handle_terminal_send_requests,
                    vmux_terminal::handle_run_shell_requests,
                ),
            );
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
            app.init_resource::<AgentStrategies>();
            app.insert_resource(FocusedStack::default());
            app.insert_resource(test_settings());
            app.init_resource::<Assets<Mesh>>();
            app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
            app.init_resource::<CapturedNavigateUrls>();

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
            app.init_resource::<AgentStrategies>();
            app.insert_resource(FocusedStack::default());
            app.insert_resource(test_settings());
            app.init_resource::<Assets<Mesh>>();
            app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

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
            app.init_resource::<AgentStrategies>();
            app.insert_resource(FocusedStack::default());
            app.insert_resource(test_settings());
            app.init_resource::<Assets<Mesh>>();
            app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

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
            app.init_resource::<AgentStrategies>();
            app.insert_resource(FocusedStack::default());
            app.insert_resource(test_settings());
            app.init_resource::<Assets<Mesh>>();
            app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

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
            app.init_resource::<AgentStrategies>();
            app.insert_resource(FocusedStack::default());
            app.insert_resource(test_settings());
            app.init_resource::<Assets<Mesh>>();
            app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

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
            app.init_resource::<AgentStrategies>();
            app.insert_resource(FocusedStack::default());
            app.insert_resource(test_settings());
            app.init_resource::<Assets<Mesh>>();
            app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

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
            let browser_count = world.query::<&Browser>().iter(world).count();
            let terminal_count = world.query::<&Terminal>().iter(world).count();
            assert_eq!(
                browser_count, 0,
                "no browser should be spawned for unknown vmux URL"
            );
            assert_eq!(
                terminal_count, 0,
                "no terminal should be spawned for unknown vmux URL"
            );
        }

        #[test]
        fn browser_navigate_with_claude_url_does_not_spawn_standalone_browser() {
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, AgentPlugin));
            add_consumer_systems(&mut app);
            app.init_resource::<AgentStrategies>();
            app.insert_resource(FocusedStack::default());
            app.insert_resource(test_settings());
            app.init_resource::<Assets<Mesh>>();
            app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

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
            app.init_resource::<AgentStrategies>();
            app.insert_resource(FocusedStack::default());
            app.insert_resource(test_settings());
            app.init_resource::<Assets<Mesh>>();
            app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

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
        use vmux_history::LastActivatedAt;
        use vmux_layout::Browser;
        use vmux_layout::pane::Pane;
        use vmux_layout::space::Space;
        use vmux_layout::stack::stack_bundle;

        #[derive(Resource, Default)]
        struct CapturedNavigateUrls(Vec<String>);

        fn build_app() -> App {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin));
            app.add_systems(
                Update,
                super::super::handle_browser_commands.in_set(vmux_command::ReadAppCommands),
            );
            app.init_resource::<Assets<Mesh>>();
            app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
            app.init_resource::<CapturedNavigateUrls>();
            app.add_observer(
                |trigger: On<RequestNavigate>, mut captured: ResMut<CapturedNavigateUrls>| {
                    captured.0.push(trigger.url.clone());
                },
            );
            app
        }

        fn spawn_focused_stack(app: &mut App) {
            let space = app
                .world_mut()
                .spawn((Space::default(), LastActivatedAt(1)))
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

        #[test]
        fn in_place_with_explicit_url_triggers_request_navigate() {
            let mut app = build_app();
            spawn_focused_stack(&mut app);

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
        fn in_place_with_none_url_uses_startup_setting() {
            let mut app = build_app();
            app.insert_resource(vmux_layout::settings::EffectiveStartupUrl(
                "https://startup.example".into(),
            ));
            spawn_focused_stack(&mut app);

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
        fn in_place_with_none_url_and_no_startup_uses_default() {
            let mut app = build_app();
            spawn_focused_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace { url: None },
                )));

            app.update();

            let captured = app.world().resource::<CapturedNavigateUrls>();
            assert_eq!(
                captured.0,
                vec![vmux_command::open::handler::DEFAULT_NEW_PAGE_URL.to_string()]
            );
        }
    }
}
