use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    window::WindowResized,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use vmux_command::CommandBar;
use vmux_command::CommandBarPanelActive;
use vmux_command::{CommandBarNativeSize, PendingCommandBarReveal};
use vmux_ecs::overlay::{OverlayState, WindowOverlay};
use vmux_ecs::page::PageReady;
use vmux_layout::Browser;
use vmux_layout::active_pane::ActiveStack;
use vmux_layout::{
    Header, LayoutCef, Open, PendingWebviewReveal,
    bookmark::{BookmarkContextMenuActive, BookmarkTextInputActive},
    pane::Pane,
    presentation::Layer as PresentationLayer,
    side_sheet::SideSheet,
    stack::{LayoutFocus, Stack},
    tab::Tab,
    window::{
        VmuxWindow, WEBVIEW_Z_BASE, WEBVIEW_Z_HEADER, WEBVIEW_Z_MAIN, WEBVIEW_Z_MODAL,
        WEBVIEW_Z_SIDE_SHEET, WindowHierarchy,
    },
};

use vmux_ecs::{KeyboardOwner, PagePresented};
use vmux_setting::AppSettings;

use crate::host::{CommandBarRoute, LayoutPointerCapture, NATIVE_COMMAND_BAR_ROUTE};

#[cfg(target_os = "macos")]
use crate::native_bridge::CommandBarPointerEvent;
use crate::native_bridge::NativeBridge;
use vmux_flex::prelude::*;

pub(crate) struct PresentPlugin;

impl Plugin for PresentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (
                sync_keyboard_target,
                sync_children_to_ui,
                sync_windowed_layout,
                sync_windowed_frames.in_set(crate::BrowserSystemSet::SyncWindowedFrames),
                sync_windowed_command_bar.in_set(crate::BrowserSystemSet::SyncWindowedCommandBar),
                flush_command_bar_pointer,
                apply_repaint_nudge,
                sync_webview_resize,
                sync_osr_webview_focus,
            )
                .chain()
                .after(vmux_layout::presentation::StackMotionSet)
                .after(LayoutSystems::Layout),
        );
    }
}

type LayoutKeyboardCapture = Or<(
    With<BookmarkTextInputActive>,
    With<BookmarkContextMenuActive>,
    With<CommandBarPanelActive>,
)>;

pub(crate) type LayoutKeyboardHost = (With<LayoutCef>, LayoutKeyboardCapture);

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct WindowFrameQueries<'w, 's> {
    stack_rect: Query<'w, 's, &'static vmux_layout::presentation::StackRect>,
    hierarchy: WindowHierarchy<'w, 's>,
    child_of: Query<'w, 's, &'static ChildOf>,
    focus: LayoutFocus<'w, 's>,
    pane_rect: Query<'w, 's, &'static ComputedNode, With<Pane>>,
    header_rect: Query<'w, 's, (Entity, &'static ComputedNode), (With<Header>, With<Open>)>,
    tabs: Query<'w, 's, (), With<Tab>>,
    layers: Query<'w, 's, &'static PresentationLayer>,
}

impl WindowFrameQueries<'_, '_> {
    pub(crate) fn tab_ancestor(&self, start: Entity) -> Option<Entity> {
        let mut entity = start;
        loop {
            if self.tabs.contains(entity) {
                return Some(entity);
            }
            entity = self.child_of.get(entity).ok()?.get();
        }
    }

    pub(crate) fn presentation(&self, start: Entity) -> PresentationLayer {
        let mut entity = start;
        let mut resolved = PresentationLayer::Current;
        for _ in 0..32 {
            if let Ok(layer) = self.layers.get(entity) {
                match *layer {
                    PresentationLayer::Hidden => return PresentationLayer::Hidden,
                    PresentationLayer::Retained => resolved = PresentationLayer::Retained,
                    PresentationLayer::Incoming if resolved != PresentationLayer::Retained => {
                        resolved = PresentationLayer::Incoming;
                    }
                    PresentationLayer::Current | PresentationLayer::Incoming => {}
                }
            }
            let Ok(parent) = self.child_of.get(entity) else {
                break;
            };
            entity = parent.parent();
        }
        resolved
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn page_is_active(&self, page: Entity) -> bool {
        matches!(
            self.presentation(page),
            PresentationLayer::Current | PresentationLayer::Incoming
        )
    }
}

fn sync_keyboard_target(
    presentation: WindowFrameQueries,
    focus: vmux_layout::stack::FocusedStack,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    modal_q: Query<(Entity, &Node, Has<KeyboardOwner>), With<WindowOverlay>>,
    layout_keyboard_q: Query<(Entity, &HostWindow), LayoutKeyboardHost>,
    focused_window: vmux_layout::window::FocusedWindow,
    content_q: Query<
        (
            Entity,
            Has<KeyboardOwner>,
            Has<vmux_layout::hosted_page::SurfaceHandoff>,
            Has<vmux_layout::hosted_page::SurfaceBlocked>,
        ),
        With<Browser>,
    >,
    mut commands: Commands,
) {
    if let Some(modal) = modal_q.iter().find_map(|(entity, node, keyboard_target)| {
        (node.display != Display::None && keyboard_target).then_some(entity)
    }) {
        for (browser_e, has_kb, _, _) in &content_q {
            if browser_e != modal && has_kb {
                commands.entity(browser_e).try_remove::<KeyboardOwner>();
            }
        }
        return;
    }

    if layout_keyboard_q
        .iter()
        .any(|(_, host)| Some(host.0) == focused_window.entity())
    {
        for (browser_e, has_kb, _, _) in &content_q {
            if has_kb {
                commands.entity(browser_e).try_remove::<KeyboardOwner>();
            }
        }
        return;
    }

    let focused_target = focus.stack.and_then(|stack| {
        content_q
            .iter()
            .find(|(browser, _, handing_off, blocked)| {
                !*handing_off
                    && !*blocked
                    && !status_q.contains(*browser)
                    && !side_sheet_q.contains(*browser)
                    && presentation
                        .child_of
                        .get(*browser)
                        .is_ok_and(|parent| parent.parent() == stack)
                    && presentation.presentation(*browser).interactive()
            })
            .map(|(browser, _, _, _)| browser)
    });
    let target = focused_target;

    for (browser, has_keyboard, _, _) in &content_q {
        if status_q.contains(browser) || side_sheet_q.contains(browser) {
            continue;
        }
        if target == Some(browser) {
            if !has_keyboard {
                commands.entity(browser).try_insert(KeyboardOwner);
            }
        } else if has_keyboard {
            commands.entity(browser).try_remove::<KeyboardOwner>();
        }
    }
}

fn sync_children_to_ui(
    mut browser_q: Query<
        (
            Entity,
            &mut Transform,
            &ComputedNode,
            &ChildOf,
            &mut WebviewSize,
            Option<&Header>,
            Option<&SideSheet>,
            Option<&WindowOverlay>,
            Option<&Visibility>,
            Option<&HistorySwipeVisualOffset>,
            Has<PendingWebviewReveal>,
            Has<PendingCommandBarReveal>,
            Has<LayoutCef>,
            Has<WebviewWindowed>,
            Has<PagePresented>,
        ),
        With<Browser>,
    >,
    readiness: Query<
        (
            Has<PageReady>,
            Has<vmux_ecs::page::HostsPage>,
            Has<vmux_layout::hosted_page::SurfacePresentationReady>,
        ),
        With<Browser>,
    >,
    queries: WindowFrameQueries,
    roots: Query<(Entity, &HostWindow, &ComputedNode), With<VmuxWindow>>,
    mut commands: Commands,
) {
    for (
        browser,
        mut tf,
        self_computed,
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
        presented,
    ) in browser_q.iter_mut()
    {
        let (ready, hosted, presentation_ready) =
            readiness.get(browser).unwrap_or((false, false, false));
        let Some(host_window) = queries.hierarchy.get(browser) else {
            if presented {
                commands.entity(browser).remove::<PagePresented>();
            }
            continue;
        };
        let Some((glass_entity, _, glass_node)) =
            roots.iter().find(|(_, host, _)| host.0 == host_window)
        else {
            if presented {
                commands.entity(browser).remove::<PagePresented>();
            }
            continue;
        };
        let glass_rect = *glass_node;
        let glass_size_px = glass_rect.padding_box();
        let parent = child_of.get();
        let pane_entity = queries
            .child_of
            .get(parent)
            .map(|co| co.get())
            .unwrap_or(parent);
        let computed = match queries.pane_rect.get(pane_entity) {
            Ok(cn) => cn,
            Err(_) => self_computed,
        };
        let computed = queries
            .stack_rect
            .get(parent)
            .map(|rect| &rect.0)
            .unwrap_or(computed);

        if glass_size_px.x <= 0.0 || glass_size_px.y <= 0.0 {
            if presented {
                commands.entity(browser).remove::<PagePresented>();
            }
            continue;
        }

        let is_cef_ui = status.is_some() || side_sheet.is_some() || modal.is_some();

        let under_inactive_tab = parent != glass_entity
            && !is_cef_ui
            && queries.presentation(parent) == PresentationLayer::Hidden;

        let size_px = computed.size;
        let renderable = webview_layout_is_renderable(
            size_px,
            visibility,
            pending_webview_reveal || pending_command_bar_reveal,
        );
        match hidden_webview_sizing(renderable, under_inactive_tab) {
            HiddenWebviewSizing::Render => {}
            HiddenWebviewSizing::HideKeepSize => {
                let hidden = Vec3::splat(1.0e-6);
                if tf.scale != hidden {
                    tf.scale = hidden;
                }
                if presented {
                    commands.entity(browser).remove::<PagePresented>();
                }
                continue;
            }
            HiddenWebviewSizing::Collapse => {
                let hidden = Vec3::splat(1.0e-6);
                if tf.scale != hidden {
                    tf.scale = hidden;
                }
                if webview_size.0 != Vec2::ONE {
                    webview_size.0 = Vec2::ONE;
                }
                if presented {
                    commands.entity(browser).remove::<PagePresented>();
                }
                continue;
            }
        }

        let branch_presentation = queries.presentation(parent);
        let is_active_stack = matches!(
            branch_presentation,
            PresentationLayer::Current | PresentationLayer::Incoming
        );
        let is_inactive_stack = parent != glass_entity
            && !is_cef_ui
            && branch_presentation == PresentationLayer::Hidden;

        let is_inactive_tab = under_inactive_tab;

        let pending_reveal = pending_webview_reveal || pending_command_bar_reveal;
        let should_present = !pending_reveal && !is_inactive_stack && !is_inactive_tab;
        if should_present {
            if !presented {
                commands.entity(browser).insert(PagePresented);
            }
            if ready && !hosted && !is_windowed && !presentation_ready {
                commands
                    .entity(browser)
                    .insert(vmux_layout::hosted_page::SurfacePresentationReady);
            }
        } else if presented {
            commands.entity(browser).remove::<PagePresented>();
        }

        let sx = size_px.x / glass_size_px.x;
        let sy = size_px.y / glass_size_px.y;
        let new_scale = if pending_reveal || is_inactive_stack || is_inactive_tab {
            Vec3::splat(1e-6)
        } else {
            Vec3::new(sx, sy, 1.0)
        };
        if tf.scale != new_scale {
            tf.scale = new_scale;
        }

        let delta_px = computed.center - glass_rect.center;

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
            WEBVIEW_Z_BASE
        };
        let history_swipe_tx = if parent != glass_entity && !is_cef_ui {
            history_swipe_visual
                .map(|visual| visual.offset_px / glass_size_px.x)
                .unwrap_or(0.0)
        } else {
            0.0
        };
        let translation = Vec3::new(tx + history_swipe_tx, ty, z);
        if tf.translation != translation {
            tf.translation = translation;
        }

        if modal.is_some() && is_windowed {
            continue;
        }

        let dip = (size_px * computed.inverse_scale_factor).max(Vec2::splat(1.0));
        if webview_size.0 != dip {
            webview_size.0 = dip;
        }
    }
}

struct AgentRingColor;

impl AgentRingColor {
    fn for_key(key: &str) -> [f32; 3] {
        let mut hue = 1469598103934665603_u64;
        for byte in key.bytes() {
            hue ^= byte as u64;
            hue = hue.wrapping_mul(1099511628211);
        }
        Self::from_hsl((hue % 360) as f32, 0.85, 0.62)
    }

    fn from_hsl(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
        let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
        let section = hue / 60.0;
        let second = chroma * (1.0 - (section % 2.0 - 1.0).abs());
        let (red, green, blue) = match section as i32 {
            0 => (chroma, second, 0.0),
            1 => (second, chroma, 0.0),
            2 => (0.0, chroma, second),
            3 => (0.0, second, chroma),
            4 => (second, 0.0, chroma),
            _ => (chroma, 0.0, second),
        };
        let offset = lightness - chroma / 2.0;
        [red + offset, green + offset, blue + offset]
    }
}

fn windowed_ring_for(
    stack: Entity,
    focus: &ActiveStack,
    visible_pane_count: usize,
    agent: Option<(&str, ActiveStack)>,
    settings: &AppSettings,
    scale: f32,
) -> (f32, [f32; 3]) {
    let width = settings.layout.focus_ring.width * scale;
    let user = &settings.layout.focus_ring.color;
    if focus.stack == Some(stack) && visible_pane_count > 1 {
        return (width, [user.r, user.g, user.b]);
    }
    if let Some((profile, _)) = agent {
        return (width, AgentRingColor::for_key(profile));
    }
    (0.0, [user.r, user.g, user.b])
}

fn sync_windowed_frames(
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    added_hidden_windows: Query<(), Added<vmux_layout::toggle::LayoutHidden>>,
    mut removed_hidden_windows: RemovedComponents<vmux_layout::toggle::LayoutHidden>,
    focus: vmux_layout::stack::FocusedStack,
    active_panes: vmux_layout::active_pane::ActivePaneQuery,
    clear_color: Res<vmux_layout::window::WindowBackground>,
    browser_q: Query<
        (
            Entity,
            &ComputedNode,
            &ChildOf,
            Option<&Visibility>,
            Has<PendingWebviewReveal>,
            Has<vmux_layout::hosted_page::SurfaceBlocked>,
            Has<vmux_ecs::PageTransition>,
            Has<PageReady>,
            Has<vmux_layout::hosted_page::SurfacePaintReady>,
            Has<vmux_layout::hosted_page::SurfacePresentationReady>,
            Option<&BinIpcSurfaceGeneration>,
        ),
        (
            With<Browser>,
            With<WebviewWindowed>,
            Without<LayoutCef>,
            Without<WindowOverlay>,
        ),
    >,
    queries: WindowFrameQueries,
    mut memory: Local<FrameSyncMemory>,
    mut last_windowed_pages: Local<Vec<Entity>>,
    focused_window: vmux_layout::window::FocusedWindow,
    capturing: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    mut commands: Commands,
) {
    let hidden_window_removed = removed_hidden_windows.read().count() > 0;
    let force_raise = !added_hidden_windows.is_empty() || hidden_window_removed;
    let mut hidden = Vec::new();
    let mut visible = Vec::new();
    memory.visible_frames.clear();
    for (
        entity,
        self_computed,
        child_of,
        visibility,
        pending_reveal,
        surface_blocked,
        transitioning,
        ready,
        paint_ready,
        presentation_ready,
        generation,
    ) in &browser_q
    {
        let parent = child_of.get();
        let pane_entity = queries
            .child_of
            .get(parent)
            .map(|co| co.get())
            .unwrap_or(parent);
        let computed = queries.pane_rect.get(pane_entity).unwrap_or(self_computed);
        let computed = queries
            .stack_rect
            .get(parent)
            .map(|rect| &rect.0)
            .unwrap_or(computed);
        let tab = queries.tab_ancestor(parent);
        let presentation = queries.presentation(parent);
        let tab_active = presentation.visible();
        let previous = presentation == PresentationLayer::Retained;
        let stack_active = matches!(
            presentation,
            PresentationLayer::Current | PresentationLayer::Incoming
        );
        let renderable = webview_layout_is_renderable(
            computed.size,
            visibility,
            pending_reveal || transitioning,
        );
        if !windowed_page_is_visible(renderable, tab_active, stack_active, previous) {
            memory.sync_interactivity(&browsers, entity, generation, false);
            commands
                .entity(entity)
                .remove::<(PaneFrame, FocusRing, AllCorners)>();
            hidden.push(entity);
            continue;
        }
        let shown = !pending_reveal;
        let blocked = surface_blocked || transitioning || !shown || !presentation.interactive();
        if shown {
            visible.push(entity);
        } else {
            hidden.push(entity);
        }
        let host_window = queries.hierarchy.get(entity);
        let header_frame = host_window.and_then(|host_window| {
            queries.header_rect.iter().find_map(|(header, rect)| {
                if queries.hierarchy.get(header) == Some(host_window) {
                    WindowedFrameRect::from_node(rect)
                } else {
                    None
                }
            })
        });
        let visible_pane_count = tab
            .map(|tab| queries.focus.leaves(tab).len())
            .filter(|count| *count > 0)
            .unwrap_or_else(|| queries.focus.leaf_count().max(1));
        let Some(pane_frame) = WindowedFrameRect::from_node(computed) else {
            commands
                .entity(entity)
                .remove::<(PaneFrame, FocusRing, AllCorners)>();
            continue;
        };
        let scale = computed.scale();
        let frame = windowed_page_frame_rect(pane_frame, header_frame, visible_pane_count);
        let Some(logical) = PaneFrame::from_windowed(frame, scale) else {
            commands
                .entity(entity)
                .remove::<(PaneFrame, FocusRing, AllCorners)>();
            continue;
        };
        let became_visible = !memory.visible_pages.contains(&entity);
        let browser_ready = browsers.windowed_view_ready(&entity);
        let was_raised = memory.raised_frame.contains_key(&entity);
        let first_native_frame = browser_ready && !was_raised;
        let reveal = windowed_page_needs_reveal(became_visible, browser_ready, was_raised);
        let key = (
            frame.left.round() as i32,
            frame.top.round() as i32,
            frame.width.round() as i32,
            frame.height.round() as i32,
            scale.to_bits(),
        );
        if !browser_ready || memory.raised_frame.get(&entity) != Some(&key) {
            browsers.set_windowed_frame(
                &entity,
                frame.left,
                frame.top,
                frame.width,
                frame.height,
                scale,
            );
        }
        if reveal && shown {
            browsers.set_windowed_hidden(&entity, false);
        }
        memory.sync_interactivity(&browsers, entity, generation, !blocked);
        browsers.set_windowed_z_position(
            &entity,
            windowed_page_z_position(!capturing.is_empty() || transitioning, presentation),
        );
        let all_corners = windowed_page_all_corners(header_frame.is_none(), visible_pane_count);
        browsers.set_windowed_corner_radius(
            &entity,
            settings.layout.radius * scale,
            scale,
            all_corners,
        );
        let (focus_ring_width, focus_ring_rgb) = windowed_ring_for(
            parent,
            &focus,
            visible_pane_count,
            active_panes.agent_in_pane(pane_entity),
            &settings,
            scale,
        );
        browsers.set_windowed_focus_ring(&entity, focus_ring_width, scale, focus_ring_rgb);
        commands.entity(entity).insert((
            logical,
            FocusRing {
                width: focus_ring_width / scale,
                rgb: focus_ring_rgb,
            },
            AllCorners(all_corners),
        ));
        browsers.set_agent_badge(&entity, scale, None);
        let cover_rgb = clear_color.0.to_srgba();
        browsers.set_windowed_corner_cover(
            &entity,
            settings.layout.radius * scale,
            scale,
            all_corners,
            [cover_rgb.red, cover_rgb.green, cover_rgb.blue],
        );
        if browser_ready {
            if shown && ready && !paint_ready {
                commands
                    .entity(entity)
                    .insert(vmux_layout::hosted_page::SurfacePaintReady);
            }
            if shown && ready && !presentation_ready {
                commands
                    .entity(entity)
                    .insert(vmux_layout::hosted_page::SurfacePresentationReady);
            }
            if shown && host_window == focused_window.entity() && !blocked {
                memory.visible_frames.push(frame);
            }
            memory.raised_frame.insert(entity, key);
            if shown && (force_raise || became_visible || first_native_frame) {
                browsers.raise_windowed_to_front(&entity);
            }
            if reveal && shown {
                browsers.nudge_windowed_repaint(&entity);
            }
        }
    }
    let current_windowed: Vec<Entity> = visible.iter().chain(&hidden).copied().collect();
    memory
        .raised_frame
        .retain(|entity, _| current_windowed.contains(entity));
    memory
        .interactivity
        .retain(|entity, _| current_windowed.contains(entity));
    for entity in last_windowed_pages
        .iter()
        .filter(|entity| !current_windowed.contains(entity))
    {
        commands
            .entity(*entity)
            .try_remove::<(PaneFrame, FocusRing, AllCorners)>();
    }
    let newly_windowed: Vec<Entity> = current_windowed
        .iter()
        .copied()
        .filter(|entity| !last_windowed_pages.contains(entity))
        .collect();
    let ever_shown: Vec<Entity> = memory.raised_frame.keys().copied().collect();
    for entity in
        windowed_pages_to_hide(&hidden, &memory.visible_pages, &ever_shown, &newly_windowed)
    {
        browsers.set_windowed_hidden(&entity, true);
    }
    memory.visible_pages = visible;
    *last_windowed_pages = current_windowed;
    memory.visible_frames =
        NativeBridge::set_windowed_page_frames(std::mem::take(&mut memory.visible_frames));
}

fn windowed_page_needs_reveal(became_visible: bool, browser_ready: bool, was_raised: bool) -> bool {
    became_visible || (browser_ready && !was_raised)
}

fn windowed_page_is_visible(
    renderable: bool,
    tab_active: bool,
    stack_active: bool,
    stack_previous: bool,
) -> bool {
    renderable && tab_active && (stack_active || stack_previous)
}

fn windowed_page_z_position(layout_captures_pointer: bool, presentation: PresentationLayer) -> f64 {
    if layout_captures_pointer {
        return 0.0;
    }
    match presentation {
        PresentationLayer::Retained => 600.0,
        PresentationLayer::Incoming => 400.0,
        PresentationLayer::Current => 500.0,
        PresentationLayer::Hidden => 0.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowedFrameRect {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[derive(Default)]
struct FrameSyncMemory {
    raised_frame: std::collections::HashMap<Entity, (i32, i32, i32, i32, u32)>,
    interactivity: std::collections::HashMap<Entity, (u64, bool)>,
    visible_pages: Vec<Entity>,
    visible_frames: Vec<WindowedFrameRect>,
}

impl FrameSyncMemory {
    fn sync_interactivity(
        &mut self,
        browsers: &Browsers,
        entity: Entity,
        generation: Option<&BinIpcSurfaceGeneration>,
        interactive: bool,
    ) {
        let generation = generation.map_or(0, |generation| generation.0);
        let next = (generation, interactive);
        if self.interactivity.get(&entity) == Some(&next) {
            return;
        }
        browsers.set_windowed_interactive(&entity, interactive);
        self.interactivity.insert(entity, next);
    }
}

#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub(crate) struct FocusRing {
    pub(crate) width: f32,
    pub(crate) rgb: [f32; 3],
}

#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub(crate) struct PaneFrame {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct AllCorners(pub(crate) bool);

impl PaneFrame {
    fn from_windowed(frame: WindowedFrameRect, scale: f32) -> Option<Self> {
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }

        Some(Self {
            left: frame.left / scale,
            top: frame.top / scale,
            width: frame.width / scale,
            height: frame.height / scale,
        })
    }
}

impl WindowedFrameRect {
    fn from_node(rect: &ComputedNode) -> Option<Self> {
        if rect.is_empty() {
            return None;
        }
        let min = rect.min();
        Some(Self {
            left: min.x,
            top: min.y,
            width: rect.size.x,
            height: rect.size.y,
        })
    }

    pub(crate) fn right(self) -> f32 {
        self.left + self.width
    }

    pub(crate) fn bottom(self) -> f32 {
        self.top + self.height
    }
}

fn windowed_page_frame_rect(
    pane: WindowedFrameRect,
    header: Option<WindowedFrameRect>,
    visible_pane_count: usize,
) -> WindowedFrameRect {
    let Some(header) = header else {
        return pane;
    };
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
    focused_window: vmux_layout::window::FocusedWindow,
    mut last_raised_frame: Local<std::collections::HashMap<Entity, (i32, i32, i32, i32)>>,
) {
    let mut current = std::collections::HashSet::new();
    for (entity, host_window) in &layout_q {
        current.insert(entity);
        let window_entity = host_window.map(|h| h.0).or(focused_window.entity());
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
        if browsers.windowed_view_ready(&entity) {
            let key = (0, 0, w.round() as i32, h.round() as i32);
            let changed = last_raised_frame.insert(entity, key) != Some(key);
            if changed {
                browsers.raise_windowed_to_front(&entity);
            }
        }
    }
    last_raised_frame.retain(|entity, _| current.contains(entity));
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CommandBarWindowedFrame {
    pub(crate) left_px: f32,
    pub(crate) top_px: f32,
    pub(crate) width_px: f32,
    pub(crate) height_px: f32,
}

const COMMAND_BAR_NATIVE_RADIUS_PX: f32 = 16.0;
fn publish_command_bar_route(owns_input: bool, frame: Option<CommandBarWindowedFrame>, scale: f32) {
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

fn command_bar_windowed_view_should_render_hidden(
    display: Display,
    visibility: Visibility,
) -> bool {
    display != Display::None && visibility == Visibility::Hidden
}

fn sync_windowed_command_bar(
    browsers: NonSend<Browsers>,
    modal_q: Query<
        (
            Entity,
            &Node,
            &Visibility,
            Has<KeyboardOwner>,
            Has<WebviewWindowed>,
            Has<vmux_ecs::overlay::OverlayShownInline>,
            Option<&HostWindow>,
            Option<&CommandBarNativeSize>,
        ),
        (With<WindowOverlay>, With<CommandBar>),
    >,
    native_size_changed: Query<(), Changed<CommandBarNativeSize>>,
    windows: Query<&Window>,
    focused_window: vmux_layout::window::FocusedWindow,
    mut was_open: Local<bool>,
) {
    let matched = modal_q.single();
    let Ok((
        entity,
        node,
        visibility,
        has_keyboard_target,
        is_windowed,
        shown_inline,
        host_window,
        native_size,
    )) = matched
    else {
        publish_command_bar_route(false, None, 1.0);
        *was_open = false;
        return;
    };
    if shown_inline {
        publish_command_bar_route(false, None, 1.0);
        if is_windowed {
            browsers.set_windowed_focus(&entity, false);
            hide_windowed_command_bar(&browsers, entity);
        }
        *was_open = false;
        return;
    }
    let state = OverlayState::resolve(node.display, *visibility, has_keyboard_target, shown_inline);
    let open = state.is_shown();
    let owns_input = state.owns_input();
    let render_hidden = command_bar_windowed_view_should_render_hidden(node.display, *visibility);
    if !open && !render_hidden {
        publish_command_bar_route(owns_input, None, 1.0);
        if is_windowed {
            browsers.set_windowed_focus(&entity, false);
            hide_windowed_command_bar(&browsers, entity);
        }
        *was_open = false;
        return;
    }
    if !browsers.has_browser(entity) {
        publish_command_bar_route(owns_input, None, 1.0);
        return;
    }
    let window_entity = host_window.map(|h| h.0).or(focused_window.entity());
    let Some(window_entity) = window_entity else {
        publish_command_bar_route(owns_input, None, 1.0);
        if is_windowed {
            hide_windowed_command_bar(&browsers, entity);
        }
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        publish_command_bar_route(owns_input, None, 1.0);
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
        publish_command_bar_route(owns_input, frame, scale);
        *was_open = open;
        return;
    }
    if render_hidden {
        publish_command_bar_route(owns_input, None, scale);
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
        publish_command_bar_route(owns_input, None, scale);
        hide_windowed_command_bar(&browsers, entity);
        return;
    };
    publish_command_bar_route(owns_input, Some(frame), scale);

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
    browsers.set_windowed_corner_radius(&entity, COMMAND_BAR_NATIVE_RADIUS_PX * scale, scale, true);
    browsers.set_windowed_hidden(&entity, false);
    browsers.raise_windowed_to_front(&entity);
    browsers.set_windowed_focus(&entity, true);
    if !*was_open || native_size_changed.contains(entity) {
        browsers.nudge_windowed_repaint(&entity);
        *was_open = true;
    }
}

#[cfg(target_os = "macos")]
fn flush_command_bar_pointer(
    browsers: NonSend<Browsers>,
    modal_q: Query<Entity, (With<WindowOverlay>, With<WebviewWindowed>, With<CommandBar>)>,
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
fn flush_command_bar_pointer() {}
fn apply_repaint_nudge(browsers: NonSend<Browsers>, ready: Query<Entity, Changed<PageReady>>) {
    for entity in &ready {
        browsers.nudge_windowed_repaint(&entity);
    }
}

fn sync_webview_resize(
    browsers: NonSend<Browsers>,
    webviews: Query<(Entity, &WebviewSize), (With<Browser>, Without<WindowOverlay>)>,
    host_window: Query<&HostWindow>,
    windows: Query<&Window>,
    focused_window: vmux_layout::window::FocusedWindow,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut last_entries: Local<std::collections::HashMap<u64, (Vec2, f32)>>,
    mut window_resized: MessageReader<WindowResized>,
    mut pending_create: Local<std::collections::HashSet<Entity>>,
) {
    let force = window_resized.read().count() > 0;
    if force {
        last_entries.clear();
    }
    let mut pushed_any = false;
    let mut awaiting_create = std::collections::HashSet::new();
    let mut seen = std::collections::HashSet::new();
    for (entity, size) in webviews.iter() {
        if !browsers.has_browser(entity) {
            awaiting_create.insert(entity);
            continue;
        }
        let key = entity.to_bits();
        seen.insert(key);
        let window_entity = host_window
            .get(entity)
            .ok()
            .map(|h| h.0)
            .or(focused_window.entity());
        let device_scale_factor = window_entity
            .and_then(|e| windows.get(e).ok())
            .map(|w| w.resolution.scale_factor())
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(1.0);
        if last_entries.get(&key).is_some_and(|(last_size, scale)| {
            *last_size == size.0 && (*scale - device_scale_factor).abs() < 0.01
        }) {
            continue;
        }
        browsers.resize(&entity, size.0, device_scale_factor);
        pushed_any = true;
        last_entries.insert(key, (size.0, device_scale_factor));
    }
    last_entries.retain(|key, _| seen.contains(key));
    let newly_awaiting = awaiting_create
        .iter()
        .any(|entity| !pending_create.contains(entity));
    *pending_create = awaiting_create;
    if windowed_reconcile_should_wake(pushed_any, newly_awaiting)
        && let Some(proxy) = proxy.as_ref()
    {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn windowed_reconcile_should_wake(pushed_any: bool, newly_awaiting: bool) -> bool {
    pushed_any || newly_awaiting
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
            Has<WindowOverlay>,
            Has<KeyboardOwner>,
            Has<WebviewWindowed>,
            Has<LayoutCef>,
            Has<BookmarkTextInputActive>,
            Has<BookmarkContextMenuActive>,
            Has<CommandBarPanelActive>,
            Has<Header>,
            Has<SideSheet>,
        ),
        With<WebviewSource>,
    >,
    windows: Query<&Window>,
    host_windows: Query<&HostWindow>,
    focused_window: vmux_layout::window::FocusedWindow,
    focus: vmux_layout::stack::FocusedStack,
    presentation: WindowFrameQueries,
    stacks: Query<(), With<Stack>>,

    mut ready: Local<Vec<Entity>>,
    mut auxiliary: Local<Vec<Entity>>,
    mut last_active: Local<Option<Entity>>,
    mut last_ready_set: Local<Vec<Entity>>,
) {
    ready.clear();
    let mut layout_shells = Vec::new();
    let mut modal_keyboard_target = None;
    let mut layout_keyboard_target = None;
    let mut page_keyboard_target = None;
    let focused_window = focused_window.entity();
    let window = focused_window.and_then(|window| windows.get(window).ok());
    let window_visible = window.is_some_and(|window| window.visible);
    let window_focused = window.is_some_and(|window| window.focused);
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
        is_header,
        is_side_sheet,
    ) in webviews.iter()
    {
        if !browsers.has_browser(entity) {
            continue;
        }
        if focused_window.is_some()
            && host_windows
                .get(entity)
                .is_ok_and(|host| Some(host.0) != focused_window)
        {
            browsers.set_osr_hidden(&entity);
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
            } else if !is_layout && !is_header && !is_side_sheet && has_keyboard_target {
                page_keyboard_target = Some(entity);
            }
        } else if keep_hidden_osr_webview_warm(is_modal, is_windowed, window_visible) {
            browsers.set_osr_not_hidden(&entity);
        } else {
            browsers.set_osr_hidden(&entity);
        }
    }
    if ready.is_empty() {
        if last_active.is_some() || !last_ready_set.is_empty() {
            browsers.sync_osr_focus_to_active_pane(None, &[]);
            *last_active = None;
            last_ready_set.clear();
        }
        return;
    }
    ready.sort_by_key(|e| e.to_bits());
    let fallback = focus.stack.is_none().then_some(ready[0]);
    let active = layout_keyboard_target.or_else(|| {
        choose_osr_active_webview(modal_keyboard_target, page_keyboard_target, fallback)
    });

    if !window_visible || !window_focused {
        if last_active.is_some() || *last_ready_set != *ready {
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
        browsers.sync_osr_focus_to_active_pane(active, auxiliary.as_slice());
        *last_active = active;
        last_ready_set.clone_from(&ready);
    }
    for &e in ready.iter() {
        let mut parent_is_stack = false;
        let mut pane_is_leaf = false;
        let mut is_active = false;
        let mut is_prev = false;

        if let Ok(parent) = presentation.child_of.get(e).map(|co| co.get()) {
            parent_is_stack = stacks.contains(parent);
            if parent_is_stack
                && let Ok(pane) = presentation.child_of.get(parent).map(|co| co.get())
            {
                pane_is_leaf = presentation.focus.is_leaf(pane);
                if pane_is_leaf {
                    let layer = presentation.presentation(e);
                    is_active = matches!(
                        layer,
                        PresentationLayer::Current | PresentationLayer::Incoming
                    );
                    is_prev = layer == PresentationLayer::Retained;
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
    fallback: Option<Entity>,
) -> Option<Entity> {
    if modal_keyboard_target.is_some_and(|(_, is_windowed)| is_windowed) {
        None
    } else {
        modal_keyboard_target
            .map(|(entity, _)| entity)
            .or(active_stack)
            .or(fallback)
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tests::test_app_settings_with_radius;
    use bevy::ecs::system::RunSystemOnce;

    use vmux_history::LastActivatedAt;

    #[test]
    fn the_user_ring_outranks_an_agent_ring_on_the_same_pane() {
        let mut world = World::new();
        let stack = world.spawn_empty().id();
        let settings = test_app_settings_with_radius(0.0);
        let user = &settings.layout.focus_ring.color;

        let agent = (
            "claude",
            ActiveStack {
                tab: None,
                pane: None,
                stack: Some(stack),
            },
        );
        let unfocused = ActiveStack::default();

        let (width, rgb) = windowed_ring_for(stack, &unfocused, 2, Some(agent), &settings, 1.0);
        assert!(width > 0.0, "an agent's active pane draws a ring");
        assert_ne!(
            rgb,
            [user.r, user.g, user.b],
            "and it is not the user's colour"
        );

        let focused = ActiveStack {
            stack: Some(stack),
            ..Default::default()
        };
        let (width, rgb) = windowed_ring_for(stack, &focused, 2, Some(agent), &settings, 1.0);
        assert!(width > 0.0);
        assert_eq!(rgb, [user.r, user.g, user.b]);

        let (width, _) = windowed_ring_for(stack, &focused, 1, None, &settings, 1.0);
        assert_eq!(width, 0.0);
    }

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
    fn browser_created_after_layout_visibility_is_revealed_on_its_first_native_frame() {
        assert!(windowed_page_needs_reveal(false, true, false));
        assert!(!windowed_page_needs_reveal(false, false, false));
        assert!(!windowed_page_needs_reveal(false, true, true));
    }

    #[test]
    fn windowed_visibility_uses_layout_state() {
        assert!(windowed_page_is_visible(true, true, true, false));
        assert!(windowed_page_is_visible(true, true, false, true));
        assert!(!windowed_page_is_visible(false, true, true, true));
        assert!(!windowed_page_is_visible(true, false, true, true));
        assert!(!windowed_page_is_visible(true, true, false, false));
    }

    #[test]
    fn windowed_page_stays_above_layout_until_layout_captures_pointer() {
        assert_eq!(
            windowed_page_z_position(false, PresentationLayer::Current),
            500.0
        );
        assert_eq!(
            windowed_page_z_position(true, PresentationLayer::Current),
            0.0
        );
    }

    #[test]
    fn retained_tab_stays_above_its_incoming_replacement() {
        assert!(
            windowed_page_z_position(false, PresentationLayer::Retained)
                > windowed_page_z_position(false, PresentationLayer::Incoming)
        );
    }

    #[test]
    fn page_inherits_the_layout_projection() {
        let mut app = App::new();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), PresentationLayer::Incoming))
            .id();
        let page = app.world_mut().spawn((Browser, ChildOf(tab))).id();

        let layer = app
            .world_mut()
            .run_system_once(move |queries: WindowFrameQueries| queries.presentation(page))
            .unwrap();

        assert_eq!(layer, PresentationLayer::Incoming);
    }

    #[test]
    fn retained_ancestor_overrides_an_incoming_descendant() {
        let mut app = App::new();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), PresentationLayer::Retained))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), PresentationLayer::Incoming, ChildOf(tab)))
            .id();
        let page = app.world_mut().spawn((Browser, ChildOf(stack))).id();

        let layer = app
            .world_mut()
            .run_system_once(move |queries: WindowFrameQueries| queries.presentation(page))
            .unwrap();

        assert_eq!(layer, PresentationLayer::Retained);
    }

    #[test]
    fn hidden_or_collapsed_webviews_do_not_render() {
        assert!(!webview_layout_is_renderable(
            Vec2::ZERO,
            Some(&Visibility::Visible),
            false
        ));
        assert!(!webview_layout_is_renderable(
            Vec2::new(100.0, 0.0),
            Some(&Visibility::Visible),
            false
        ));
        assert!(!webview_layout_is_renderable(
            Vec2::new(100.0, 20.0),
            Some(&Visibility::Hidden),
            false
        ));
        assert!(webview_layout_is_renderable(
            Vec2::new(100.0, 20.0),
            Some(&Visibility::Visible),
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

        let window = app.world_mut().spawn_empty().id();
        let glass = app
            .world_mut()
            .spawn((
                VmuxWindow,
                HostWindow(window),
                ComputedNode {
                    size: Vec2::new(1200.0, 800.0),
                    ..default()
                },
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
                WebviewSize(Vec2::ONE),
                ChildOf(glass),
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                LastActivatedAt::new(1),
                PresentationLayer::Current,
                ChildOf(glass),
            ))
            .id();
        let pane = app
            .world_mut()
            .spawn((
                Pane,
                ComputedNode {
                    size: Vec2::new(1200.0, 740.0),
                    ..default()
                },
                ChildOf(tab),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(1),
                PresentationLayer::Current,
                ChildOf(pane),
            ))
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
                WebviewSize(Vec2::ONE),
                ChildOf(stack),
            ))
            .id();

        app.update();

        let layout_z = app.world().get::<Transform>(layout).unwrap().translation.z;
        let page_z = app.world().get::<Transform>(page).unwrap().translation.z;

        assert!(layout_z > page_z);
        assert!(app.world().get::<PagePresented>(page).is_some());
    }

    #[test]
    fn pending_tab_reveal_keeps_the_previous_osr_surface_presented() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .add_systems(Update, sync_children_to_ui);

        let window = app.world_mut().spawn_empty().id();
        let glass = app
            .world_mut()
            .spawn((
                VmuxWindow,
                HostWindow(window),
                ComputedNode {
                    size: Vec2::new(1200.0, 800.0),
                    ..default()
                },
            ))
            .id();
        let previous_tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                LastActivatedAt::new(1),
                PresentationLayer::Retained,
                ChildOf(glass),
            ))
            .id();
        let previous_pane = app
            .world_mut()
            .spawn((
                Pane,
                ComputedNode {
                    size: Vec2::new(1200.0, 740.0),
                    ..default()
                },
                ChildOf(previous_tab),
            ))
            .id();
        let previous_stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(1),
                PresentationLayer::Current,
                ChildOf(previous_pane),
            ))
            .id();
        let previous_page = app
            .world_mut()
            .spawn((
                Browser,
                Transform::default(),
                ComputedNode {
                    size: Vec2::new(1200.0, 740.0),
                    ..default()
                },
                WebviewSize(Vec2::ONE),
                ChildOf(previous_stack),
            ))
            .id();
        let incoming_tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                LastActivatedAt::new(2),
                PresentationLayer::Incoming,
                vmux_layout::presentation::Transition::retain(previous_tab),
                ChildOf(glass),
            ))
            .id();
        let incoming_pane = app
            .world_mut()
            .spawn((
                Pane,
                ComputedNode {
                    size: Vec2::new(1200.0, 740.0),
                    ..default()
                },
                ChildOf(incoming_tab),
            ))
            .id();
        let incoming_stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt::new(2),
                PresentationLayer::Current,
                ChildOf(incoming_pane),
            ))
            .id();
        app.world_mut().spawn((
            Browser,
            Transform::default(),
            ComputedNode {
                size: Vec2::new(1200.0, 740.0),
                ..default()
            },
            WebviewSize(Vec2::ONE),
            ChildOf(incoming_stack),
        ));

        app.update();

        assert!(app.world().get::<PagePresented>(previous_page).is_some());
        assert!(app.world().get::<Transform>(previous_page).unwrap().scale.x > 0.5);
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
            choose_osr_active_webview(Some((modal, false)), Some(pane), Some(pane)),
            Some(modal)
        );
    }

    #[test]
    fn windowed_command_bar_modal_suppresses_osr_focus_targets() {
        let pane = Entity::from_bits(1);
        let modal = Entity::from_bits(2);

        assert_eq!(
            choose_osr_active_webview(Some((modal, true)), Some(pane), Some(pane)),
            None
        );
    }

    #[test]
    fn focused_stack_without_a_ready_surface_does_not_focus_another_page() {
        let fallback = Entity::from_bits(1);

        assert_eq!(choose_osr_active_webview(None, None, None), None);
        assert_eq!(
            choose_osr_active_webview(None, None, Some(fallback)),
            Some(fallback)
        );
    }

    #[test]
    fn open_command_bar_is_exclusive_cef_keyboard_target() {
        let mut app = App::new();
        app.add_plugins(vmux_layout::LayoutContractPlugin)
            .add_systems(Update, sync_keyboard_target);
        let page = app.world_mut().spawn((Browser, KeyboardOwner)).id();
        let modal = app
            .world_mut()
            .spawn((
                Browser,
                WindowOverlay,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                KeyboardOwner,
            ))
            .id();

        app.update();

        assert!(app.world().get::<KeyboardOwner>(modal).is_some());
        assert!(app.world().get::<KeyboardOwner>(page).is_none());
    }

    #[test]
    fn only_the_focused_stack_owns_page_keyboard_input() {
        let mut app = App::new();
        app.add_plugins(vmux_layout::LayoutContractPlugin)
            .add_systems(Update, sync_keyboard_target);
        let focused_stack = app
            .world_mut()
            .spawn((Stack::default(), PresentationLayer::Current))
            .id();
        let other_stack = app
            .world_mut()
            .spawn((Stack::default(), PresentationLayer::Current))
            .id();
        app.world_mut().spawn(
            ActiveStack {
                stack: Some(focused_stack),
                ..Default::default()
            }
            .local_bundle(),
        );
        let focused_page = app
            .world_mut()
            .spawn((Browser, ChildOf(focused_stack)))
            .id();
        let other_page = app
            .world_mut()
            .spawn((Browser, KeyboardOwner, ChildOf(other_stack)))
            .id();

        app.update();

        assert!(app.world().get::<KeyboardOwner>(focused_page).is_some());
        assert!(app.world().get::<KeyboardOwner>(other_page).is_none());
    }

    #[test]
    fn retained_page_releases_keyboard_while_the_incoming_stack_loads() {
        let mut app = App::new();
        app.add_plugins(vmux_layout::LayoutContractPlugin)
            .add_systems(Update, sync_keyboard_target);
        let retained_stack = app
            .world_mut()
            .spawn((Stack::default(), PresentationLayer::Retained))
            .id();
        let incoming_stack = app
            .world_mut()
            .spawn((Stack::default(), PresentationLayer::Incoming))
            .id();
        app.world_mut().spawn(
            ActiveStack {
                stack: Some(incoming_stack),
                ..Default::default()
            }
            .local_bundle(),
        );
        let retained_page = app
            .world_mut()
            .spawn((Browser, KeyboardOwner, ChildOf(retained_stack)))
            .id();
        let incoming_page = app
            .world_mut()
            .spawn((Browser, ChildOf(incoming_stack)))
            .id();

        app.update();

        assert!(app.world().get::<KeyboardOwner>(retained_page).is_none());
        assert!(app.world().get::<KeyboardOwner>(incoming_page).is_none());
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

        let frame = windowed_page_frame_rect(pane, Some(header), 1);

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
    fn split_pane_windowed_frame_keeps_the_gap_the_layout_gave_it() {
        let pane = WindowedFrameRect {
            left: 610.2,
            top: 104.2,
            width: 560.6,
            height: 640.0,
        };
        let header = WindowedFrameRect {
            left: 150.0,
            top: 24.0,
            width: 1020.0,
            height: 72.2,
        };

        let frame = windowed_page_frame_rect(pane, Some(header), 2);

        assert_eq!(
            frame,
            WindowedFrameRect {
                left: 611.0,
                top: 105.0,
                width: 559.0,
                height: 639.0,
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
    fn published_route_is_the_only_source_of_command_bar_hit_state() {
        let frame = CommandBarWindowedFrame {
            left_px: 100.0,
            top_px: 50.0,
            width_px: 200.0,
            height_px: 100.0,
        };

        let before = CommandBarRoute::current().generation;
        publish_command_bar_route(true, Some(frame), 2.0);
        let published = CommandBarRoute::current();
        assert_eq!(published.generation, before.wrapping_add(1));
        assert_eq!(published.scale, 2.0);

        publish_command_bar_route(false, Some(frame), 1.0);
        assert!(!CommandBarRoute::current().owns_input);
    }

    #[test]
    fn revealing_command_bar_owns_input_while_its_view_stays_parked() {
        let revealing = OverlayState::resolve(Display::Flex, Visibility::Hidden, true, false);

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
            Visibility::Visible
        ));
    }

    #[test]
    fn windowed_reconcile_wakes_until_hosted_pages_are_sized() {
        assert!(windowed_reconcile_should_wake(true, false));
        assert!(windowed_reconcile_should_wake(false, true));
        assert!(!windowed_reconcile_should_wake(false, false));
    }
}
