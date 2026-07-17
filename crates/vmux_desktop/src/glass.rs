use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_cef_core::prelude::WebviewDirtyRect;
use vmux_layout::cef::LayoutCef;
use vmux_layout::scene::InteractionMode;

/// How long to keep re-asserting activation after reveal before giving up, so a degenerate case
/// (activation never confirms) cannot wake the loop forever.
const ACTIVATION_RETRY_BUDGET: Duration = Duration::from_secs(3);

pub(crate) struct GlassPlugin;

impl Plugin for GlassPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(bevy_cef::prelude::NativeOverlayPresenter(Some(
            native_overlay_presenter(),
        )))
        .init_non_send::<GlassState>()
        .init_non_send::<LayoutOverlay>()
        .init_non_send::<CommandBarOverlay>()
        .add_systems(PreUpdate, install_window_glass)
        .add_systems(
            Update,
            (
                sync_window_glass_visibility,
                keep_window_surface_layer_transparent,
            ),
        )
        .add_systems(
            Update,
            handle_toggle_fullscreen_command.in_set(vmux_command::ReadAppCommands),
        )
        .add_systems(
            Last,
            (
                reveal_window_after_layout_ready,
                restore_fullscreen_after_reveal,
                ensure_window_active_after_reveal,
                sync_layout_overlay,
                sync_command_bar_overlay,
            )
                .chain(),
        );
    }
}

#[derive(Default)]
struct GlassState {
    installed: bool,
    visible: bool,
    revealed: bool,
    revealed_at: Option<Instant>,
    active_confirmed: bool,
    _glass: Option<objc2::rc::Retained<objc2_app_kit::NSGlassEffectView>>,
    _backdrop_window: Option<objc2::rc::Retained<objc2_app_kit::NSPanel>>,
    _parent_window: Option<objc2::rc::Retained<objc2_app_kit::NSWindow>>,
}

fn install_window_glass(
    mut state: NonSendMut<GlassState>,
    window: Query<Entity, With<bevy::window::PrimaryWindow>>,
) {
    use bevy::winit::WINIT_WINDOWS;
    use objc2::{ClassType, MainThreadMarker, MainThreadOnly, rc::Retained, runtime::AnyClass};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSGlassEffectView,
        NSGlassEffectViewStyle, NSPanel, NSView, NSWindowCollectionBehavior, NSWindowOrderingMode,
        NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    if state.installed {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Ok(entity) = window.single() else {
        return;
    };
    let ns_view = WINIT_WINDOWS.with_borrow(|windows| {
        let id = windows.entity_to_winit.get(&entity)?;
        let wrapper = windows.windows.get(id)?;
        let handle = wrapper.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::AppKit(h) => Some(h.ns_view),
            _ => None,
        }
    });
    let Some(ns_view) = ns_view else {
        return;
    };
    let content: &NSView = unsafe { &*ns_view.as_ptr().cast::<NSView>() };
    let Some(parent_window) = content.window() else {
        return;
    };
    if AnyClass::get(c"NSGlassEffectView").is_none() {
        warn!("glass: NSGlassEffectView unavailable (needs macOS 26+)");
        state.installed = true;
        return;
    }
    let frame = parent_window.frame();
    let backdrop_window = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        frame,
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    let clear_color = NSColor::clearColor();
    let backdrop: &objc2_app_kit::NSWindow = backdrop_window.as_super();
    backdrop.setOpaque(false);
    backdrop.setBackgroundColor(Some(&clear_color));
    backdrop.setHasShadow(false);
    backdrop.setIgnoresMouseEvents(true);
    backdrop.setCanHide(false);
    backdrop.setHidesOnDeactivate(false);
    backdrop_window.setFloatingPanel(false);
    backdrop_window.setBecomesKeyOnlyIfNeeded(true);
    backdrop.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    let glass: Retained<NSGlassEffectView> = NSGlassEffectView::new(mtm);
    glass.setStyle(NSGlassEffectViewStyle::Clear);
    glass.setTintColor(Some(&NSColor::clearColor()));
    let glass_view: &NSView = &glass;
    glass_view.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(frame.size.width, frame.size.height),
    ));
    glass_view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    backdrop.setContentView(Some(glass_view));
    unsafe {
        parent_window.addChildWindow_ordered(backdrop, NSWindowOrderingMode::Below);
    }
    state.visible = true;
    state._glass = Some(glass);
    state._backdrop_window = Some(backdrop_window);
    state._parent_window = Some(parent_window);
    state.installed = true;
    info!("glass: NSGlassEffectView installed in nonactivating child-window backdrop");
}

fn reveal_window_after_layout_ready(
    mut state: NonSendMut<GlassState>,
    mut window: Query<(Entity, &mut Window), With<bevy::window::PrimaryWindow>>,
    status: Res<crate::boot_status::SplashStatus>,
) {
    if state.revealed || !state.installed || !status.reveal_ready {
        return;
    }
    let Ok((_, mut window)) = window.single_mut() else {
        return;
    };
    window.visible = true;
    state.revealed = true;
    state.revealed_at = Some(Instant::now());
}

/// After reveal, apply the persisted fullscreen intent once: enter native
/// fullscreen if it was saved, then mark restore complete so geometry capture
/// can begin. Consumes [`crate::window_state::PendingFullscreenRestore`].
fn restore_fullscreen_after_reveal(
    state: NonSend<GlassState>,
    pending: Option<Res<crate::window_state::PendingFullscreenRestore>>,
    mut commands: Commands,
) {
    use objc2_app_kit::NSWindowStyleMask;

    let Some(pending) = pending else {
        return;
    };
    if !state.revealed {
        return;
    }
    if pending.0
        && let Some(parent_window) = &state._parent_window
        && !parent_window
            .styleMask()
            .contains(NSWindowStyleMask::FullScreen)
    {
        parent_window.toggleFullScreen(None);
    }
    commands.remove_resource::<crate::window_state::PendingFullscreenRestore>();
    commands.insert_resource(crate::window_state::WindowRestoreComplete);
}

fn should_attempt_activation(
    revealed: bool,
    active_confirmed: bool,
    elapsed_since_reveal: Option<Duration>,
) -> bool {
    if !revealed || active_confirmed {
        return false;
    }
    match elapsed_since_reveal {
        Some(elapsed) => elapsed < ACTIVATION_RETRY_BUDGET,
        None => true,
    }
}

/// The reveal frame shows the window, but the app is still in the background (the splash is a
/// nonactivating panel). Activation is async, so retry it each frame until the app is active and
/// the window is key, waking the loop in between so the retry actually runs.
fn ensure_window_active_after_reveal(
    mut state: NonSendMut<GlassState>,
    window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    let elapsed = state.revealed_at.map(|at| at.elapsed());
    if !should_attempt_activation(state.revealed, state.active_confirmed, elapsed) {
        return;
    }
    let Ok(entity) = window.single() else {
        return;
    };
    if crate::background_lifecycle::ensure_native_window_active(entity) {
        state.active_confirmed = true;
    } else if let Some(proxy) = proxy {
        let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
    }
}

fn glass_backdrop_visible(mode: InteractionMode) -> bool {
    mode == InteractionMode::User
}

/// Toggle native macOS fullscreen when the `ToggleFullscreen` command fires (Ctrl+Cmd+F).
/// vmux hides the native window buttons, so this is the entry point into/out of fullscreen.
fn handle_toggle_fullscreen_command(
    state: NonSend<GlassState>,
    mut reader: MessageReader<vmux_command::AppCommand>,
) {
    use vmux_command::{AppCommand, LayoutCommand, WindowCommand};

    let toggle = reader.read().any(|cmd| {
        matches!(
            cmd,
            AppCommand::Layout(LayoutCommand::Window(WindowCommand::ToggleFullscreen))
        )
    });
    if toggle && let Some(parent_window) = &state._parent_window {
        parent_window.toggleFullScreen(None);
    }
}

fn sync_window_glass_visibility(
    mut state: NonSendMut<GlassState>,
    mode: Res<InteractionMode>,
    mut clear_color: ResMut<ClearColor>,
    mut window_q: Query<&mut bevy::window::Window, With<bevy::window::PrimaryWindow>>,
    terminal_focus_q: Query<
        (),
        (
            With<vmux_terminal::Terminal>,
            With<bevy_cef::prelude::CefKeyboardTarget>,
        ),
    >,
    modal_open_q: Query<
        (&Node, Has<bevy_cef::prelude::CefKeyboardTarget>),
        With<vmux_layout::window::Modal>,
    >,
    mut window_fullscreen: ResMut<crate::window_state::WindowFullscreen>,
) {
    use objc2::ClassType;
    use objc2_app_kit::NSWindowStyleMask;

    let bevy_fullscreen = window_q
        .single()
        .map(|w| {
            matches!(
                w.mode,
                bevy::window::WindowMode::BorderlessFullscreen(_)
                    | bevy::window::WindowMode::Fullscreen(..)
            )
        })
        .unwrap_or(false);
    let native_fullscreen = state
        ._parent_window
        .as_ref()
        .is_some_and(|w| w.styleMask().contains(NSWindowStyleMask::FullScreen));
    let fullscreen = bevy_fullscreen || native_fullscreen;

    if window_fullscreen.0 != fullscreen {
        window_fullscreen.0 = fullscreen;
    }

    let [r, g, b] = vmux_layout::window::WINDOW_BACKGROUND_SRGB;
    let want_clear = if fullscreen {
        Color::srgb(r, g, b)
    } else {
        Color::NONE
    };
    if clear_color.0 != want_clear {
        clear_color.0 = want_clear;
    }

    let terminal_focused = !terminal_focus_q.is_empty();
    let command_bar_open = vmux_layout::command_bar::handler::is_command_bar_open(&modal_open_q);
    crate::native_keyboard::set_escape_exits_fullscreen(
        fullscreen && !terminal_focused && !command_bar_open,
    );

    if crate::native_keyboard::take_exit_fullscreen_request() {
        if native_fullscreen {
            if let Some(parent_window) = &state._parent_window {
                parent_window.toggleFullScreen(None);
            }
        } else if let Ok(mut window) = window_q.single_mut() {
            window.mode = bevy::window::WindowMode::Windowed;
        }
        return;
    }

    let visible = glass_backdrop_visible(*mode) && !fullscreen;
    if let (Some(backdrop_window), Some(parent_window)) =
        (&state._backdrop_window, &state._parent_window)
    {
        let backdrop_window: &objc2_app_kit::NSWindow = backdrop_window.as_super();
        backdrop_window.setFrame_display(parent_window.frame(), false);
    }
    if state.visible == visible {
        return;
    }
    if let Some(glass) = &state._glass {
        let glass_view: &objc2_app_kit::NSView = glass;
        glass_view.setHidden(!visible);
    }
    state.visible = visible;
}

#[derive(Default)]
struct LayoutOverlay {
    layer: Option<objc2::rc::Retained<objc2_quartz_core::CALayer>>,
    shown: bool,
    held: Option<bevy_cef_core::prelude::AcceleratedFrame>,
}

#[derive(Default)]
struct CommandBarOverlay {
    view: Option<objc2::rc::Retained<objc2_app_kit::NSView>>,
    shown: bool,
    /// Keeps the currently-displayed IOSurface alive while it's the overlay layer's contents.
    held: Option<bevy_cef_core::prelude::AcceleratedFrame>,
}

#[derive(Clone)]
struct SendLayer(objc2::rc::Retained<objc2_quartz_core::CALayer>);

unsafe impl Send for SendLayer {}
unsafe impl Sync for SendLayer {}

#[derive(Clone)]
struct MetalOverlayContext {
    device: objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLDevice>>,
    queue: objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLCommandQueue>>,
}

unsafe impl Send for MetalOverlayContext {}
unsafe impl Sync for MetalOverlayContext {}

#[derive(Clone)]
struct MetalOverlaySurface {
    io_surface: objc2_core_foundation::CFRetained<objc2_io_surface::IOSurfaceRef>,
    texture: objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>>,
}

unsafe impl Send for MetalOverlaySurface {}
unsafe impl Sync for MetalOverlaySurface {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum OverlayDamage {
    #[default]
    None,
    Full,
    Rects(Vec<WebviewDirtyRect>),
}

impl OverlayDamage {
    fn from_frame(width: u32, height: u32, dirty: &[WebviewDirtyRect]) -> Self {
        if dirty.is_empty() {
            Self::Full
        } else {
            overlay_damage_from_rects(width, height, dirty.iter().copied())
        }
    }

    fn union(self, other: Self, width: u32, height: u32) -> Self {
        match (self, other) {
            (Self::Full, _) | (_, Self::Full) => Self::Full,
            (Self::None, damage) | (damage, Self::None) => damage,
            (Self::Rects(mut left), Self::Rects(right)) => {
                left.extend(right);
                overlay_damage_from_rects(width, height, left)
            }
        }
    }

    fn into_frame_dirty(self) -> Vec<WebviewDirtyRect> {
        match self {
            Self::None | Self::Full => Vec::new(),
            Self::Rects(rects) => rects,
        }
    }
}

struct MetalOverlaySlot {
    surface: MetalOverlaySurface,
    initialized: bool,
    stale: OverlayDamage,
}

struct MetalOverlaySwapchain {
    generation: u64,
    width: u32,
    height: u32,
    format: bevy_cef_core::prelude::AcceleratedPixelFormat,
    slots: [MetalOverlaySlot; 2],
    front: Option<usize>,
}

struct MetalSourceTexture {
    io_surface: usize,
    width: u32,
    height: u32,
    format: bevy_cef_core::prelude::AcceleratedPixelFormat,
    texture: objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>>,
    last_used: u64,
}

unsafe impl Send for MetalSourceTexture {}
unsafe impl Sync for MetalSourceTexture {}

#[derive(Clone)]
struct MetalOverlayInFlight {
    generation: u64,
    target: usize,
    damage: OverlayDamage,
}

#[derive(Default)]
struct NativeOverlayPresentState {
    layers: HashMap<Entity, SendLayer>,
    held: HashMap<Entity, bevy_cef_core::prelude::AcceleratedFrame>,
    metal: Option<MetalOverlayContext>,
    swapchains: HashMap<Entity, MetalOverlaySwapchain>,
    source_textures: HashMap<Entity, Vec<MetalSourceTexture>>,
    in_flight: HashMap<Entity, MetalOverlayInFlight>,
    next_generation: u64,
    source_texture_clock: u64,
}

static NATIVE_OVERLAY_PRESENT_STATE: LazyLock<Mutex<NativeOverlayPresentState>> =
    LazyLock::new(|| Mutex::new(NativeOverlayPresentState::default()));
static NATIVE_OVERLAY_MAILBOX: LazyLock<
    Mutex<HashMap<Entity, Option<bevy_cef_core::prelude::AcceleratedFrame>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));
static NATIVE_OVERLAY_PRESENT_SCHEDULED: AtomicBool = AtomicBool::new(false);

fn overlay_rect_union(left: WebviewDirtyRect, right: WebviewDirtyRect) -> Option<WebviewDirtyRect> {
    let left_right = left.x.saturating_add(left.width);
    let left_bottom = left.y.saturating_add(left.height);
    let right_right = right.x.saturating_add(right.width);
    let right_bottom = right.y.saturating_add(right.height);
    if left_right < right.x
        || right_right < left.x
        || left_bottom < right.y
        || right_bottom < left.y
    {
        return None;
    }
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    Some(WebviewDirtyRect {
        x,
        y,
        width: left_right.max(right_right).saturating_sub(x),
        height: left_bottom.max(right_bottom).saturating_sub(y),
    })
}

fn overlay_damage_from_rects(
    width: u32,
    height: u32,
    rects: impl IntoIterator<Item = WebviewDirtyRect>,
) -> OverlayDamage {
    let mut merged = Vec::<WebviewDirtyRect>::new();
    for rect in rects {
        let right = rect.x.saturating_add(rect.width).min(width);
        let bottom = rect.y.saturating_add(rect.height).min(height);
        if right <= rect.x || bottom <= rect.y || rect.x >= width || rect.y >= height {
            continue;
        }
        let mut rect = WebviewDirtyRect {
            x: rect.x,
            y: rect.y,
            width: right - rect.x,
            height: bottom - rect.y,
        };
        let mut index = 0;
        while index < merged.len() {
            if let Some(union) = overlay_rect_union(merged[index], rect) {
                rect = union;
                merged.swap_remove(index);
                index = 0;
            } else {
                index += 1;
            }
        }
        merged.push(rect);
    }
    if merged.is_empty() {
        return OverlayDamage::None;
    }
    let area = merged.iter().fold(0_u64, |total, rect| {
        total.saturating_add(rect.width as u64 * rect.height as u64)
    });
    let full_area = width as u64 * height as u64;
    if merged.len() > 16 || area.saturating_mul(2) >= full_area {
        OverlayDamage::Full
    } else {
        OverlayDamage::Rects(merged)
    }
}

fn coalesced_overlay_dirty(
    width: u32,
    height: u32,
    previous: &[WebviewDirtyRect],
    latest: &[WebviewDirtyRect],
    same_surface: bool,
) -> bevy_cef_core::prelude::WebviewDirtyRects {
    if !same_surface {
        return Default::default();
    }
    OverlayDamage::from_frame(width, height, previous)
        .union(
            OverlayDamage::from_frame(width, height, latest),
            width,
            height,
        )
        .into_frame_dirty()
        .into_iter()
        .collect()
}

fn native_overlay_blit_regions<'a>(
    width: u32,
    height: u32,
    damage: &'a OverlayDamage,
    initialized: bool,
) -> Cow<'a, [WebviewDirtyRect]> {
    if !initialized || matches!(damage, OverlayDamage::Full) {
        return Cow::Owned(vec![WebviewDirtyRect {
            x: 0,
            y: 0,
            width,
            height,
        }]);
    }
    match damage {
        OverlayDamage::None => Cow::Borrowed(&[]),
        OverlayDamage::Full => unreachable!(),
        OverlayDamage::Rects(rects) => Cow::Borrowed(rects),
    }
}

fn native_overlay_presenter() -> bevy_cef_core::prelude::AcceleratedFramePresenter {
    Arc::new(|frame| {
        let mut pending = NATIVE_OVERLAY_MAILBOX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let webview = frame.webview;
        let should_schedule = match pending.entry(webview) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Some(frame));
                true
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if let Some(previous) = entry.get_mut().replace(frame)
                    && let Some(latest) = entry.get_mut().as_mut()
                {
                    latest.dirty = coalesced_overlay_dirty(
                        latest.width,
                        latest.height,
                        &previous.dirty,
                        &latest.dirty,
                        previous.width == latest.width
                            && previous.height == latest.height
                            && previous.format == latest.format,
                    );
                }
                false
            }
        };
        drop(pending);
        if should_schedule {
            schedule_native_overlay_present();
        }
    })
}

fn schedule_native_overlay_present() {
    if NATIVE_OVERLAY_PRESENT_SCHEDULED.swap(true, Ordering::AcqRel) {
        return;
    }
    dispatch2::DispatchQueue::main().exec_async(drain_native_overlay_present);
}

fn drain_native_overlay_present() {
    use objc2::runtime::AnyObject;

    let ready_layers = {
        let state = NATIVE_OVERLAY_PRESENT_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state
            .layers
            .iter()
            .filter(|(entity, _)| !state.in_flight.contains_key(entity))
            .map(|(entity, layer)| (*entity, layer.clone()))
            .collect::<Vec<_>>()
    };
    let ready = {
        let mut pending = NATIVE_OVERLAY_MAILBOX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ready_layers
            .into_iter()
            .filter_map(|(entity, layer)| Some((entity, layer, pending.get_mut(&entity)?.take()?)))
            .collect::<Vec<_>>()
    };

    for (entity, layer, frame) in ready {
        if present_native_overlay_dirty(entity, &frame) {
            continue;
        }
        let io_surface = frame.io_surface as *mut AnyObject;
        if io_surface.is_null() {
            continue;
        }
        unsafe { layer.0.setContents(Some(&*io_surface)) };
        NATIVE_OVERLAY_PRESENT_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .held
            .insert(entity, frame);
    }

    NATIVE_OVERLAY_PRESENT_SCHEDULED.store(false, Ordering::Release);
    let has_ready = {
        let state = NATIVE_OVERLAY_PRESENT_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut pending = NATIVE_OVERLAY_MAILBOX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending.retain(|entity, frame| frame.is_some() || state.in_flight.contains_key(entity));
        pending.iter().any(|(entity, frame)| {
            frame.is_some()
                && state.layers.contains_key(entity)
                && !state.in_flight.contains_key(entity)
        })
    };
    if has_ready {
        schedule_native_overlay_present();
    }
}

fn register_native_overlay_layer(
    entity: Entity,
    layer: &objc2::rc::Retained<objc2_quartz_core::CALayer>,
) {
    let mut state = NATIVE_OVERLAY_PRESENT_STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state
        .layers
        .entry(entity)
        .or_insert_with(|| SendLayer(layer.clone()));
    drop(state);
    let should_schedule = NATIVE_OVERLAY_MAILBOX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&entity)
        .is_some_and(Option::is_some);
    if should_schedule {
        schedule_native_overlay_present();
    }
}

fn unregister_native_overlay_layer(entity: Entity) {
    let mut state = NATIVE_OVERLAY_PRESENT_STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.layers.remove(&entity);
    state.held.remove(&entity);
    state.swapchains.remove(&entity);
    state.source_textures.remove(&entity);
    state.in_flight.remove(&entity);
    drop(state);
    NATIVE_OVERLAY_MAILBOX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&entity);
}

fn create_metal_overlay_context() -> Option<MetalOverlayContext> {
    use objc2_metal::MTLDevice;

    let device = objc2_metal::MTLCreateSystemDefaultDevice()?;
    let queue = device.newCommandQueue()?;
    Some(MetalOverlayContext { device, queue })
}

fn overlay_metal_format(
    format: bevy_cef_core::prelude::AcceleratedPixelFormat,
) -> (objc2_metal::MTLPixelFormat, i32) {
    match format {
        bevy_cef_core::prelude::AcceleratedPixelFormat::Rgba8 => {
            (objc2_metal::MTLPixelFormat::RGBA8Unorm_sRGB, 0x5247_4241)
        }
        bevy_cef_core::prelude::AcceleratedPixelFormat::Bgra8 => {
            (objc2_metal::MTLPixelFormat::BGRA8Unorm_sRGB, 0x4247_5241)
        }
    }
}

fn overlay_texture_descriptor(
    width: u32,
    height: u32,
    pixel_format: objc2_metal::MTLPixelFormat,
) -> objc2::rc::Retained<objc2_metal::MTLTextureDescriptor> {
    let descriptor = unsafe {
        objc2_metal::MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            pixel_format,
            width as usize,
            height as usize,
            false,
        )
    };
    descriptor.setStorageMode(objc2_metal::MTLStorageMode::Shared);
    descriptor
}

fn create_overlay_io_surface(
    width: u32,
    height: u32,
    pixel_fourcc: i32,
) -> Option<objc2_core_foundation::CFRetained<objc2_io_surface::IOSurfaceRef>> {
    use objc2_core_foundation::{CFDictionary, CFNumber, CFType};
    use objc2_io_surface::{
        IOSurfaceRef, kIOSurfaceAllocSize, kIOSurfaceBytesPerElement, kIOSurfaceBytesPerRow,
        kIOSurfaceHeight, kIOSurfacePixelFormat, kIOSurfaceWidth,
    };

    let bytes_per_row = (width as usize * 4).div_ceil(256) * 256;
    let width_value = CFNumber::new_i64(width as i64);
    let height_value = CFNumber::new_i64(height as i64);
    let bytes_per_row_value = CFNumber::new_i64(bytes_per_row as i64);
    let bytes_per_element_value = CFNumber::new_i64(4);
    let alloc_size_value = CFNumber::new_i64((bytes_per_row * height as usize) as i64);
    let pixel_format_value = CFNumber::new_i32(pixel_fourcc);
    let keys: [&CFType; 6] = unsafe {
        [
            kIOSurfaceWidth.as_ref(),
            kIOSurfaceHeight.as_ref(),
            kIOSurfaceBytesPerRow.as_ref(),
            kIOSurfaceBytesPerElement.as_ref(),
            kIOSurfaceAllocSize.as_ref(),
            kIOSurfacePixelFormat.as_ref(),
        ]
    };
    let values: [&CFType; 6] = [
        width_value.as_ref(),
        height_value.as_ref(),
        bytes_per_row_value.as_ref(),
        bytes_per_element_value.as_ref(),
        alloc_size_value.as_ref(),
        pixel_format_value.as_ref(),
    ];
    let properties = CFDictionary::<CFType, CFType>::from_slices(&keys, &values);
    unsafe { IOSurfaceRef::new(properties.as_opaque()) }
}

fn create_metal_overlay_surface(
    context: &MetalOverlayContext,
    width: u32,
    height: u32,
    format: bevy_cef_core::prelude::AcceleratedPixelFormat,
) -> Option<MetalOverlaySurface> {
    use objc2_metal::MTLDevice;

    let (pixel_format, pixel_fourcc) = overlay_metal_format(format);
    let io_surface = create_overlay_io_surface(width, height, pixel_fourcc)?;
    let descriptor = overlay_texture_descriptor(width, height, pixel_format);
    let texture =
        context
            .device
            .newTextureWithDescriptor_iosurface_plane(&descriptor, &io_surface, 0)?;
    Some(MetalOverlaySurface {
        io_surface,
        texture,
    })
}

fn create_metal_overlay_swapchain(
    context: &MetalOverlayContext,
    generation: u64,
    width: u32,
    height: u32,
    format: bevy_cef_core::prelude::AcceleratedPixelFormat,
) -> Option<MetalOverlaySwapchain> {
    let create_slot = || {
        Some(MetalOverlaySlot {
            surface: create_metal_overlay_surface(context, width, height, format)?,
            initialized: false,
            stale: OverlayDamage::None,
        })
    };
    Some(MetalOverlaySwapchain {
        generation,
        width,
        height,
        format,
        slots: [create_slot()?, create_slot()?],
        front: None,
    })
}

fn cached_source_texture(
    state: &mut NativeOverlayPresentState,
    context: &MetalOverlayContext,
    entity: Entity,
    frame: &bevy_cef_core::prelude::AcceleratedFrame,
) -> Option<objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>>> {
    use objc2_metal::MTLDevice;

    state.source_texture_clock = state.source_texture_clock.wrapping_add(1);
    let last_used = state.source_texture_clock;
    let cache = state.source_textures.entry(entity).or_default();
    if let Some(cached) = cache.iter_mut().find(|cached| {
        cached.io_surface == frame.io_surface
            && cached.width == frame.width
            && cached.height == frame.height
            && cached.format == frame.format
    }) {
        cached.last_used = last_used;
        return Some(cached.texture.clone());
    }
    let source_surface = unsafe { &*(frame.io_surface as *const objc2_io_surface::IOSurfaceRef) };
    let (pixel_format, _) = overlay_metal_format(frame.format);
    let descriptor = overlay_texture_descriptor(frame.width, frame.height, pixel_format);
    let texture =
        context
            .device
            .newTextureWithDescriptor_iosurface_plane(&descriptor, source_surface, 0)?;
    if cache.len() >= 4
        && let Some((oldest, _)) = cache
            .iter()
            .enumerate()
            .min_by_key(|(_, cached)| cached.last_used)
    {
        cache.swap_remove(oldest);
    }
    cache.push(MetalSourceTexture {
        io_surface: frame.io_surface,
        width: frame.width,
        height: frame.height,
        format: frame.format,
        texture: texture.clone(),
        last_used,
    });
    Some(texture)
}

fn complete_native_overlay_present(
    entity: Entity,
    generation: u64,
    target: usize,
    succeeded: bool,
) {
    use objc2::runtime::AnyObject;

    let (layer, surface, can_schedule) = {
        let mut state = NATIVE_OVERLAY_PRESENT_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(in_flight) = state.in_flight.remove(&entity) else {
            return;
        };
        if in_flight.generation != generation || in_flight.target != target {
            return;
        }
        let surface = if succeeded {
            let Some(swapchain) = state.swapchains.get_mut(&entity) else {
                return;
            };
            if swapchain.generation != generation {
                return;
            }
            for (index, slot) in swapchain.slots.iter_mut().enumerate() {
                if index == target {
                    slot.initialized = true;
                    slot.stale = OverlayDamage::None;
                } else if slot.initialized {
                    slot.stale = std::mem::take(&mut slot.stale).union(
                        in_flight.damage.clone(),
                        swapchain.width,
                        swapchain.height,
                    );
                }
            }
            swapchain.front = Some(target);
            Some(swapchain.slots[target].surface.clone())
        } else {
            None
        };
        let layer = surface
            .as_ref()
            .and_then(|_| state.layers.get(&entity).cloned());
        if surface.is_some() {
            state.held.remove(&entity);
        }
        let can_schedule =
            state.layers.contains_key(&entity) && !state.in_flight.contains_key(&entity);
        (layer, surface, can_schedule)
    };
    if let (Some(layer), Some(surface)) = (layer, surface) {
        let io_surface =
            (&*surface.io_surface as *const objc2_io_surface::IOSurfaceRef).cast::<AnyObject>();
        unsafe { layer.0.setContents(Some(&*io_surface)) };
    }
    let should_schedule = if can_schedule {
        let mut pending = NATIVE_OVERLAY_MAILBOX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match pending.get(&entity) {
            Some(Some(_)) => true,
            Some(None) => {
                pending.remove(&entity);
                false
            }
            None => false,
        }
    } else {
        false
    };
    if should_schedule {
        schedule_native_overlay_present();
    }
}

fn present_native_overlay_dirty(
    entity: Entity,
    frame: &bevy_cef_core::prelude::AcceleratedFrame,
) -> bool {
    use objc2_metal::{
        MTLBlitCommandEncoder, MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandEncoder,
        MTLCommandQueue, MTLOrigin, MTLSize,
    };

    if frame.io_surface == 0 || frame.width == 0 || frame.height == 0 {
        return false;
    }
    let (
        context,
        surface,
        source_texture,
        generation,
        target,
        frame_damage,
        copy_damage,
        initialized,
    ) = {
        let mut state = NATIVE_OVERLAY_PRESENT_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.in_flight.contains_key(&entity) {
            return false;
        }
        if state.metal.is_none() {
            state.metal = create_metal_overlay_context();
        }
        let Some(context) = state.metal.clone() else {
            return false;
        };
        let recreate = state.swapchains.get(&entity).is_none_or(|swapchain| {
            swapchain.width != frame.width
                || swapchain.height != frame.height
                || swapchain.format != frame.format
        });
        if recreate {
            state.next_generation = state.next_generation.wrapping_add(1);
            let generation = state.next_generation;
            let Some(swapchain) = create_metal_overlay_swapchain(
                &context,
                generation,
                frame.width,
                frame.height,
                frame.format,
            ) else {
                return false;
            };
            state.swapchains.insert(entity, swapchain);
            state.source_textures.remove(&entity);
        }
        let Some(source_texture) = cached_source_texture(&mut state, &context, entity, frame)
        else {
            return false;
        };
        let frame_damage = OverlayDamage::from_frame(frame.width, frame.height, &frame.dirty);
        let swapchain = state.swapchains.get(&entity).unwrap();
        let target = swapchain.front.map_or(0, |front| 1 - front);
        let slot = &swapchain.slots[target];
        let copy_damage = if slot.initialized {
            slot.stale
                .clone()
                .union(frame_damage.clone(), frame.width, frame.height)
        } else {
            OverlayDamage::Full
        };
        (
            context,
            slot.surface.clone(),
            source_texture,
            swapchain.generation,
            target,
            frame_damage,
            copy_damage,
            slot.initialized,
        )
    };
    let Some(command_buffer) = context.queue.commandBuffer() else {
        return false;
    };
    let Some(blit) = command_buffer.blitCommandEncoder() else {
        return false;
    };
    let copy = |x: u32, y: u32, width: u32, height: u32| {
        if width == 0 || height == 0 {
            return;
        }
        unsafe {
            blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                &source_texture,
                0,
                0,
                MTLOrigin {
                    x: x as usize,
                    y: y as usize,
                    z: 0,
                },
                MTLSize {
                    width: width as usize,
                    height: height as usize,
                    depth: 1,
                },
                &surface.texture,
                0,
                0,
                MTLOrigin {
                    x: x as usize,
                    y: y as usize,
                    z: 0,
                },
            );
        }
    };
    for rect in
        native_overlay_blit_regions(frame.width, frame.height, &copy_damage, initialized).iter()
    {
        copy(rect.x, rect.y, rect.width, rect.height);
    }
    blit.endEncoding();
    NATIVE_OVERLAY_PRESENT_STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .in_flight
        .insert(
            entity,
            MetalOverlayInFlight {
                generation,
                target,
                damage: frame_damage,
            },
        );
    let keepalive = frame.keepalive.clone();
    let completed = block2::RcBlock::new(
        move |buffer: std::ptr::NonNull<
            objc2::runtime::ProtocolObject<dyn objc2_metal::MTLCommandBuffer>,
        >| {
            let succeeded =
                unsafe { buffer.as_ref() }.status() == MTLCommandBufferStatus::Completed;
            let keepalive = keepalive.clone();
            dispatch2::DispatchQueue::main().exec_async(move || {
                complete_native_overlay_present(entity, generation, target, succeeded);
                drop(keepalive);
            });
        },
    );
    unsafe { command_buffer.addCompletedHandler(block2::RcBlock::as_ptr(&completed)) };
    command_buffer.commit();
    true
}

fn primary_content_view_ptr(entity: Entity) -> Option<*mut core::ffi::c_void> {
    use bevy::winit::WINIT_WINDOWS;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    WINIT_WINDOWS.with_borrow(|windows| {
        let id = windows.entity_to_winit.get(&entity)?;
        let wrapper = windows.windows.get(id)?;
        let handle = wrapper.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::AppKit(h) => Some(h.ns_view.as_ptr()),
            _ => None,
        }
    })
}

fn keep_window_surface_layer_transparent(window: Query<Entity, With<bevy::window::PrimaryWindow>>) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSColor, NSView};

    if MainThreadMarker::new().is_none() {
        return;
    }
    let Ok(entity) = window.single() else {
        return;
    };
    let Some(ns_view) = primary_content_view_ptr(entity) else {
        return;
    };
    let content: &NSView = unsafe { &*ns_view.cast::<NSView>() };
    content.setWantsLayer(true);
    let Some(layer) = content.layer() else {
        return;
    };
    let clear_color = NSColor::clearColor();
    layer.setOpaque(false);
    layer.setBackgroundColor(Some(&clear_color.CGColor()));
}

fn sync_layout_overlay(
    mut state: NonSendMut<LayoutOverlay>,
    layout_e_q: Query<Entity, With<LayoutCef>>,
    window_q: Query<Entity, With<bevy::window::PrimaryWindow>>,
    windows: Query<&bevy::window::Window>,
    mode: Res<InteractionMode>,
    mut overlay_frames: ResMut<bevy_cef::prelude::NativeOverlayFrames>,
) {
    use objc2::{MainThreadMarker, rc::Retained, runtime::AnyObject};
    use objc2_app_kit::{NSColor, NSView};
    use objc2_quartz_core::{CAAutoresizingMask, CALayer};

    if *mode != InteractionMode::User {
        for layout_e in &layout_e_q {
            unregister_native_overlay_layer(layout_e);
        }
        if state.shown {
            if let Some(layer) = &state.layer {
                layer.setHidden(true);
            }
            state.shown = false;
        }
        return;
    }
    let Some(_mtm) = MainThreadMarker::new() else {
        return;
    };
    let (Ok(window_e), Ok(layout_e)) = (window_q.single(), layout_e_q.single()) else {
        return;
    };
    let next = overlay_frames.0.remove(&layout_e);
    let Some(ns_view) = primary_content_view_ptr(window_e) else {
        return;
    };
    let content: &NSView = unsafe { &*ns_view.cast::<NSView>() };
    content.setWantsLayer(true);
    let Some(host_layer) = content.layer() else {
        return;
    };
    let clear_color = NSColor::clearColor();
    host_layer.setOpaque(false);
    host_layer.setBackgroundColor(Some(&clear_color.CGColor()));
    let bounds = content.bounds();

    if state.layer.is_none() {
        let layer: Retained<objc2_quartz_core::CALayer> = CALayer::new();
        layer.setOpaque(false);
        layer.setBackgroundColor(Some(&clear_color.CGColor()));
        layer.setZPosition(100.0);
        layer.setAutoresizingMask(
            CAAutoresizingMask::LayerWidthSizable | CAAutoresizingMask::LayerHeightSizable,
        );
        host_layer.addSublayer(&layer);
        state.layer = Some(layer);
    }
    let Some(layer) = state.layer.clone() else {
        return;
    };
    register_native_overlay_layer(layout_e, &layer);
    layer.setOpaque(false);
    layer.setBackgroundColor(Some(&clear_color.CGColor()));
    layer.setFrame(bounds);
    layer.setContentsScale(
        windows
            .get(window_e)
            .map(|w| w.resolution.scale_factor() as f64)
            .unwrap_or(2.0),
    );

    if let Some(frame) = next {
        let io_surface = frame.io_surface as *mut AnyObject;
        if !io_surface.is_null() {
            unsafe { layer.setContents(Some(&*io_surface)) };
            state.held = Some(frame);
        }
    }
    layer.setHidden(false);
    state.shown = true;
}

/// A2: show the command bar's OSR IOSurface in a full-window native overlay composited **above** the
/// page (so the page stays visible through the surface's transparent backdrop). The surface is
/// produced by the OSR modal and routed here via `NativeOverlayFrames`.
fn sync_command_bar_overlay(
    mut state: NonSendMut<CommandBarOverlay>,
    modal_open_q: Query<
        (&Node, Has<bevy_cef::prelude::CefKeyboardTarget>),
        With<vmux_layout::window::Modal>,
    >,
    modal_e_q: Query<Entity, With<vmux_layout::window::Modal>>,
    window_q: Query<Entity, With<bevy::window::PrimaryWindow>>,
    windows: Query<&bevy::window::Window>,
    mut overlay_frames: ResMut<bevy_cef::prelude::NativeOverlayFrames>,
) {
    use objc2::{MainThreadMarker, MainThreadOnly, runtime::AnyObject};
    use objc2_app_kit::NSView;

    let open = vmux_layout::command_bar::handler::is_command_bar_open(&modal_open_q);
    if !open {
        if state.shown {
            if let Some(view) = &state.view {
                view.setHidden(true);
            }
            state.shown = false;
            state.held = None;
        }
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let (Ok(window_e), Ok(modal_e)) = (window_q.single(), modal_e_q.single()) else {
        return;
    };
    // Pull the latest OSR frame for the modal. A *windowed* command bar produces none, so leave the
    // overlay dormant rather than covering the native command bar with an empty input-stealing layer.
    let next = overlay_frames.0.remove(&modal_e);
    if next.is_none() && state.held.is_none() {
        return;
    }
    let Some(ns_view) = primary_content_view_ptr(window_e) else {
        return;
    };
    let content: &NSView = unsafe { &*ns_view.cast::<NSView>() };
    let bounds = content.bounds();

    if state.view.is_none() {
        let view = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        view.setWantsLayer(true);
        state.view = Some(view);
    }
    let Some(view) = state.view.clone() else {
        return;
    };
    view.setFrame(bounds);

    if let Some(frame) = next {
        if let Some(layer) = view.layer() {
            let io_surface = frame.io_surface as *mut AnyObject;
            if !io_surface.is_null() {
                let scale = windows
                    .get(window_e)
                    .map(|w| w.resolution.scale_factor() as f64)
                    .unwrap_or(2.0);
                layer.setOpaque(false);
                layer.setContentsScale(scale);
                unsafe { layer.setContents(Some(&*io_surface)) };
            }
        }
        state.held = Some(frame);
    }

    if !state.shown {
        view.setHidden(false);
        state.shown = true;
    }
    // Raise above the native pages (re-add reorders to front; pages re-raise each frame).
    content.addSubview(&view);
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_layout::scene::InteractionMode;

    #[test]
    fn glass_backdrop_is_hidden_in_player_mode() {
        assert!(!glass_backdrop_visible(InteractionMode::Player));
        assert!(glass_backdrop_visible(InteractionMode::User));
    }

    #[test]
    fn glass_install_does_not_reveal_window() {
        let source = include_str!("glass.rs");
        let install = source
            .split("fn install_window_glass")
            .nth(1)
            .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
            .unwrap_or_default();

        assert!(!install.contains("window.visible = true"));
        assert!(!install.contains("activate_native_window"));
    }

    #[test]
    fn window_backdrop_uses_clear_glass_style() {
        let source = include_str!("glass.rs");
        let install = source
            .split("fn install_window_glass")
            .nth(1)
            .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
            .unwrap_or_default();

        assert!(install.contains("NSGlassEffectViewStyle::Clear"));
        assert!(!install.contains("NSGlassEffectViewStyle::Regular"));
    }

    #[test]
    fn window_backdrop_uses_clear_glass_tint() {
        let source = include_str!("glass.rs");
        let install = source
            .split("fn install_window_glass")
            .nth(1)
            .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
            .unwrap_or_default();

        assert!(install.contains("glass.setTintColor(Some(&NSColor::clearColor()))"));
    }

    #[test]
    fn window_backdrop_lives_in_nonactivating_child_window() {
        let source = include_str!("glass.rs");
        let install = source
            .split("fn install_window_glass")
            .nth(1)
            .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
            .unwrap_or_default();

        assert!(install.contains("NSPanel"));
        assert!(install.contains("NSWindowStyleMask::NonactivatingPanel"));
        assert!(install.contains("setIgnoresMouseEvents(true)"));
        assert!(install.contains("addChildWindow_ordered"));
        assert!(install.contains("NSWindowOrderingMode::Below"));
    }

    #[test]
    fn window_backdrop_tracks_parent_window_frame() {
        let source = include_str!("glass.rs");
        let sync = source
            .split("fn sync_window_glass_visibility")
            .nth(1)
            .and_then(|tail| {
                tail.split("#[derive(Default)]\nstruct LayoutOverlay")
                    .next()
            })
            .unwrap_or_default();

        assert!(sync.contains("backdrop_window.setFrame_display(parent_window.frame(), false)"));
    }

    #[test]
    fn desktop_enables_nspanel_binding_for_glass_backdrop() {
        let manifest = include_str!("../Cargo.toml");

        assert!(manifest.contains("\"NSPanel\""));
    }

    #[test]
    fn layout_overlay_uses_layer_for_hit_test_passthrough() {
        let source = include_str!("glass.rs");
        let overlay = source
            .split("fn sync_layout_overlay")
            .nth(1)
            .and_then(|tail| tail.split("fn sync_command_bar_overlay").next())
            .unwrap_or_default();

        assert!(overlay.contains("Retained<objc2_quartz_core::CALayer>"));
        assert!(overlay.contains("CALayer::new()"));
        assert!(overlay.contains("addSublayer"));
        assert!(overlay.contains("layer.setContents"));
        assert!(!overlay.contains("NSView::initWithFrame"));
    }

    #[test]
    fn layout_overlay_keeps_host_and_overlay_layers_transparent() {
        let source = include_str!("glass.rs");
        let overlay = source
            .split("fn sync_layout_overlay")
            .nth(1)
            .and_then(|tail| tail.split("fn sync_command_bar_overlay").next())
            .unwrap_or_default();

        assert!(overlay.contains("host_layer.setOpaque(false)"));
        assert!(overlay.contains("host_layer.setBackgroundColor(Some(&clear_color.CGColor()))"));
        assert!(overlay.contains("layer.setBackgroundColor(Some(&clear_color.CGColor()))"));
    }

    #[test]
    fn native_overlay_blits_only_dirty_regions() {
        let dirty = vec![
            WebviewDirtyRect {
                x: 10,
                y: 20,
                width: 30,
                height: 40,
            },
            WebviewDirtyRect {
                x: 50,
                y: 60,
                width: 70,
                height: 80,
            },
        ];

        let damage = OverlayDamage::from_frame(200, 100, &dirty);

        assert_eq!(
            native_overlay_blit_regions(200, 100, &damage, true).as_ref(),
            [
                dirty[0],
                WebviewDirtyRect {
                    height: 40,
                    ..dirty[1]
                },
            ]
        );
    }

    #[test]
    fn native_overlay_resize_forces_full_blit() {
        let dirty = vec![WebviewDirtyRect {
            x: 10,
            y: 20,
            width: 30,
            height: 40,
        }];

        let damage = OverlayDamage::from_frame(200, 100, &dirty);

        assert_eq!(
            native_overlay_blit_regions(200, 100, &damage, false).as_ref(),
            [WebviewDirtyRect {
                x: 0,
                y: 0,
                width: 200,
                height: 100,
            }]
        );
    }

    #[test]
    fn native_overlay_coalescing_unions_dirty_regions() {
        let previous = vec![WebviewDirtyRect {
            x: 10,
            y: 20,
            width: 30,
            height: 40,
        }];
        let latest = vec![WebviewDirtyRect {
            x: 100,
            y: 20,
            width: 30,
            height: 40,
        }];

        let dirty = coalesced_overlay_dirty(200, 100, &previous, &latest, true);

        assert_eq!(
            dirty.as_slice(),
            &[
                WebviewDirtyRect {
                    x: 10,
                    y: 20,
                    width: 30,
                    height: 40,
                },
                WebviewDirtyRect {
                    x: 100,
                    y: 20,
                    width: 30,
                    height: 40,
                },
            ]
        );
    }

    #[test]
    fn native_overlay_coalescing_merges_overlapping_regions() {
        let previous = vec![WebviewDirtyRect {
            x: 10,
            y: 20,
            width: 30,
            height: 40,
        }];
        let latest = vec![WebviewDirtyRect {
            x: 20,
            y: 30,
            width: 40,
            height: 50,
        }];

        assert_eq!(
            coalesced_overlay_dirty(200, 100, &previous, &latest, true).as_slice(),
            &[WebviewDirtyRect {
                x: 10,
                y: 20,
                width: 50,
                height: 60,
            }]
        );
    }

    #[test]
    fn native_overlay_full_damage_survives_coalescing() {
        let latest = vec![WebviewDirtyRect {
            x: 20,
            y: 30,
            width: 40,
            height: 50,
        }];

        assert!(coalesced_overlay_dirty(200, 100, &[], &latest, true).is_empty());
        assert!(coalesced_overlay_dirty(200, 100, &latest, &latest, false).is_empty());
    }

    #[test]
    fn native_overlay_metal_completion_is_asynchronous() {
        let source = include_str!("glass.rs");
        let presenter = source
            .split("fn present_native_overlay_dirty")
            .nth(1)
            .and_then(|tail| tail.split("fn primary_content_view_ptr").next())
            .unwrap_or_default();

        assert!(presenter.contains("addCompletedHandler"));
        assert!(!presenter.contains("waitUntilCompleted"));
    }

    fn reveal_test_app(reveal_ready: bool) -> App {
        let mut app = App::new();
        app.add_systems(Update, reveal_window_after_layout_ready);
        app.world_mut().insert_non_send(GlassState {
            installed: true,
            ..default()
        });
        app.world_mut().spawn((
            Window {
                visible: false,
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));
        app.insert_resource(crate::boot_status::SplashStatus {
            phase: crate::boot_status::BootPhase::Starting,
            reveal_ready,
        });
        app
    }

    #[test]
    fn startup_window_stays_hidden_until_reveal_ready() {
        let mut app = reveal_test_app(false);

        app.update();

        let window = app
            .world_mut()
            .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
            .single(app.world())
            .expect("primary window");
        assert!(!window.visible);
    }

    #[test]
    fn startup_window_reveals_after_reveal_ready() {
        let mut app = reveal_test_app(true);

        app.update();

        let window = app
            .world_mut()
            .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
            .single(app.world())
            .expect("primary window");
        assert!(window.visible);
    }

    #[test]
    fn no_activation_before_reveal() {
        assert!(!should_attempt_activation(false, false, None));
    }

    #[test]
    fn activates_immediately_after_reveal() {
        assert!(should_attempt_activation(true, false, None));
        assert!(should_attempt_activation(true, false, Some(Duration::ZERO)));
    }

    #[test]
    fn stops_once_confirmed() {
        assert!(!should_attempt_activation(
            true,
            true,
            Some(Duration::from_millis(10))
        ));
    }

    #[test]
    fn retries_within_budget_then_gives_up() {
        assert!(should_attempt_activation(
            true,
            false,
            Some(ACTIVATION_RETRY_BUDGET - Duration::from_millis(1))
        ));
        assert!(!should_attempt_activation(
            true,
            false,
            Some(ACTIVATION_RETRY_BUDGET)
        ));
    }

    #[test]
    fn reveal_does_not_activate_inline() {
        let source = include_str!("glass.rs");
        let reveal = source
            .split("fn reveal_window_after_layout_ready")
            .nth(1)
            .and_then(|tail| tail.split("fn should_attempt_activation").next())
            .unwrap_or_default();

        assert!(!reveal.contains("activate_native_window"));
        assert!(reveal.contains("state.revealed_at = Some(Instant::now())"));
    }

    #[test]
    fn activation_retry_system_is_registered() {
        let source = include_str!("glass.rs");
        let build = source
            .split("fn build(&self, app: &mut App)")
            .nth(1)
            .and_then(|tail| tail.split("#[derive(Default)]").next())
            .unwrap_or_default();

        assert!(build.contains("ensure_window_active_after_reveal"));
    }

    #[test]
    fn surface_transparency_system_is_registered() {
        let source = include_str!("glass.rs");
        let build = source
            .split("fn build(&self, app: &mut App)")
            .nth(1)
            .and_then(|tail| tail.split("#[derive(Default)]").next())
            .unwrap_or_default();

        assert!(build.contains("keep_window_surface_layer_transparent"));
    }

    #[test]
    fn surface_layer_kept_non_opaque_and_clear() {
        let source = include_str!("glass.rs");
        let func = source
            .split("fn keep_window_surface_layer_transparent")
            .nth(1)
            .and_then(|tail| tail.split("fn sync_layout_overlay").next())
            .unwrap_or_default();

        assert!(func.contains("layer.setOpaque(false)"));
        assert!(func.contains("layer.setBackgroundColor(Some(&clear_color.CGColor()))"));
    }
}
