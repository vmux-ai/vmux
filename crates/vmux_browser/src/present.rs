//! Making the webviews match the layout Bevy just computed.
//!
//! Everything here runs in one chain between `UiSystems::Layout` and the standard material
//! render: geometry, visibility, focus and the native frames of windowed webviews all read the
//! same finished layout, so the order is load-bearing rather than incidental.

use bevy::{
    ecs::relationship::Relationship,
    material::AlphaMode,
    prelude::*,
    ui::{UiGlobalTransform, UiSystems},
    window::{PrimaryWindow, WindowResized},
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::{RenderTextureMessage, webview_debug_log};
use std::sync::atomic::Ordering;
use vmux_core::page::PageReady;
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::command_bar::handler::{CommandBarNativeSize, PendingCommandBarReveal};
use vmux_layout::command_bar::state::CommandBarState;
use vmux_layout::{
    Header, LayoutCef, Open, PendingWebviewReveal,
    bookmark::{BookmarkContextMenuActive, BookmarkTextInputActive},
    command_bar::panel::CommandBarPanelActive,
    pane::{Pane, PaneSplit},
    side_sheet::SideSheet,
    stack::{Stack, active_stack_in_pane, collect_leaf_panes},
    tab::Tab,
    window::{
        Modal, VmuxWindow, WEBVIEW_Z_HEADER, WEBVIEW_Z_MAIN, WEBVIEW_Z_MODAL, WEBVIEW_Z_SIDE_SHEET,
    },
};

use vmux_setting::AppSettings;

use crate::{
    CLAUDE_LOGO_PNG, CODEX_LOGO_PNG, CommandBarRoute, LogoBitmap,
    NATIVE_COMMAND_BAR_DISMISS_REQUESTED, NATIVE_COMMAND_BAR_ROUTE, VIBE_LOGO_PNG, agent_ring_rgb,
    decode_premultiplied, hex_to_rgb,
};

#[cfg(target_os = "macos")]
use crate::native_bridge::CommandBarPointerEvent;
use crate::native_bridge::NativeBridge;

pub(crate) struct PresentPlugin;

impl Plugin for PresentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (
                sync_keyboard_target,
                sync_windowed_content_mesh_materials,
                sync_modal_mesh_visibility,
                sync_children_to_ui,
                sync_windowed_layout,
                sync_windowed_frames,
                sync_windowed_command_bar,
                flush_native_command_bar_pointer_events,
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

/// The layout page owns the keyboard whenever one of its own DOM surfaces has focus — the bookmark
/// field, a context menu, or the command bar panel. One rule rather than a special case per
/// surface, since every consumer has to agree or the keyboard lands somewhere else.
pub(crate) type LayoutKeyboardCapture = Or<(
    With<BookmarkTextInputActive>,
    With<BookmarkContextMenuActive>,
    With<CommandBarPanelActive>,
)>;
fn sync_keyboard_target(
    mode: Res<vmux_layout::scene::InteractionMode>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    child_of_q: Query<&ChildOf>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    modal_q: Query<(Entity, &Node, Has<CefKeyboardTarget>), With<Modal>>,
    layout_keyboard_q: Query<Entity, (With<LayoutCef>, LayoutKeyboardCapture)>,
    content_q: Query<(Entity, Has<CefKeyboardTarget>), With<Browser>>,
    terminal_q: Query<(), With<vmux_terminal::Terminal>>,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
    mut commands: Commands,
) {
    if let Some(modal) = modal_q.iter().find_map(|(entity, node, keyboard_target)| {
        (node.display != Display::None && keyboard_target).then_some(entity)
    }) {
        for (browser_e, has_kb) in &content_q {
            if browser_e != modal && has_kb {
                commands.entity(browser_e).try_remove::<CefKeyboardTarget>();
            }
        }
        suppress.0 = false;
        return;
    }

    if let Ok(layout) = layout_keyboard_q.single() {
        for (browser_e, has_kb) in &content_q {
            if browser_e == layout {
                if !has_kb {
                    commands.entity(browser_e).try_insert(CefKeyboardTarget);
                }
            } else if has_kb {
                commands.entity(browser_e).try_remove::<CefKeyboardTarget>();
            }
        }
        suppress.0 = false;
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
                commands.entity(browser_e).try_insert(CefKeyboardTarget);
            }
            // Suppress CEF keyboard forwarding when a terminal is focused —
            // terminals receive input via the service, not CEF key events.
            suppress.0 = terminal_q.contains(browser_e);
        } else if has_kb {
            commands.entity(browser_e).try_remove::<CefKeyboardTarget>();
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
            Has<LayoutCef>,
            Has<WebviewWindowed>,
        ),
        With<Browser>,
    >,
    child_of_q: Query<&ChildOf>,
    pane_rect: Query<(&ComputedNode, &UiGlobalTransform), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    tabs_q: Query<(Entity, &LastActivatedAt), With<Tab>>,
    active_tab_q: Query<(), (With<Tab>, With<vmux_core::Active>)>,
    new_stack_ctx: Res<vmux_layout::NewStackContext>,
    glass: Single<(Entity, &ComputedNode, &UiGlobalTransform), With<VmuxWindow>>,
) {
    let &(glass_entity, glass_node, glass_ui_gt) = &*glass;
    let pad = glass_node.padding;
    let glass_size_px = glass_node.size + pad.min_inset + pad.max_inset;

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
        is_layout_cef,
        is_windowed,
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

        let is_cef_ui = status.is_some() || side_sheet.is_some() || modal.is_some();

        let under_inactive_tab = parent != glass_entity
            && !is_cef_ui
            && match tab_ancestor(parent, &child_of_q, &tabs_q) {
                Some(tab) => !active_tab_q.contains(tab),
                None => false,
            };

        let size_px = computed.size;
        let renderable = webview_layout_is_renderable(
            size_px,
            visibility,
            pending_webview_reveal || pending_command_bar_reveal,
        );
        match hidden_webview_sizing(renderable, under_inactive_tab) {
            HiddenWebviewSizing::Render => {}
            HiddenWebviewSizing::HideKeepSize => {
                tf.scale = Vec3::splat(1.0e-6);
                continue;
            }
            HiddenWebviewSizing::Collapse => {
                tf.scale = Vec3::splat(1.0e-6);
                if webview_size.0 != Vec2::ONE {
                    webview_size.0 = Vec2::ONE;
                }
                continue;
            }
        }

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

        let is_inactive_tab = under_inactive_tab;

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
        } else if is_layout_cef || status.is_some() {
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

        // A windowed modal's node fills the whole layout area, but its native view is a small
        // centred box that `sync_windowed_command_bar` sizes. Writing the node size here makes the
        // page lay out at the full width inside that box, so the shell renders far too wide.
        if modal.is_some() && is_windowed {
            continue;
        }

        let dip = (size_px * computed.inverse_scale_factor).max(Vec2::splat(1.0));
        if webview_size.0 != dip {
            webview_size.0 = dip;
        }
    }
}
fn set_windowed_content_mesh_material(
    material: &mut WebviewExtendStandardMaterial,
    windowed: bool,
) {
    let alpha = if windowed { 0.0 } else { 1.0 };
    material.base.base_color = material.base.base_color.with_alpha(alpha);
    material.base.alpha_mode =
        webview_content_alpha_mode(alpha, material.extension.pane_corner_clip.x);
}
fn webview_content_alpha_mode(alpha: f32, radius: f32) -> AlphaMode {
    if alpha < 1.0 {
        AlphaMode::Blend
    } else if radius > 0.0 {
        AlphaMode::AlphaToCoverage
    } else {
        AlphaMode::Opaque
    }
}
fn sync_windowed_content_mesh_materials(
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    browsers: Query<
        (
            &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
            Has<WebviewWindowed>,
        ),
        (
            With<Browser>,
            Without<LayoutCef>,
            Without<Modal>,
            Without<Header>,
            Without<SideSheet>,
        ),
    >,
) {
    for (handle, windowed) in &browsers {
        if let Some(mut material) = materials.get_mut(handle.id()) {
            set_windowed_content_mesh_material(&mut material, windowed);
        }
    }
}
fn sync_modal_mesh_visibility(
    modal_q: Query<
        (
            &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
            Has<WebviewWindowed>,
        ),
        With<Modal>,
    >,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for (handle, windowed) in &modal_q {
        if let Some(mut material) = materials.get_mut(handle.id()) {
            set_windowed_content_mesh_material(&mut material, windowed);
        }
    }
}
/// Pick the focus-ring width + color for a windowed browser pane. The local
/// user's ring (their accent) draws on their focused stack; each agent's ring
/// (a distinct per-agent hue) draws on the agent's own active pane. User takes
/// precedence when a pane is active for both.
fn windowed_ring_for(
    stack: Entity,
    pane: Entity,
    focus: &vmux_layout::stack::FocusedStack,
    visible_pane_count: usize,
    active_panes: &vmux_layout::active_panes::ActivePanes,
    settings: &AppSettings,
    scale: f32,
) -> (f32, [f32; 3], Option<vmux_core::agent::AgentKind>) {
    use vmux_layout::active_panes::ProfileId;
    let width = settings.layout.focus_ring.width * scale;
    let user = &settings.layout.focus_ring.color;
    if focus.stack == Some(stack) && visible_pane_count > 1 {
        return (width, [user.r, user.g, user.b], None);
    }
    for (profile, active) in active_panes.0.iter() {
        if let ProfileId::Agent(key) = profile
            && active.pane == Some(pane)
        {
            return (width, agent_ring_rgb(key), active.kind);
        }
    }
    (0.0, [user.r, user.g, user.b], None)
}
/// The agent's logo bitmap, decoded once and cached for the process lifetime.
fn agent_logo(kind: vmux_core::agent::AgentKind) -> Option<&'static LogoBitmap> {
    use std::sync::OnceLock;
    use vmux_core::agent::AgentKind;
    static CLAUDE: OnceLock<Option<LogoBitmap>> = OnceLock::new();
    static CODEX: OnceLock<Option<LogoBitmap>> = OnceLock::new();
    static VIBE: OnceLock<Option<LogoBitmap>> = OnceLock::new();
    let (cell, png) = match kind {
        AgentKind::Claude => (&CLAUDE, CLAUDE_LOGO_PNG),
        AgentKind::Codex => (&CODEX, CODEX_LOGO_PNG),
        AgentKind::Vibe => (&VIBE, VIBE_LOGO_PNG),
    };
    cell.get_or_init(|| decode_premultiplied(png)).as_ref()
}
/// Stable per-kind tag the native layer caches on, so the badge image is only
/// rebuilt when the owning agent's kind changes.
fn agent_kind_tag(kind: vmux_core::agent::AgentKind) -> u8 {
    use vmux_core::agent::AgentKind;
    match kind {
        AgentKind::Claude => 1,
        AgentKind::Codex => 2,
        AgentKind::Vibe => 3,
    }
}
/// The agent's brand color (Claude clay / Codex green / Mistral purple), used as
/// the badge circle fill behind its logo.
fn agent_brand_rgb(kind: vmux_core::agent::AgentKind) -> [f32; 3] {
    hex_to_rgb(&vmux_core::team::AvatarSpec::for_agent(kind).color).unwrap_or([0.5, 0.5, 0.5])
}
/// Position windowed (native) content webviews to match their pane rect. Reads the mesh scale set
/// by `sync_children_to_ui` (visible active pane has a real scale; inactive panes ~1e-6) to pick
/// which native view to show. No-op for OSR webviews / non-macOS (`set_windowed_*` are no-ops).
pub(crate) fn sync_windowed_frames(
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    layout_hidden: Res<vmux_layout::toggle::LayoutHidden>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    active_panes: Res<vmux_layout::active_panes::ActivePanes>,
    clear_color: Res<ClearColor>,
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
    header_rect: Query<(&ComputedNode, &UiGlobalTransform), (With<Header>, With<Open>)>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut last_raised_frame: Local<std::collections::HashMap<Entity, (i32, i32, i32, i32)>>,
    mut last_visible_pages: Local<Vec<Entity>>,
    mut last_windowed_pages: Local<Vec<Entity>>,
    mut visible_frames: Local<Vec<WindowedFrameRect>>,
) {
    let visible_pane_count =
        visible_pane_count_for_windowed_sync(focus.tab, &all_children, &leaf_panes);
    let header_frame = header_rect
        .iter()
        .find_map(|(computed, ui_gt)| windowed_frame_rect_from_computed(computed, ui_gt));
    let force_raise = layout_hidden.is_changed();
    let mut hidden = Vec::new();
    let mut visible = Vec::new();
    visible_frames.clear();
    for (entity, tf, self_computed, self_ui_gt, child_of) in &browser_q {
        if tf.scale.x <= 1.0e-3 {
            hidden.push(entity);
            continue;
        }
        visible.push(entity);
        let parent = child_of.get();
        let pane_entity = child_of_q.get(parent).map(|co| co.get()).unwrap_or(parent);
        let (computed, ui_gt) = pane_rect
            .get(pane_entity)
            .unwrap_or((self_computed, self_ui_gt));
        let Some(pane_frame) = windowed_frame_rect_from_computed(computed, ui_gt) else {
            continue;
        };
        let scale = 1.0 / computed.inverse_scale_factor.max(1.0e-6);
        let frame = windowed_page_frame_rect(
            pane_frame,
            header_frame,
            layout_hidden.0,
            visible_pane_count,
        );
        let became_visible = !last_visible_pages.contains(&entity);
        if became_visible {
            browsers.set_windowed_hidden(&entity, false);
        }
        browsers.set_windowed_frame(
            &entity,
            frame.left,
            frame.top,
            frame.width,
            frame.height,
            scale,
        );
        let all_corners = windowed_page_all_corners(layout_hidden.0, visible_pane_count);
        browsers.set_windowed_corner_radius(
            &entity,
            settings.layout.radius * scale,
            scale,
            all_corners,
        );
        let (focus_ring_width, focus_ring_rgb, focus_ring_kind) = windowed_ring_for(
            parent,
            pane_entity,
            &focus,
            visible_pane_count,
            &active_panes,
            &settings,
            scale,
        );
        browsers.set_windowed_focus_ring(&entity, focus_ring_width, scale, focus_ring_rgb);
        let badge = focus_ring_kind.and_then(|kind| {
            agent_logo(kind).map(|logo| {
                (
                    logo.rgba.as_slice(),
                    logo.width,
                    logo.height,
                    agent_brand_rgb(kind),
                    agent_kind_tag(kind),
                )
            })
        });
        browsers.set_agent_badge(&entity, scale, badge);
        let cover_rgb = clear_color.0.to_srgba();
        browsers.set_windowed_corner_cover(
            &entity,
            settings.layout.radius * scale,
            scale,
            all_corners,
            [cover_rgb.red, cover_rgb.green, cover_rgb.blue],
        );
        if browsers.has_browser(entity) {
            visible_frames.push(frame);
            let key = (
                frame.left.round() as i32,
                frame.top.round() as i32,
                frame.width.round() as i32,
                frame.height.round() as i32,
            );
            let changed = last_raised_frame.insert(entity, key) != Some(key);
            if force_raise || changed || became_visible {
                browsers.raise_windowed_to_front(&entity);
            }
        }
    }
    let current_windowed: Vec<Entity> = visible.iter().chain(&hidden).copied().collect();
    let newly_windowed: Vec<Entity> = current_windowed
        .iter()
        .copied()
        .filter(|entity| !last_windowed_pages.contains(entity))
        .collect();
    let ever_shown: Vec<Entity> = last_raised_frame.keys().copied().collect();
    for entity in windowed_pages_to_hide(&hidden, &last_visible_pages, &ever_shown, &newly_windowed)
    {
        browsers.set_windowed_hidden(&entity, true);
    }
    *last_visible_pages = visible;
    *last_windowed_pages = current_windowed;
    *visible_frames = NativeBridge::set_windowed_page_frames(std::mem::take(&mut *visible_frames));
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowedFrameRect {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}
impl WindowedFrameRect {
    pub(crate) fn right(self) -> f32 {
        self.left + self.width
    }

    pub(crate) fn bottom(self) -> f32 {
        self.top + self.height
    }
}
fn windowed_frame_rect_from_computed(
    computed: &ComputedNode,
    ui_gt: &UiGlobalTransform,
) -> Option<WindowedFrameRect> {
    let size = computed.size;
    if size.x <= 0.0 || size.y <= 0.0 || !size.x.is_finite() || !size.y.is_finite() {
        return None;
    }
    let center = ui_gt.transform_point2(Vec2::ZERO);
    Some(WindowedFrameRect {
        left: center.x - size.x * 0.5,
        top: center.y - size.y * 0.5,
        width: size.x,
        height: size.y,
    })
}
fn windowed_page_frame_rect(
    pane: WindowedFrameRect,
    header: Option<WindowedFrameRect>,
    layout_hidden: bool,
    visible_pane_count: usize,
) -> WindowedFrameRect {
    let Some(header) = header else {
        return pane;
    };
    if layout_hidden {
        return pane;
    }
    let (left, right) = if visible_pane_count == 1 {
        (header.left.ceil(), header.right().floor())
    } else {
        (pane.left.ceil(), pane.right().floor())
    };
    let top = header.bottom().ceil().max(pane.top.ceil());
    let bottom = pane.bottom().floor();
    if right <= left || bottom <= top {
        return pane;
    }
    WindowedFrameRect {
        left,
        top,
        width: right - left,
        height: bottom - top,
    }
}
fn visible_pane_count_for_windowed_sync(
    focused_tab: Option<Entity>,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> usize {
    if let Some(tab) = focused_tab {
        let mut leaves = Vec::new();
        collect_leaf_panes(tab, all_children, leaf_panes, &mut leaves);
        if !leaves.is_empty() {
            return leaves.len();
        }
    }
    leaf_panes.iter().count().max(1)
}
fn windowed_pages_to_hide(
    hidden: &[Entity],
    prev_visible: &[Entity],
    ever_shown: &[Entity],
    newly_windowed: &[Entity],
) -> Vec<Entity> {
    hidden
        .iter()
        .copied()
        .filter(|entity| {
            prev_visible.contains(entity)
                || !ever_shown.contains(entity)
                || newly_windowed.contains(entity)
        })
        .collect()
}
fn windowed_page_all_corners(layout_hidden: bool, visible_pane_count: usize) -> bool {
    layout_hidden || visible_pane_count > 1
}
fn sync_windowed_layout(
    browsers: NonSend<Browsers>,
    layout_q: Query<(Entity, Option<&HostWindow>), (With<LayoutCef>, With<WebviewWindowed>)>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    mut last_raised_frame: Local<std::collections::HashMap<Entity, (i32, i32, i32, i32)>>,
) {
    for (entity, host_window) in &layout_q {
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
        if browsers.has_browser(entity) {
            let key = (0, 0, w.round() as i32, h.round() as i32);
            let changed = last_raised_frame.insert(entity, key) != Some(key);
            if changed {
                browsers.raise_windowed_to_front(&entity);
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CommandBarWindowedFrame {
    pub(crate) left_px: f32,
    pub(crate) top_px: f32,
    pub(crate) width_px: f32,
    pub(crate) height_px: f32,
}
const COMMAND_BAR_NATIVE_RADIUS_PX: f32 = 16.0;
fn publish_native_command_bar_route(
    owns_input: bool,
    frame: Option<CommandBarWindowedFrame>,
    scale: f32,
) {
    let mut stored = NATIVE_COMMAND_BAR_ROUTE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let generation = stored.generation.wrapping_add(1);
    *stored = CommandBarRoute {
        generation,
        owns_input,
        frame,
        scale,
    };
    if !owns_input {
        NATIVE_COMMAND_BAR_DISMISS_REQUESTED.store(false, Ordering::Relaxed);
    }
}
fn command_bar_windowed_frame(
    window_width_px: f32,
    window_height_px: f32,
    scale: f32,
    measured_size: Option<Vec2>,
    bounds: Option<WindowedFrameRect>,
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

    let window_bounds = WindowedFrameRect {
        left: 0.0,
        top: 0.0,
        width: window_width_px,
        height: window_height_px,
    };
    let bounds = bounds.unwrap_or(window_bounds);
    let area_left = bounds.left / scale;
    let area_top = bounds.top / scale;
    let area_w = bounds.width / scale;
    let area_h = bounds.height / scale;
    let top = area_top + area_h * 0.15;
    let available_w = (area_w - MARGIN * 2.0).max(1.0);
    let min_w = MIN_W.min(available_w);
    let box_w = available_w.min(MAX_W).max(min_w);
    let available_h = (area_top + area_h - top - MARGIN).max(1.0);
    let min_h = MIN_H.min(available_h);
    let measured_h = measured_size
        .map(|size| size.y)
        .filter(|height| height.is_finite() && *height > 0.0)
        .unwrap_or(FALLBACK_H);
    let box_h = measured_h.min(available_h).max(min_h);
    let box_x = area_left + ((area_w - box_w) * 0.5).max(0.0);

    Some(CommandBarWindowedFrame {
        left_px: box_x * scale,
        top_px: top * scale,
        width_px: box_w * scale,
        height_px: box_h * scale,
    })
}
fn hide_windowed_command_bar(browsers: &Browsers, entity: Entity) {
    browsers.set_windowed_hidden(&entity, true);
}
/// The surface exists but must stay off screen — either prewarmed before any open, or revealing
/// while the page paints. Both keep the native view alive and parked outside the window so it can
/// still hold first responder.
fn command_bar_windowed_view_should_render_hidden(
    display: Display,
    visibility: Visibility,
) -> bool {
    display != Display::None && visibility == Visibility::Hidden
}
pub(crate) fn sync_windowed_command_bar(
    browsers: NonSend<Browsers>,
    modal_q: Query<
        (
            Entity,
            &Node,
            &Visibility,
            Has<CefKeyboardTarget>,
            Has<WebviewWindowed>,
            Option<&HostWindow>,
            Option<&CommandBarNativeSize>,
        ),
        With<Modal>,
    >,
    native_size_changed: Query<(), Changed<CommandBarNativeSize>>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    mut was_open: Local<bool>,
) {
    let matched = modal_q.single();
    let Ok((entity, node, visibility, has_keyboard_target, is_windowed, host_window, native_size)) =
        matched
    else {
        publish_native_command_bar_route(false, None, 1.0);
        *was_open = false;
        return;
    };
    let state = CommandBarState::from_modal(node.display, *visibility, has_keyboard_target);
    let open = state.is_shown();
    let owns_input = state.owns_input();
    let render_hidden = command_bar_windowed_view_should_render_hidden(node.display, *visibility);
    if !open && !render_hidden {
        publish_native_command_bar_route(owns_input, None, 1.0);
        if is_windowed {
            browsers.set_windowed_focus(&entity, false);
            hide_windowed_command_bar(&browsers, entity);
        }
        *was_open = false;
        return;
    }
    if !browsers.has_browser(entity) {
        publish_native_command_bar_route(owns_input, None, 1.0);
        return;
    }
    let window_entity = host_window
        .map(|h| h.0)
        .or_else(|| primary_window.single().ok());
    let Some(window_entity) = window_entity else {
        publish_native_command_bar_route(owns_input, None, 1.0);
        if is_windowed {
            hide_windowed_command_bar(&browsers, entity);
        }
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        publish_native_command_bar_route(owns_input, None, 1.0);
        if is_windowed {
            hide_windowed_command_bar(&browsers, entity);
        }
        return;
    };
    let scale = window.resolution.scale_factor();
    if !is_windowed {
        let frame = if open {
            native_size.map(|size| CommandBarWindowedFrame {
                left_px: size.shell_left * scale,
                top_px: size.shell_top * scale,
                width_px: size.shell_width * scale,
                height_px: size.shell_height * scale,
            })
        } else {
            None
        };
        publish_native_command_bar_route(owns_input, frame, scale);
        *was_open = open;
        return;
    }
    if render_hidden {
        publish_native_command_bar_route(owns_input, None, scale);
        let Some(frame) = command_bar_windowed_frame(
            window.resolution.physical_width() as f32,
            window.resolution.physical_height() as f32,
            scale,
            native_size.map(|size| Vec2::new(size.shell_width, size.shell_height)),
            NativeBridge::windowed_page_bounds(),
        ) else {
            hide_windowed_command_bar(&browsers, entity);
            return;
        };
        // A revealing bar already owns input, so keep the renderer focused; only a prewarmed,
        // never-opened surface gets unfocused here.
        if !owns_input {
            browsers.set_windowed_focus(&entity, false);
        }
        browsers.resize(
            &entity,
            Vec2::new(frame.width_px / scale, frame.height_px / scale),
            scale,
        );
        browsers.set_windowed_frame(
            &entity,
            -frame.width_px - 16.0 * scale,
            -frame.height_px - 16.0 * scale,
            frame.width_px,
            frame.height_px,
            scale,
        );
        browsers.set_windowed_hidden(&entity, false);
        *was_open = false;
        return;
    }
    let measured = native_size.map(|size| Vec2::new(size.shell_width, size.shell_height));
    let Some(frame) = command_bar_windowed_frame(
        window.resolution.physical_width() as f32,
        window.resolution.physical_height() as f32,
        scale,
        measured,
        NativeBridge::windowed_page_bounds(),
    ) else {
        publish_native_command_bar_route(owns_input, None, scale);
        hide_windowed_command_bar(&browsers, entity);
        return;
    };
    publish_native_command_bar_route(owns_input, Some(frame), scale);

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
    // A windowed CEF view cannot be transparent, so the page's own `rounded-2xl` leaves opaque
    // square corners behind it. Clip the outer view instead. This only touches the outermost
    // `CefBrowserHostView` layer, unlike `set_windowed_z_position`, which reorders that layer among
    // its siblings and leaves the view painting nothing but its background.
    browsers.set_windowed_corner_radius(&entity, COMMAND_BAR_NATIVE_RADIUS_PX * scale, scale, true);
    browsers.set_windowed_hidden(&entity, false);
    // Frontmost sibling wins AppKit hit-testing; the raise is a no-op once it already is.
    browsers.raise_windowed_to_front(&entity);
    browsers.set_windowed_focus(&entity, true);
    if !*was_open || native_size_changed.contains(entity) {
        browsers.nudge_windowed_repaint(&entity);
        *was_open = true;
    }
}
#[cfg(target_os = "macos")]
fn flush_native_command_bar_pointer_events(
    browsers: NonSend<Browsers>,
    modal_q: Query<Entity, (With<Modal>, With<WebviewWindowed>)>,
) {
    let Ok(entity) = modal_q.single() else {
        return;
    };
    for event in NativeBridge::drain_command_bar_pointer_events() {
        match event {
            CommandBarPointerEvent::Move { position, buttons } => {
                browsers.send_native_mouse_move(&entity, buttons, position, false);
            }
            CommandBarPointerEvent::Button {
                position,
                button,
                released,
            } => {
                bevy::log::info!(
                    ?entity,
                    ?position,
                    ?button,
                    released,
                    "command bar native pointer forwarded"
                );
                browsers.send_mouse_click(&entity, position, button, released);
            }
        }
    }
}
#[cfg(not(target_os = "macos"))]
fn flush_native_command_bar_pointer_events() {}
fn apply_repaint_nudge(browsers: NonSend<Browsers>, ready: Query<Entity, Changed<PageReady>>) {
    for entity in &ready {
        browsers.nudge_windowed_repaint(&entity);
    }
}
fn sync_cef_webview_resize_after_ui(
    browsers: NonSend<Browsers>,
    webviews: Query<(Entity, &WebviewSize), (With<Browser>, Without<Modal>)>,
    host_window: Query<&HostWindow>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut last_entries: Local<Vec<(u64, Vec2, f32)>>,
    mut window_resized: MessageReader<WindowResized>,
    mut first_run: Local<Option<std::time::Instant>>,
) {
    // Force-resize all CEF browsers (tabs, terminals, side sheets, modals) on
    // window resize so backgrounded surfaces also repaint at the new size
    // instead of showing a stale frame until they become active.
    let force = window_resized.read().count() > 0;
    if force {
        last_entries.clear();
    }
    let mut pushed_any = false;
    let mut awaiting_create = false;
    for (entity, size) in webviews.iter() {
        if !browsers.has_browser(entity) {
            awaiting_create = true;
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
        pushed_any = true;
        if let Some(entry) = last_entries.iter_mut().find(|(k, _, _)| *k == key) {
            entry.1 = size.0;
            entry.2 = device_scale_factor;
        } else {
            last_entries.push((key, size.0, device_scale_factor));
        }
    }
    let within_startup_grace = first_run
        .get_or_insert_with(std::time::Instant::now)
        .elapsed()
        < std::time::Duration::from_secs(10);
    if windowed_reconcile_should_wake(pushed_any, awaiting_create, within_startup_grace)
        && let Some(proxy) = proxy.as_ref()
    {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}
fn windowed_reconcile_should_wake(
    pushed_any: bool,
    awaiting_create: bool,
    within_startup_grace: bool,
) -> bool {
    pushed_any || (awaiting_create && within_startup_grace)
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
    mode: Res<vmux_layout::scene::InteractionMode>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    tabs: Query<
        (
            Entity,
            &WebviewSize,
            &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
        ),
        (With<Browser>, Without<LayoutCef>, Without<Modal>),
    >,
    status: Query<
        (
            &WebviewSize,
            &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
        ),
        With<Header>,
    >,
    side_sheet: Query<
        (
            &WebviewSize,
            &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
        ),
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
        let pane_count = pane_count_for_browser(
            browser_e,
            &child_of_q,
            &tab_q,
            &pane_q,
            &all_children,
            &leaf_panes,
        )
        .unwrap_or(1);
        let corner_mode = if *mode == vmux_layout::scene::InteractionMode::Player
            || layout_hidden.0
            || pane_count > 1
        {
            0.0
        } else {
            1.0
        };
        if let Some(mut mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, corner_mode);
            mat.base.alpha_mode = webview_content_alpha_mode(mat.base.base_color.alpha(), r);
        }
    }
    for (size, mat_h) in &status {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mut mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 0.0);
            mat.base.alpha_mode = webview_content_alpha_mode(mat.base.base_color.alpha(), r);
        }
    }
    for (size, mat_h) in &side_sheet {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mut mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 0.0);
            mat.base.alpha_mode = webview_content_alpha_mode(mat.base.base_color.alpha(), r);
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
            Has<WebviewWindowed>,
            Has<LayoutCef>,
            Has<BookmarkTextInputActive>,
            Has<BookmarkContextMenuActive>,
            Has<CommandBarPanelActive>,
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
    let mut layout_shells = Vec::new();
    let mut modal_keyboard_target = None;
    let mut layout_keyboard_target = None;
    let window_visible = primary_window.visible;
    let window_focused = primary_window.focused;
    for (
        entity,
        visibility,
        computed,
        pending_reveal,
        pending_command_bar_reveal,
        is_modal,
        has_keyboard_target,
        is_windowed,
        is_layout,
        bookmark_text_input_active,
        bookmark_context_menu_active,
        command_bar_panel_active,
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
            if is_layout {
                layout_shells.push(entity);
                if bookmark_text_input_active
                    || bookmark_context_menu_active
                    || command_bar_panel_active
                {
                    layout_keyboard_target = Some(entity);
                }
            }
            if is_modal && has_keyboard_target {
                modal_keyboard_target = Some((entity, is_windowed));
            }
        } else if keep_hidden_osr_webview_warm(is_modal, is_windowed, window_visible) {
            browsers.set_osr_not_hidden(&entity);
        } else {
            browsers.set_osr_hidden(&entity);
        }
    }
    if ready.is_empty() {
        return;
    }
    ready.sort_by_key(|e| e.to_bits());
    let active_stack_opt = focus.stack;
    let active_stack = active_stack_opt.and_then(|tab| {
        ready
            .iter()
            .copied()
            .find(|&b| child_of_q.get(b).ok().map(|co| co.get()) == Some(tab))
    });
    let active = layout_keyboard_target
        .or_else(|| choose_osr_active_webview(modal_keyboard_target, active_stack, ready[0]));

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
    } else if *last_active == active && *last_ready_set == *ready {
    } else {
        auxiliary.clear();
        let (active, next_auxiliary) = osr_focus_targets(
            ready.as_slice(),
            active,
            layout_keyboard_target.is_some(),
            |e| layout_shells.contains(&e),
        );
        auxiliary.extend(next_auxiliary);
        webview_debug_log(format!(
            "osr focus active={active:?} auxiliary={:?} ready={ready:?}",
            auxiliary.as_slice()
        ));
        browsers.sync_osr_focus_to_active_pane(active, auxiliary.as_slice());
        *last_active = active;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HiddenWebviewSizing {
    Render,
    HideKeepSize,
    Collapse,
}
fn hidden_webview_sizing(renderable: bool, under_inactive_tab: bool) -> HiddenWebviewSizing {
    if renderable {
        HiddenWebviewSizing::Render
    } else if under_inactive_tab {
        HiddenWebviewSizing::HideKeepSize
    } else {
        HiddenWebviewSizing::Collapse
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
fn keep_hidden_osr_webview_warm(is_modal: bool, is_windowed: bool, window_visible: bool) -> bool {
    is_modal && !is_windowed && window_visible
}
fn choose_osr_active_webview(
    modal_keyboard_target: Option<(Entity, bool)>,
    active_stack: Option<Entity>,
    fallback: Entity,
) -> Option<Entity> {
    if modal_keyboard_target.is_some_and(|(_, is_windowed)| is_windowed) {
        None
    } else {
        modal_keyboard_target
            .map(|(entity, _)| entity)
            .or(active_stack)
            .or(Some(fallback))
    }
}
fn osr_focus_targets(
    ready: &[Entity],
    active: Option<Entity>,
    allow_layout_active: bool,
    mut is_layout: impl FnMut(Entity) -> bool,
) -> (Option<Entity>, Vec<Entity>) {
    let active = active.filter(|&e| allow_layout_active || !is_layout(e));
    let auxiliary = ready
        .iter()
        .copied()
        .filter(|&e| Some(e) != active)
        .collect();
    (active, auxiliary)
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
fn flush_pending_osr_textures(
    mut ew: MessageWriter<RenderTextureMessage>,
    browsers: NonSend<Browsers>,
) {
    for texture in browsers.drain_render_textures() {
        ew.write(texture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_app_settings_with_radius;
    use crate::{
        command_bar_windowed_click_should_dismiss, native_command_bar_route,
        request_native_command_bar_dismiss, request_native_command_bar_dismiss_for_mouse_down,
        take_native_command_bar_dismiss_requested,
    };
    use bevy::input::ButtonState;

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
    fn inactive_tab_pages_keep_size_other_hidden_pages_collapse() {
        assert_eq!(
            hidden_webview_sizing(true, false),
            HiddenWebviewSizing::Render
        );
        assert_eq!(
            hidden_webview_sizing(true, true),
            HiddenWebviewSizing::Render
        );
        assert_eq!(
            hidden_webview_sizing(false, true),
            HiddenWebviewSizing::HideKeepSize
        );
        assert_eq!(
            hidden_webview_sizing(false, false),
            HiddenWebviewSizing::Collapse
        );
    }
    #[test]
    fn layout_shell_osr_renders_above_player_page_osr() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .add_systems(Update, sync_children_to_ui);

        let glass = app
            .world_mut()
            .spawn((
                VmuxWindow,
                ComputedNode {
                    size: Vec2::new(1200.0, 800.0),
                    ..default()
                },
                UiGlobalTransform::default(),
            ))
            .id();
        let layout = app
            .world_mut()
            .spawn((
                Browser,
                LayoutCef,
                Transform::default(),
                ComputedNode {
                    size: Vec2::new(1200.0, 800.0),
                    ..default()
                },
                bevy::ui::ComputedStackIndex(0),
                UiGlobalTransform::default(),
                WebviewSize(Vec2::ONE),
                ChildOf(glass),
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let pane = app
            .world_mut()
            .spawn((
                Pane,
                ComputedNode {
                    size: Vec2::new(1200.0, 740.0),
                    ..default()
                },
                UiGlobalTransform::default(),
                ChildOf(tab),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
            .id();
        let page = app
            .world_mut()
            .spawn((
                Browser,
                Transform::default(),
                ComputedNode {
                    size: Vec2::new(1200.0, 740.0),
                    ..default()
                },
                bevy::ui::ComputedStackIndex(0),
                UiGlobalTransform::default(),
                WebviewSize(Vec2::ONE),
                ChildOf(stack),
            ))
            .id();

        app.update();

        let layout_z = app.world().get::<Transform>(layout).unwrap().translation.z;
        let page_z = app.world().get::<Transform>(page).unwrap().translation.z;

        assert!(layout_z > page_z);
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
    fn hidden_osr_command_bar_stays_warm_for_reopen() {
        assert!(keep_hidden_osr_webview_warm(true, false, true));
        assert!(!keep_hidden_osr_webview_warm(false, false, true));
        assert!(!keep_hidden_osr_webview_warm(true, true, true));
        assert!(!keep_hidden_osr_webview_warm(true, false, false));
    }
    #[test]
    fn command_bar_modal_wins_osr_focus_for_keyboard_input() {
        let pane = Entity::from_bits(1);
        let modal = Entity::from_bits(2);

        assert_eq!(
            choose_osr_active_webview(Some((modal, false)), Some(pane), pane),
            Some(modal)
        );
    }
    #[test]
    fn windowed_command_bar_modal_suppresses_osr_focus_targets() {
        let pane = Entity::from_bits(1);
        let modal = Entity::from_bits(2);

        assert_eq!(
            choose_osr_active_webview(Some((modal, true)), Some(pane), pane),
            None
        );
    }
    #[test]
    fn open_command_bar_is_exclusive_cef_keyboard_target() {
        let mut app = App::new();
        app.add_plugins(vmux_layout::LayoutContractPlugin)
            .insert_resource(vmux_layout::scene::InteractionMode::User)
            .insert_resource(CefSuppressKeyboardInput(true))
            .add_systems(Update, sync_keyboard_target);
        let page = app.world_mut().spawn((Browser, CefKeyboardTarget)).id();
        let modal = app
            .world_mut()
            .spawn((
                Browser,
                Modal,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                CefKeyboardTarget,
            ))
            .id();

        app.update();

        assert!(app.world().get::<CefKeyboardTarget>(modal).is_some());
        assert!(app.world().get::<CefKeyboardTarget>(page).is_none());
        assert!(!app.world().resource::<CefSuppressKeyboardInput>().0);
    }
    #[test]
    fn layout_shell_is_auxiliary_osr_focus_target() {
        let active = Entity::from_bits(1);
        let layout = Entity::from_bits(2);
        let sidecar = Entity::from_bits(3);

        assert_eq!(
            osr_focus_targets(&[active, layout, sidecar], Some(active), false, |e| e
                == layout),
            (Some(active), vec![layout, sidecar])
        );
    }
    #[test]
    fn layout_shell_is_not_active_osr_focus_target() {
        let layout = Entity::from_bits(1);
        let sidecar = Entity::from_bits(2);

        assert_eq!(
            osr_focus_targets(&[layout, sidecar], Some(layout), false, |e| e == layout),
            (None, vec![layout, sidecar])
        );
    }
    #[test]
    fn bookmark_text_input_can_make_layout_shell_active_osr_target() {
        let layout = Entity::from_bits(1);
        let sidecar = Entity::from_bits(2);

        assert_eq!(
            osr_focus_targets(&[layout, sidecar], Some(layout), true, |e| e == layout),
            (Some(layout), vec![sidecar])
        );
    }
    #[test]
    fn windowed_pages_hide_on_deactivate_and_first_show() {
        let just_deactivated = Entity::from_bits(1);
        let still_inactive = Entity::from_bits(2);
        let never_shown = Entity::from_bits(3);

        let hidden = [just_deactivated, still_inactive, never_shown];
        let prev_visible = [just_deactivated];
        let ever_shown = [just_deactivated, still_inactive];

        assert_eq!(
            windowed_pages_to_hide(&hidden, &prev_visible, &ever_shown, &[]),
            vec![just_deactivated, never_shown]
        );
    }
    #[test]
    fn recreated_inactive_windowed_page_is_hidden() {
        let page = Entity::from_bits(1);

        assert_eq!(
            windowed_pages_to_hide(&[page], &[], &[page], &[page]),
            vec![page]
        );
    }
    #[test]
    fn windowed_content_mesh_material_is_hidden() {
        let mut material = WebviewExtendStandardMaterial::default();

        set_windowed_content_mesh_material(&mut material, true);

        assert_eq!(material.base.base_color.alpha(), 0.0);
        assert_eq!(material.base.alpha_mode, AlphaMode::Blend);

        set_windowed_content_mesh_material(&mut material, false);

        assert_eq!(material.base.base_color.alpha(), 1.0);
        assert_eq!(material.base.alpha_mode, AlphaMode::Opaque);
    }
    #[test]
    fn player_osr_pane_clip_uses_alpha_to_coverage_for_rounded_corners() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_app_settings_with_radius(12.0))
            .insert_resource(vmux_layout::toggle::LayoutHidden(false))
            .insert_resource(vmux_layout::scene::InteractionMode::Player)
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, sync_webview_pane_corner_clip);

        let handle = app
            .world_mut()
            .resource_mut::<Assets<WebviewExtendStandardMaterial>>()
            .add(WebviewExtendStandardMaterial::default());
        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane)))
            .id();
        app.world_mut().spawn((
            Browser,
            WebviewSize(Vec2::new(320.0, 240.0)),
            WebviewMaterialHandle(handle.clone()),
            ChildOf(stack),
        ));

        app.update();

        let material = app
            .world()
            .resource::<Assets<WebviewExtendStandardMaterial>>()
            .get(&handle)
            .expect("webview material");

        assert_eq!(
            material.extension.pane_corner_clip,
            Vec4::new(12.0, 320.0, 240.0, 0.0)
        );
        assert_eq!(material.base.alpha_mode, AlphaMode::AlphaToCoverage);
    }
    #[test]
    fn layout_cef_shell_keeps_blend_material() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_app_settings_with_radius(12.0))
            .insert_resource(vmux_layout::toggle::LayoutHidden(false))
            .insert_resource(vmux_layout::scene::InteractionMode::Player)
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, sync_webview_pane_corner_clip);

        let mut material = WebviewExtendStandardMaterial::default();
        material.base.alpha_mode = AlphaMode::Blend;
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WebviewExtendStandardMaterial>>()
            .add(material);
        app.world_mut().spawn((
            Browser,
            LayoutCef,
            WebviewSize(Vec2::new(320.0, 240.0)),
            WebviewMaterialHandle(handle.clone()),
        ));

        app.update();

        let material = app
            .world()
            .resource::<Assets<WebviewExtendStandardMaterial>>()
            .get(&handle)
            .expect("webview material");

        assert_eq!(material.extension.pane_corner_clip, Vec4::ZERO);
        assert_eq!(material.base.alpha_mode, AlphaMode::Blend);
    }
    #[test]
    fn windowed_page_keeps_single_pane_top_edge_flat_under_header() {
        assert!(!windowed_page_all_corners(false, 1));
    }
    #[test]
    fn windowed_page_rounds_when_layout_hidden_or_split() {
        assert!(windowed_page_all_corners(true, 1));
        assert!(windowed_page_all_corners(false, 2));
    }
    #[test]
    fn single_pane_windowed_frame_matches_header_edges_without_side_gaps() {
        let pane = WindowedFrameRect {
            left: 60.2,
            top: 84.0,
            width: 150.6,
            height: 300.0,
        };
        let header = WindowedFrameRect {
            left: 72.1,
            top: 0.0,
            width: 130.8,
            height: 84.2,
        };

        let frame = windowed_page_frame_rect(pane, Some(header), false, 1);

        assert_eq!(
            frame,
            WindowedFrameRect {
                left: 73.0,
                top: 85.0,
                width: 129.0,
                height: 299.0,
            }
        );
    }
    #[test]
    fn split_pane_windowed_frame_starts_below_header_without_changing_width() {
        let pane = WindowedFrameRect {
            left: 610.2,
            top: 24.0,
            width: 560.6,
            height: 720.0,
        };
        let header = WindowedFrameRect {
            left: 150.0,
            top: 24.0,
            width: 1020.0,
            height: 72.2,
        };

        let frame = windowed_page_frame_rect(pane, Some(header), false, 2);

        assert_eq!(
            frame,
            WindowedFrameRect {
                left: 611.0,
                top: 97.0,
                width: 559.0,
                height: 647.0,
            }
        );
    }
    #[test]
    fn windowed_frame_hit_test_uses_physical_page_bounds() {
        let frame = WindowedFrameRect {
            left: 100.0,
            top: 50.0,
            width: 400.0,
            height: 300.0,
        };

        assert!(NativeBridge::frame_contains(frame, Vec2::new(100.0, 50.0)));
        assert!(NativeBridge::frame_contains(frame, Vec2::new(500.0, 350.0)));
        assert!(!NativeBridge::frame_contains(frame, Vec2::new(99.0, 200.0)));
        assert!(!NativeBridge::frame_contains(
            frame,
            Vec2::new(300.0, 351.0)
        ));
    }
    #[test]
    fn command_bar_windowed_frame_uses_measured_height() {
        let frame =
            command_bar_windowed_frame(1600.0, 1000.0, 2.0, Some(Vec2::new(500.0, 220.0)), None)
                .unwrap();

        assert!((frame.left_px - 224.0).abs() < 0.01);
        assert!((frame.top_px - 150.0).abs() < 0.01);
        assert!((frame.width_px - 1152.0).abs() < 0.01);
        assert!((frame.height_px - 440.0).abs() < 0.01);
    }
    #[test]
    fn command_bar_windowed_frame_clamps_height_to_window() {
        let frame =
            command_bar_windowed_frame(800.0, 500.0, 1.0, Some(Vec2::new(500.0, 1000.0)), None)
                .unwrap();

        assert!((frame.top_px - 75.0).abs() < 0.01);
        assert!((frame.height_px - 409.0).abs() < 0.01);
    }
    #[test]
    fn command_bar_windowed_frame_centers_in_page_workspace() {
        let frame = command_bar_windowed_frame(
            1600.0,
            1000.0,
            2.0,
            Some(Vec2::new(500.0, 220.0)),
            Some(WindowedFrameRect {
                left: 300.0,
                top: 100.0,
                width: 1200.0,
                height: 800.0,
            }),
        )
        .unwrap();

        assert!((frame.left_px - 332.0).abs() < 0.01);
        assert!((frame.top_px - 220.0).abs() < 0.01);
        assert!((frame.width_px - 1136.0).abs() < 0.01);
        assert!((frame.height_px - 440.0).abs() < 0.01);
    }
    #[test]
    fn windowed_command_bar_outside_click_dismisses() {
        let frame = CommandBarWindowedFrame {
            left_px: 100.0,
            top_px: 50.0,
            width_px: 200.0,
            height_px: 100.0,
        };

        assert!(command_bar_windowed_click_should_dismiss(
            true,
            MouseButton::Left,
            ButtonState::Pressed,
            Some(Vec2::new(99.0, 80.0)),
            Some(frame),
        ));
        assert!(!command_bar_windowed_click_should_dismiss(
            true,
            MouseButton::Left,
            ButtonState::Pressed,
            Some(Vec2::new(150.0, 80.0)),
            Some(frame),
        ));
        assert!(!command_bar_windowed_click_should_dismiss(
            true,
            MouseButton::Right,
            ButtonState::Pressed,
            Some(Vec2::new(99.0, 80.0)),
            Some(frame),
        ));
        assert!(!command_bar_windowed_click_should_dismiss(
            false,
            MouseButton::Left,
            ButtonState::Pressed,
            Some(Vec2::new(99.0, 80.0)),
            Some(frame),
        ));
    }
    /// One test, because every case here mutates the process-wide published route and the test
    /// runner is multi-threaded.
    #[test]
    fn published_route_is_the_only_source_of_command_bar_hit_state() {
        let frame = CommandBarWindowedFrame {
            left_px: 100.0,
            top_px: 50.0,
            width_px: 200.0,
            height_px: 100.0,
        };

        let before = native_command_bar_route().generation;
        publish_native_command_bar_route(true, Some(frame), 2.0);
        let published = native_command_bar_route();
        assert_eq!(published.generation, before.wrapping_add(1));
        assert_eq!(published.scale, 2.0);

        // A frame published by a bar that no longer owns input must not turn an unrelated click
        // into a dismiss.
        publish_native_command_bar_route(false, Some(frame), 1.0);
        assert!(!request_native_command_bar_dismiss_for_mouse_down(
            90.0, 60.0
        ));
        assert!(!take_native_command_bar_dismiss_requested());

        publish_native_command_bar_route(true, Some(frame), 1.0);
        assert!(!request_native_command_bar_dismiss_for_mouse_down(
            120.0, 60.0
        ));
        assert!(!take_native_command_bar_dismiss_requested());
        assert!(request_native_command_bar_dismiss_for_mouse_down(
            90.0, 60.0
        ));
        assert!(take_native_command_bar_dismiss_requested());
        assert!(!take_native_command_bar_dismiss_requested());

        // Revealing: owns input, but no rectangle is on screen to click outside of yet.
        publish_native_command_bar_route(true, None, 1.0);
        assert!(!request_native_command_bar_dismiss_for_mouse_down(
            90.0, 60.0
        ));

        // Closing drops a dismiss that was requested while open.
        assert!(request_native_command_bar_dismiss());
        publish_native_command_bar_route(false, None, 1.0);
        assert!(!take_native_command_bar_dismiss_requested());
    }
    #[test]
    fn revealing_command_bar_owns_input_while_its_view_stays_parked() {
        let revealing = CommandBarState::from_modal(Display::Flex, Visibility::Hidden, true);

        assert!(revealing.owns_input());
        assert!(!revealing.is_shown());
        assert!(command_bar_windowed_view_should_render_hidden(
            Display::Flex,
            Visibility::Hidden
        ));
    }
    #[test]
    fn collapsed_command_bar_view_is_never_render_hidden() {
        assert!(!command_bar_windowed_view_should_render_hidden(
            Display::None,
            Visibility::Hidden
        ));
        assert!(!command_bar_windowed_view_should_render_hidden(
            Display::Flex,
            Visibility::Inherited
        ));
    }
    #[test]
    fn windowed_reconcile_wakes_until_native_pages_are_sized() {
        assert!(windowed_reconcile_should_wake(true, false, false));
        assert!(windowed_reconcile_should_wake(false, true, true));
        assert!(!windowed_reconcile_should_wake(false, true, false));
        assert!(!windowed_reconcile_should_wake(false, false, true));
    }
}
