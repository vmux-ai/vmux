//! Browser and page-open orchestration on top of `bevy_cef`: page resolution, CEF
//! backend management, and input forwarding between the native layout and embedded pages.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod appearance;
mod command;
mod extensions;
mod frame_rate;
mod host_focus;
mod input;
mod page_life;

mod native_bridge;
mod native_layout;
mod navigation;
mod present;

use crate::page_life::spawn_popup_stacks;
use present::CommandBarWindowedFrame;
mod page_open;
mod page_state;
mod scroll;
mod snapshot;
pub use host_focus::HostFocusIntent;

pub use native_bridge::NativeBridge;
/// Entry points for the AppKit monitors. Nothing in Rust calls them.
#[cfg(target_os = "macos")]
pub use native_bridge::{queue_command_bar_pointer_button, queue_command_bar_pointer_move};
pub use native_layout::NativeLayout;
#[cfg(target_os = "macos")]
pub use native_layout::NativeLayoutPointerMoveResult;

use bevy::{
    ecs::relationship::Relationship,
    input::{ButtonState, mouse::MouseButton},
    material::AlphaMode,
    picking::pointer::PointerButton,
    prelude::*,
    ui::UiGlobalTransform,
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::{CefEmbeddedHosts, CommandLineConfig, webview_debug_log};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use vmux_command::ReadAppCommands;
use vmux_core::{
    CefPageAttachRequest, HostSpawnRegistry, OscTitle, PageMetadata, PageOpenRequest, PageOpenSet,
    page::{PageManifest, PageReady},
};
use vmux_history::LastActivatedAt;
use vmux_layout::command_bar::handler::PendingCommandBarReveal;
use vmux_layout::event::{RemoteCommandEvent, RemoteCopyEvent, SideSheetCommandEvent};
pub use vmux_layout::{Browser, Loading};
use vmux_layout::{
    Header, LayoutCef, Open, PendingWebviewReveal, UpdateState,
    bookmark::BookmarkContextMenuActive,
    command_bar::panel::CommandBarPanelActive,
    event::{
        DebugSimulateDownload, DebugUpdateClear, DebugUpdateReady, HeaderCommandEvent, StackRow,
    },
    pane::{Pane, PaneSplit},
    side_sheet::SideSheet,
    stack::{Stack, active_stack_in_pane, collect_leaf_panes},
    tab::Tab,
    window::Modal,
};

use vmux_setting::AppSettings;
use vmux_ui::i18n::Locale;
use vmux_ui::theme::ThemeEvent;

/// Wires browser orchestration: resolves CEF embedded hosts from page manifests, manages
/// the CEF backend, and forwards pointer and cursor input between the layout and pages.
pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        let profile = vmux_core::profile::active_profile_name();
        let startup_settings = vmux_setting::read_settings_from_disk();
        let startup_locale =
            Locale::requested(Some(&startup_settings.appearance.locale)).into_string();
        let startup_accept_language_list = browser_accept_language_list(&startup_locale);
        let prepared_extensions = crate::extensions::load::apply_env().unwrap_or_else(|error| {
            bevy::log::error!(%error, "failed to prepare extensions; starting without them");
            unsafe { std::env::remove_var("VMUX_LOAD_EXTENSIONS") };
            Vec::new()
        });
        let conformance_extension = std::env::var("VMUX_EXTENSION_CONFORMANCE_ID").ok();
        let extension_registrations = prepared_extensions
            .iter()
            .map(|runtime| crate::extensions::bridge::BridgeRegistration {
                extension_id: runtime.extension_id.clone(),
                authorization: crate::extensions::bridge::BridgeAuthorization {
                    permissions: runtime.granted_permissions.iter().cloned().collect(),
                    host_permissions: runtime
                        .granted_host_permissions
                        .iter()
                        .map(|pattern| {
                            vmux_core::extension::match_pattern::ChromeMatchPattern::parse(pattern)
                                .unwrap_or_else(|error| {
                                    panic!("invalid stored host permission: {error}")
                                })
                        })
                        .collect(),
                    conformance: conformance_extension.as_deref()
                        == Some(runtime.extension_id.as_str()),
                },
            })
            .collect::<Vec<_>>();
        let extension_bridge = crate::extensions::bridge::ExtensionBridgeServer::start_registered(
            &profile,
            extension_registrations,
        )
        .unwrap_or_else(|error| panic!("failed to start extension bridge: {error}"));
        app.add_plugins((
            extensions::ExtensionsPlugin,
            extensions::bridge_page::ExtensionBridgePagePlugin,
            extensions::broker::ExtensionBrokerPlugin,
            extensions::project::ExtensionProjectPlugin,
            extensions::windows::ExtensionWindowsPlugin,
        ));
        let mut manifests = app.world_mut().query::<&PageManifest>();
        let embedded_hosts = CefEmbeddedHosts(
            manifests
                .iter(app.world())
                .map(PageManifest::embedded_host)
                .collect(),
        );
        webview_debug_log(format!("BrowserPlugin embedded_hosts={embedded_hosts:?}"));
        let cef_command_line = cef_command_line_config();
        configure_cef_backend_sync(app)
            .insert_resource(crate::extensions::load::PreparedExtensions(
                prepared_extensions,
            ))
            .insert_resource(extension_bridge)
            .add_message::<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>()
            .add_message::<WebviewLoadCompleted>()
            .add_message::<PageOpenRequest>()
            .add_message::<CefPageAttachRequest>()
            .add_plugins(vmux_layout::LayoutContractPlugin)
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
            .add_plugins(
                (
                    CefPlugin {
                        command_line_config: cef_command_line,
                        root_cache_path: cef_root_cache_path(),
                        locale: startup_locale,
                        accept_language_list: startup_accept_language_list,
                        embedded_hosts,
                        ..default()
                    },
                    BinEventEmitterPlugin::<(
                        HeaderCommandEvent,
                        SideSheetCommandEvent,
                        RemoteCommandEvent,
                        RemoteCopyEvent,
                    )>::for_hosts(&["layout"]),
                    BinEventEmitterPlugin::<(
                        DebugUpdateReady,
                        DebugUpdateClear,
                        DebugSimulateDownload,
                    )>::for_hosts(&["debug"]),
                ),
            )
            .add_observer(on_debug_update_ready)
            .add_observer(on_debug_update_clear)
            .add_systems(Update, sync_layout_mesh_visibility)
            .add_systems(Update, (vmux_layout::apply_cef_state_from_webview,))
            .add_systems(
                Update,
                vmux_layout::mirror_metadata_to_url
                    .after(vmux_layout::apply_cef_state_from_webview),
            )
            .init_resource::<HostSpawnRegistry>()
            .add_plugins((
                host_focus::HostFocusPlugin,
                appearance::AppearancePlugin,
                page_life::PageLifePlugin,
                command::CommandPlugin,
                frame_rate::FrameRatePlugin,
                input::InputPlugin,
                navigation::NavigationPlugin,
                present::PresentPlugin,
                page_open::PageOpenPlugin,
                page_state::PageStatePlugin,
                snapshot::SnapshotPlugin,
                scroll::ScrollPlugin,
            ));
    }
}

#[derive(Clone, Copy, Debug, Message)]
pub(crate) struct WebviewLoadCompleted {
    webview: Entity,
}

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
enum BrowserSystems {
    SyncCefBackend,
}

fn configure_cef_backend_sync(app: &mut App) -> &mut App {
    app.configure_sets(
        Update,
        BrowserSystems::SyncCefBackend
            .after(vmux_layout::scene::SceneSystems::CompleteModeTransition)
            .before(CefSystems::CreateAndResize),
    )
    .add_systems(
        Update,
        sync_cef_backend_for_interaction_mode
            .in_set(BrowserSystems::SyncCefBackend)
            .after(PageOpenSet::Fallback)
            .after(spawn_popup_stacks),
    )
}

fn cef_command_line_config() -> CommandLineConfig {
    CommandLineConfig {
        switches: vmux_core::profile::cef_keychain_switches().to_vec(),
        switch_values: vec![("disable-features", "BackForwardCache")],
    }
}

fn theme_event(settings: &AppSettings) -> ThemeEvent {
    let locale = Locale::requested(Some(&settings.appearance.locale));
    ThemeEvent {
        radius: settings.layout.radius,
        catalog: external_locale_catalog(locale.as_str()),
        locale: locale.into_string(),
    }
}

fn browser_accept_language_list(locale: &str) -> String {
    let locale = locale.trim();
    let language = locale.split('-').next().unwrap_or(locale);
    if language.eq_ignore_ascii_case("en") {
        if locale.eq_ignore_ascii_case(language) {
            "en,en-US;q=0.9".to_string()
        } else {
            format!("{locale},en;q=0.9")
        }
    } else if locale.eq_ignore_ascii_case(language) {
        format!("{locale},en-US;q=0.9,en;q=0.8")
    } else {
        format!("{locale},{language};q=0.9,en-US;q=0.8,en;q=0.7")
    }
}

fn external_locale_catalog(locale: &str) -> Option<String> {
    let directory = vmux_core::profile::config_dir().join("locales");
    [locale, locale.split('-').next().unwrap_or(locale)]
        .into_iter()
        .find_map(|tag| std::fs::read_to_string(directory.join(format!("{tag}.ftl"))).ok())
}

#[cfg(test)]
mod accept_language_tests {
    use super::browser_accept_language_list;

    #[test]
    fn selected_locale_leads_browser_accept_language() {
        assert_eq!(
            browser_accept_language_list("ja"),
            "ja,en-US;q=0.9,en;q=0.8"
        );
        assert_eq!(
            browser_accept_language_list("pt-BR"),
            "pt-BR,pt;q=0.9,en-US;q=0.8,en;q=0.7"
        );
        assert_eq!(browser_accept_language_list("en-US"), "en-US,en;q=0.9");
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

static NATIVE_LAYOUT_POINTER_INSIDE: AtomicBool = AtomicBool::new(false);
static NATIVE_LAYOUT_ACTIVITY: AtomicBool = AtomicBool::new(false);

fn cef_pointer_hit_rect_contains(rect: CefPointerHitRect, point: Vec2) -> bool {
    if !rect.interactive {
        return false;
    }
    let half = rect.size * 0.5;
    let min = rect.center - half;
    let max = rect.center + half;
    point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
}

pub fn set_native_layout_activity(active: bool) -> bool {
    NATIVE_LAYOUT_ACTIVITY.swap(active, Ordering::Relaxed) != active
}

fn native_layout_activity_active() -> bool {
    NATIVE_LAYOUT_ACTIVITY.load(Ordering::Relaxed)
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

fn pointer_button_from_mouse_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

/// Layout-page surfaces that float free of the laid-out regions and must own the whole window's
/// pointer input while they are up.
///
/// A context menu or the command bar panel can extend past any published hit rect, and both
/// dismiss on an outside click, so the layout webview has to see every move and click. A focused
/// bookmark field does not qualify — it stays inside the side sheet's own region.
pub(crate) type LayoutPointerCapture =
    Or<(With<BookmarkContextMenuActive>, With<CommandBarPanelActive>)>;

fn tab_of(
    start: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<Tab>>,
) -> Option<Entity> {
    let mut e = start;
    loop {
        if tab_q.contains(e) {
            return Some(e);
        }
        match child_of_q.get(e) {
            Ok(co) => e = co.get(),
            Err(_) => return None,
        }
    }
}

fn webview_should_use_windowed(mode: vmux_layout::scene::InteractionMode) -> bool {
    cfg!(target_os = "macos") && mode == vmux_layout::scene::InteractionMode::User
}

fn transform_near(a: &Transform, b: &Transform) -> bool {
    a.translation.distance(b.translation) < 0.001
        && a.scale.distance(b.scale) < 0.001
        && a.rotation.dot(b.rotation).abs() > 0.9999
}

#[derive(Clone, Copy, PartialEq)]
struct WindowedBackendSignature {
    width: f32,
    height: f32,
    scale: f32,
}

#[derive(Resource, Default)]
struct WindowedBackendCameraState {
    mismatch: Option<WindowedBackendSignature>,
}

fn windowed_backend_signature(world: &mut World) -> Option<WindowedBackendSignature> {
    let mut window_q = world.query_filtered::<&Window, With<PrimaryWindow>>();
    let Ok(window) = window_q.single(world) else {
        return None;
    };
    Some(WindowedBackendSignature {
        width: window.resolution.width(),
        height: window.resolution.height(),
        scale: window.resolution.scale_factor(),
    })
}

fn clear_windowed_backend_camera_state(world: &mut World) {
    if let Some(mut state) = world.get_resource_mut::<WindowedBackendCameraState>() {
        state.mismatch = None;
    }
}

fn camera_supports_windowed_webviews(world: &mut World) -> bool {
    let expected = {
        let mut window_q = world.query_filtered::<&Window, With<PrimaryWindow>>();
        let Ok(window) = window_q.single(world) else {
            return true;
        };
        let height = window.resolution.height().max(1.0);
        let aspect = window.resolution.width() / height;
        vmux_layout::scene::frame_main_camera_transform(window, aspect, 0.0)
    };
    let camera = {
        let mut camera_q =
            world.query_filtered::<&Transform, With<vmux_layout::scene::MainCamera>>();
        let Ok(camera) = camera_q.single(world) else {
            return true;
        };
        *camera
    };
    transform_near(&camera, &expected)
}

fn windowed_backend_should_use_windowed(
    world: &mut World,
    mode: vmux_layout::scene::InteractionMode,
) -> bool {
    if !webview_should_use_windowed(mode) {
        clear_windowed_backend_camera_state(world);
        return false;
    }
    if camera_supports_windowed_webviews(world) {
        clear_windowed_backend_camera_state(world);
        return true;
    }
    let Some(signature) = windowed_backend_signature(world) else {
        clear_windowed_backend_camera_state(world);
        return true;
    };
    if !world.contains_resource::<WindowedBackendCameraState>() {
        world.insert_resource(WindowedBackendCameraState::default());
    }
    let mut state = world.resource_mut::<WindowedBackendCameraState>();
    let should_keep_windowed = state.mismatch != Some(signature);
    state.mismatch = Some(signature);
    should_keep_windowed
}

/// The layout renders on the OSR mesh in both modes: a wgpu quad that resizes with the Bevy
/// frame, so it tracks a live window resize (a native overlay cannot — its frame only updates from a
/// Bevy schedule the macOS resize loop starves). Keep the material visible.
///
/// This drives the material's alpha rather than `Visibility`: the OSR focus pipeline treats a
/// `Visibility::Hidden` webview as hidden and tells CEF to stop rendering it. Keeping the entity
/// visible leaves OSR running. Alpha mode stays `Blend` so pages show through the layout's
/// transparent areas.
fn sync_layout_mesh_visibility(
    mode: Res<vmux_layout::scene::InteractionMode>,
    layout_q: Query<&WebviewMaterialHandle<WebviewExtendStandardMaterial>, With<LayoutCef>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let want_alpha = if *mode == vmux_layout::scene::InteractionMode::User {
        0.0
    } else {
        1.0
    };
    for mat_handle in &layout_q {
        let Some(mut material) = materials.get_mut(mat_handle.id()) else {
            continue;
        };
        if material.base.alpha_mode != AlphaMode::Blend {
            material.base.alpha_mode = AlphaMode::Blend;
        }
        if material.base.base_color.alpha() != want_alpha {
            material.base.base_color.set_alpha(want_alpha);
        }
    }
}

fn sync_cef_backend_for_interaction_mode(world: &mut World) {
    let mode = world
        .get_resource::<vmux_layout::scene::InteractionMode>()
        .copied()
        .unwrap_or_default();
    let base_windowed = windowed_backend_should_use_windowed(world, mode);
    let mut query = world.query_filtered::<(
        Entity,
        Has<LayoutCef>,
        Has<Modal>,
        Has<WebviewNativeOverlay>,
        Has<WebviewNativeDirectOverlay>,
    ), (With<Browser>, With<WebviewSource>)>();
    let entities: Vec<(Entity, bool, bool, bool, bool)> = query.iter(world).collect();
    // The command bar stays OSR. A windowed CEF child view never receives DOM input here: real
    // `NSEvent`s are dropped even when it holds first responder, and `send_key_event` forwarding is
    // a windowless API that produces no DOM key events. The bar is the one surface hosting a real
    // text field, so it needs the OSR path that input injection actually reaches.
    let target_windowed =
        |is_layout: bool, is_modal: bool| base_windowed && !is_layout && !is_modal;
    let target_native_overlay = |is_layout: bool, is_modal: bool| {
        cfg!(target_os = "macos")
            && mode == vmux_layout::scene::InteractionMode::User
            && (is_layout || is_modal)
    };
    let target_native_direct_overlay = |is_layout: bool| {
        cfg!(target_os = "macos") && mode == vmux_layout::scene::InteractionMode::User && is_layout
    };
    let mut recreate = Vec::new();
    {
        let browsers = world.non_send::<Browsers>();
        for &(entity, is_layout, is_modal, actual_native_overlay, actual_direct_overlay) in
            &entities
        {
            let has_browser = browsers.has_browser(entity);
            let actual_windowed = browsers.is_windowed(&entity);
            let want_windowed = target_windowed(is_layout, is_modal);
            let want_native_overlay = target_native_overlay(is_layout, is_modal);
            let want_native_direct_overlay = target_native_direct_overlay(is_layout);
            let needs_recreate = actual_windowed.is_some_and(|actual| actual != want_windowed)
                || has_browser
                    && (actual_native_overlay != want_native_overlay
                        || actual_direct_overlay != want_native_direct_overlay);
            if needs_recreate {
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
    for (entity, is_layout, is_modal, _, _) in entities {
        let want_windowed = target_windowed(is_layout, is_modal);
        let want_native_overlay = target_native_overlay(is_layout, is_modal);
        let want_native_direct_overlay = target_native_direct_overlay(is_layout);
        let marker_matches = world.get::<WebviewWindowed>(entity).is_some() == want_windowed;
        let overlay_matches =
            world.get::<WebviewNativeOverlay>(entity).is_some() == want_native_overlay;
        let direct_overlay_matches =
            world.get::<WebviewNativeDirectOverlay>(entity).is_some() == want_native_direct_overlay;
        let needs_recreate = recreate.contains(&entity);
        if marker_matches && overlay_matches && direct_overlay_matches && !needs_recreate {
            continue;
        }
        let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
            continue;
        };
        if want_windowed {
            entity_mut.insert(WebviewWindowed);
        } else {
            entity_mut.remove::<WebviewWindowed>();
        }
        if want_native_overlay {
            entity_mut.insert(WebviewNativeOverlay);
        } else {
            entity_mut.remove::<WebviewNativeOverlay>();
        }
        if want_native_direct_overlay {
            entity_mut.insert(WebviewNativeDirectOverlay);
        } else {
            entity_mut.remove::<WebviewNativeDirectOverlay>();
        }
        if needs_recreate {
            entity_mut
                .remove::<PageReady>()
                .remove::<PendingWebviewReveal>()
                .remove::<PendingCommandBarReveal>();
        }
    }
}

/// Deterministic, distinct ring color per agent (so multiple agents read apart).
fn agent_ring_rgb(key: &str) -> [f32; 3] {
    let mut h: u64 = 1469598103934665603;
    for b in key.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    hsl_to_rgb((h % 360) as f32, 0.85, 0.62)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m]
}

const CLAUDE_LOGO_PNG: &[u8] = include_bytes!("../assets/agent-logos/claude.png");
const CODEX_LOGO_PNG: &[u8] = include_bytes!("../assets/agent-logos/codex.png");
const VIBE_LOGO_PNG: &[u8] = include_bytes!("../assets/agent-logos/vibe.png");

/// A decoded, premultiplied-RGBA agent logo, ready to hand to the native badge.
struct LogoBitmap {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

fn decode_premultiplied(png: &[u8]) -> Option<LogoBitmap> {
    let img = image::load_from_memory(png).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    let mut rgba = img.into_raw();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u16;
        px[0] = (px[0] as u16 * a / 255) as u8;
        px[1] = (px[1] as u16 * a / 255) as u8;
        px[2] = (px[2] as u16 * a / 255) as u8;
    }
    Some(LogoBitmap {
        rgba,
        width,
        height,
    })
}

fn hex_to_rgb(hex: &str) -> Option<[f32; 3]> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WindowedHoverRefreshFrame {
    left_px: f32,
    top_px: f32,
    width_px: f32,
    height_px: f32,
    scale: f32,
}

fn windowed_hover_refresh_frame(
    computed: &ComputedNode,
    ui_gt: &UiGlobalTransform,
) -> Option<WindowedHoverRefreshFrame> {
    let size_px = computed.size;
    let scale = 1.0 / computed.inverse_scale_factor.max(1.0e-6);
    if size_px.x <= 0.0
        || size_px.y <= 0.0
        || !size_px.x.is_finite()
        || !size_px.y.is_finite()
        || !scale.is_finite()
        || scale <= 0.0
    {
        return None;
    }
    let center = ui_gt.transform_point2(Vec2::ZERO);
    Some(WindowedHoverRefreshFrame {
        left_px: center.x - size_px.x * 0.5,
        top_px: center.y - size_px.y * 0.5,
        width_px: size_px.x,
        height_px: size_px.y,
        scale,
    })
}

fn windowed_hover_refresh_position(
    cursor_px: Vec2,
    frame: WindowedHoverRefreshFrame,
) -> Option<Vec2> {
    if cursor_px.x < frame.left_px
        || cursor_px.x > frame.left_px + frame.width_px
        || cursor_px.y < frame.top_px
        || cursor_px.y > frame.top_px + frame.height_px
    {
        return None;
    }
    Some(Vec2::new(
        (cursor_px.x - frame.left_px) / frame.scale,
        (cursor_px.y - frame.top_px) / frame.scale,
    ))
}

#[derive(Default)]
struct LayoutHoverRefreshState {
    #[cfg(not(target_os = "macos"))]
    sequence: u64,
    #[cfg(not(target_os = "macos"))]
    position: Option<Vec2>,
    #[cfg(not(target_os = "macos"))]
    in_region: bool,
}

fn reset_layout_cef_hover(
    browsers: &Browsers,
    buttons: &ButtonInput<MouseButton>,
    layout: Entity,
    state: &mut LayoutHoverRefreshState,
) {
    #[cfg(target_os = "macos")]
    {
        let _ = (browsers, buttons, layout);
        NativeLayout::clear_pointer_state();
        *state = LayoutHoverRefreshState::default();
    }
    #[cfg(not(target_os = "macos"))]
    {
        if state.in_region {
            browsers.send_mouse_move(
                &layout,
                buttons.get_pressed(),
                state.position.unwrap_or_default(),
                true,
            );
        }
        *state = LayoutHoverRefreshState::default();
    }
}

#[derive(Default)]
struct WindowedHoverRefreshState {
    entity: Option<Entity>,
    position: Option<Vec2>,
}

const LAYOUT_INPUT_BURST: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Default)]
struct LayoutFrameRateState {
    native_sequence: u64,
    last_input: Option<std::time::Instant>,
    dragging_layout: bool,
}

/// One consistent view of the command bar for the AppKit event thread.
///
/// The `NSEvent` monitor samples this on every key and mouse event, at arbitrary points relative to
/// the Bevy frame that wrote it. Publishing openness, hit frame, and scale as a single value stops
/// it observing a combination no frame ever produced — a stored frame left behind by a closed bar
/// used to turn clicks inside that rectangle into dismiss requests.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CommandBarRoute {
    generation: u64,
    owns_input: bool,
    /// Present only while the surface is on screen; a revealing bar has no clickable rectangle.
    frame: Option<CommandBarWindowedFrame>,
    scale: f32,
}

static NATIVE_COMMAND_BAR_ROUTE: LazyLock<Mutex<CommandBarRoute>> =
    LazyLock::new(|| Mutex::new(CommandBarRoute::default()));
static NATIVE_COMMAND_BAR_DISMISS_REQUESTED: AtomicBool = AtomicBool::new(false);
static NATIVE_LEFT_MOUSE_DOWN: AtomicBool = AtomicBool::new(false);

fn native_command_bar_route() -> CommandBarRoute {
    *NATIVE_COMMAND_BAR_ROUTE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn native_command_bar_is_open() -> bool {
    native_command_bar_route().owns_input
}

pub fn set_native_left_mouse_down(down: bool) {
    NATIVE_LEFT_MOUSE_DOWN.store(down, Ordering::Relaxed);
}

pub fn native_left_mouse_down() -> bool {
    NATIVE_LEFT_MOUSE_DOWN.load(Ordering::Relaxed)
}

fn command_bar_windowed_click_should_dismiss(
    open: bool,
    button: MouseButton,
    state: ButtonState,
    cursor: Option<Vec2>,
    frame: Option<CommandBarWindowedFrame>,
) -> bool {
    if !open || button != MouseButton::Left || state != ButtonState::Pressed {
        return false;
    }
    let (Some(cursor), Some(frame)) = (cursor, frame) else {
        return false;
    };
    !command_bar_windowed_frame_contains(frame, cursor)
}

fn command_bar_windowed_frame_contains(frame: CommandBarWindowedFrame, cursor: Vec2) -> bool {
    cursor.x >= frame.left_px
        && cursor.x <= frame.left_px + frame.width_px
        && cursor.y >= frame.top_px
        && cursor.y <= frame.top_px + frame.height_px
}

pub fn request_native_command_bar_dismiss() -> bool {
    if !native_command_bar_route().owns_input {
        return false;
    }
    NATIVE_COMMAND_BAR_DISMISS_REQUESTED.store(true, Ordering::Relaxed);
    true
}

pub fn request_native_command_bar_dismiss_for_mouse_down(x_px: f32, y_px: f32) -> bool {
    if !x_px.is_finite() || !y_px.is_finite() {
        return false;
    }
    let route = native_command_bar_route();
    if !route.owns_input {
        return false;
    }
    let Some(frame) = route.frame else {
        return false;
    };
    if command_bar_windowed_frame_contains(frame, Vec2::new(x_px, y_px)) {
        return false;
    }
    NATIVE_COMMAND_BAR_DISMISS_REQUESTED.store(true, Ordering::Relaxed);
    true
}

pub fn take_native_command_bar_dismiss_requested() -> bool {
    NATIVE_COMMAND_BAR_DISMISS_REQUESTED.swap(false, Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LayoutWindowPadding {
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
}

fn val_px(value: Val) -> f32 {
    match value {
        Val::Px(px) => px,
        _ => 0.0,
    }
}

fn layout_window_padding_from_node(node: &Node) -> LayoutWindowPadding {
    LayoutWindowPadding {
        top: val_px(node.padding.top),
        right: val_px(node.padding.right),
        bottom: val_px(node.padding.bottom),
        left: val_px(node.padding.left),
    }
}

fn layout_window_padding_from_settings(settings: &AppSettings) -> LayoutWindowPadding {
    LayoutWindowPadding {
        top: settings.layout.window.pad_top(),
        right: settings.layout.window.pad_right(),
        bottom: settings.layout.window.pad_bottom(),
        left: settings.layout.window.pad_left(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LayoutFixedOffsets {
    left: f32,
    top: f32,
    right: f32,
    height: f32,
}

fn layout_fixed_offsets_from_computed(
    computed: &ComputedNode,
    transform: &UiGlobalTransform,
    window_width_px: f32,
) -> Option<LayoutFixedOffsets> {
    if computed.size.x <= 0.0 || computed.size.y <= 0.0 || window_width_px <= 0.0 {
        return None;
    }

    let inverse_scale = computed.inverse_scale_factor.max(1.0e-6);
    let size = computed.size * inverse_scale;
    let center = transform.transform_point2(Vec2::ZERO) * inverse_scale;
    let window_width = window_width_px * inverse_scale;
    let left = center.x - size.x * 0.5;
    let top = center.y - size.y * 0.5;
    let right = window_width - (center.x + size.x * 0.5);

    Some(LayoutFixedOffsets {
        left,
        top,
        right,
        height: size.y,
    })
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

fn tab_boundary_dir(
    tab: &Tab,
    settings: &AppSettings,
    active_space: Option<&vmux_space::spaces::ActiveSpace>,
) -> Option<(std::path::PathBuf, vmux_setting::DirSource)> {
    match tab.startup_dir.as_deref() {
        Some(path) => Some((
            vmux_setting::validate_tab_workspace_dir(path)
                .unwrap_or_else(|_| std::path::PathBuf::from(path)),
            vmux_setting::DirSource::Tab,
        )),
        None => {
            let active_space = active_space?;
            vmux_setting::resolve_startup_dir_for_tab_with_source(
                settings,
                &active_space.record.id,
                None,
            )
        }
    }
}

fn abbreviate_home(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy();
        if !home.is_empty()
            && let Some(rest) = s.strip_prefix(home.as_ref())
        {
            return format!("~{rest}");
        }
    }
    s.into_owned()
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

fn effective_title<'a>(osc: Option<&'a OscTitle>, default: &'a str) -> &'a str {
    match osc {
        Some(OscTitle(t)) if !t.is_empty() => t,
        _ => default,
    }
}

fn first_browser_meta<'a>(
    stack: Entity,
    stack_children: &Query<&Children>,
    browser_meta: &'a Query<(&PageMetadata, Option<&OscTitle>), With<Browser>>,
) -> Option<(&'a PageMetadata, Option<&'a OscTitle>)> {
    let kids = stack_children.get(stack).ok()?;
    kids.iter().find_map(|c| browser_meta.get(c).ok())
}

fn should_emit_update(
    current: &UpdateState,
    last: &Option<UpdateState>,
    page_ready_changed: bool,
) -> bool {
    last.as_ref() != Some(current) || (page_ready_changed && *current != UpdateState::Idle)
}

fn on_debug_update_ready(
    trigger: On<BinReceive<DebugUpdateReady>>,
    mut state: ResMut<UpdateState>,
) {
    *state = UpdateState::Ready {
        version: trigger.event().payload.version.clone(),
    };
}

fn on_debug_update_clear(
    _trigger: On<BinReceive<DebugUpdateClear>>,
    mut state: ResMut<UpdateState>,
) {
    *state = UpdateState::Idle;
}

fn knowledge_path_url(root: &Path, requested: &Path) -> Option<String> {
    let root = root.canonicalize().ok()?;
    let metadata = std::fs::symlink_metadata(requested).ok()?;
    if metadata.file_type().is_symlink() {
        return None;
    }
    let path = requested.canonicalize().ok()?;
    if !path.starts_with(&root) {
        return None;
    }
    let markdown = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md")
                || extension.eq_ignore_ascii_case("markdown")
                || extension.eq_ignore_ascii_case("mdx")
        });
    if !path.is_dir() && !markdown {
        return None;
    }
    url::Url::from_file_path(path)
        .ok()
        .map(|url| url.to_string())
}

fn normalize_vmux_url(url: &str) -> String {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("vmux://")
        && !rest.is_empty()
        && !rest.contains('/')
        && !rest.contains('?')
    {
        return format!("vmux://{rest}/");
    }
    url.to_string()
}

/// Marks a `PageOpenTask` the fallback has seen pending once. A `vmux://` scheme
/// owned by a `HandleKnownPages` handler can, under a rare command-visibility gap,
/// reach this fallback still pending in its first frame; this grace marker defers
/// the "unknown URL" verdict one run so the owning handler's mark becomes visible
/// before we error-claim (and permanently win the race for) an owned task.
#[derive(Component, Clone, Debug)]
struct PageOpenFallbackDeferred;

#[derive(Component, Clone, Debug)]
struct PageOpenAwaitSnapshot {
    started: std::time::Duration,
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
) -> Entity {
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
    browser
}

fn attach_error_page_to_stack(
    stack: Entity,
    display_url: &str,
    title: &str,
    message: &str,
    children_q: &Query<&Children>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let source = error_page_source(title, message, display_url);
    clear_stack_children(stack, children_q, commands);
    commands.entity(stack).insert(PageMetadata {
        url: display_url.to_string(),
        title: title.to_string(),
        ..default()
    });
    let browser = commands
        .spawn((
            Browser::new_error(meshes, webview_mt, &source, display_url, title),
            ChildOf(stack),
        ))
        .id();
    commands.entity(browser).insert(CefKeyboardTarget);
}

fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
}

fn error_page_source(title: &str, message: &str, url: &str) -> String {
    format!(
        "vmux://error/?title={}&message={}&url={}",
        percent_encode(title),
        percent_encode(message),
        percent_encode(url),
    )
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// A pending agent-initiated in-place navigation, keyed by the target webview.
/// Populated by `handle_browser_navigate_requests`; drained in `vmux_desktop`
/// (`drive_pending_nav_snapshots`) once the page settles, so the navigation's
/// agent command returns the post-load snapshot inline.
pub struct NavPending {
    pub request_id: [u8; 16],
    pub started: std::time::Duration,
    pub saw_loading: bool,
    pub pane: Option<String>,
}

#[derive(Resource, Default)]
pub struct PendingNavSnapshots(pub std::collections::HashMap<Entity, NavPending>);

fn cef_root_cache_path() -> Option<String> {
    vmux_core::profile::cef_cache_path()
}

#[cfg(test)]
mod debug_update_observer_tests {
    use super::*;
    use bevy_cef::prelude::BinReceive;

    #[test]
    fn debug_ready_sets_state_then_clear_resets() {
        let mut app = App::new();
        app.init_resource::<UpdateState>()
            .add_observer(on_debug_update_ready)
            .add_observer(on_debug_update_clear);

        app.world_mut().trigger(BinReceive::<DebugUpdateReady> {
            webview: Entity::PLACEHOLDER,
            payload: DebugUpdateReady {
                version: "v9.0.0".into(),
            },
        });
        assert_eq!(
            *app.world().resource::<UpdateState>(),
            UpdateState::Ready {
                version: "v9.0.0".into()
            }
        );

        app.world_mut().trigger(BinReceive::<DebugUpdateClear> {
            webview: Entity::PLACEHOLDER,
            payload: DebugUpdateClear,
        });
        assert_eq!(*app.world().resource::<UpdateState>(), UpdateState::Idle);
    }
}

#[cfg(test)]
mod error_page_source_tests {
    use super::{error_page_source, percent_encode};

    #[test]
    fn percent_encode_escapes_reserved_keeps_unreserved() {
        assert_eq!(percent_encode("a b/&"), "a%20b%2F%26");
        assert_eq!(percent_encode("v0.0.1-rc~_"), "v0.0.1-rc~_");
    }

    #[test]
    fn error_page_source_builds_query() {
        assert_eq!(
            error_page_source("Page not found", "", "vmux://debug/"),
            "vmux://error/?title=Page%20not%20found&message=&url=vmux%3A%2F%2Fdebug%2F"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::sync_appearance_to_cef;
    use vmux_terminal::Terminal;

    #[test]
    fn cef_disables_bfcache_for_extension_ports() {
        assert!(
            cef_command_line_config()
                .switch_values
                .contains(&("disable-features", "BackForwardCache"))
        );
    }

    #[test]
    fn knowledge_paths_only_open_vault_markdown_and_directories() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("knowledge");
        let folder = vault.join("projects");
        std::fs::create_dir_all(&folder).unwrap();
        let note = folder.join("brief.md");
        let text = folder.join("brief.txt");
        let outside = temp.path().join("outside.md");
        std::fs::write(&note, "# Brief").unwrap();
        std::fs::write(&text, "Brief").unwrap();
        std::fs::write(&outside, "# Outside").unwrap();

        assert!(knowledge_path_url(&vault, &vault).is_some());
        assert!(knowledge_path_url(&vault, &folder).is_some());
        assert!(knowledge_path_url(&vault, &note).is_some());
        assert!(knowledge_path_url(&vault, &text).is_none());
        assert!(knowledge_path_url(&vault, &outside).is_none());
    }

    #[test]
    fn stored_tab_dir_is_sidebar_source_of_truth() {
        let tab = Tab {
            name: "test".into(),
            startup_dir: Some("/tmp/agent-checkout".into()),
        };
        let settings = test_app_settings_with_radius(0.0);

        assert_eq!(
            tab_boundary_dir(&tab, &settings, None),
            Some((
                std::path::PathBuf::from("/tmp/agent-checkout"),
                vmux_setting::DirSource::Tab,
            ))
        );
    }

    #[test]
    fn legacy_tab_boundary_uses_space_fallback_without_migration() {
        let dir = std::env::temp_dir();
        let record = vmux_space::model::bootstrap_space_record();
        let mut settings = test_app_settings_with_radius(0.0);
        settings.spaces.insert(
            record.id.clone(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(dir.to_string_lossy().into_owned()),
            },
        );
        let tab = Tab::default();

        let (path, source) = tab_boundary_dir(
            &tab,
            &settings,
            Some(&vmux_space::spaces::ActiveSpace { record }),
        )
        .unwrap();

        assert_eq!(path, dir);
        assert_eq!(source, vmux_setting::DirSource::Space);
        assert_eq!(tab.startup_dir, None);
    }

    #[test]
    fn normalize_vmux_url_trims_and_adds_trailing_slash_to_bare_host() {
        assert_eq!(normalize_vmux_url("vmux://lsp"), "vmux://lsp/");
        assert_eq!(normalize_vmux_url("vmux://terminal"), "vmux://terminal/");
        assert_eq!(normalize_vmux_url("vmux://lsp/"), "vmux://lsp/");
        assert_eq!(
            normalize_vmux_url("vmux://agent/vibe/"),
            "vmux://agent/vibe/"
        );
        assert_eq!(
            normalize_vmux_url("vmux://error/?title=x"),
            "vmux://error/?title=x"
        );
        assert_eq!(
            normalize_vmux_url("file:///tmp/main.rs"),
            "file:///tmp/main.rs"
        );
        assert_eq!(
            normalize_vmux_url("  vmux://agent/codex/session-id  "),
            "vmux://agent/codex/session-id"
        );
    }

    #[test]
    fn effective_title_prefers_nonempty_osc() {
        use vmux_core::OscTitle;
        assert_eq!(
            effective_title(Some(&OscTitle("osc".to_string())), "def"),
            "osc"
        );
        assert_eq!(
            effective_title(Some(&OscTitle(String::new())), "def"),
            "def"
        );
        assert_eq!(effective_title(None, "def"), "def");
    }

    fn layout_material_after_mode(
        mode: vmux_layout::scene::InteractionMode,
        initial_alpha: f32,
    ) -> WebviewExtendStandardMaterial {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .insert_resource(mode)
            .add_systems(Update, sync_layout_mesh_visibility);
        let mut material = WebviewExtendStandardMaterial::default();
        material.base.alpha_mode = AlphaMode::Blend;
        material.base.base_color.set_alpha(initial_alpha);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WebviewExtendStandardMaterial>>()
            .add(material);
        app.world_mut()
            .spawn((LayoutCef, WebviewMaterialHandle(handle.clone())));

        app.update();

        app.world()
            .resource::<Assets<WebviewExtendStandardMaterial>>()
            .get(handle.id())
            .expect("layout material")
            .clone()
    }

    #[test]
    fn user_mode_hides_layout_mesh_behind_native_overlay() {
        let mat = layout_material_after_mode(vmux_layout::scene::InteractionMode::User, 1.0);
        assert_eq!(
            mat.base.base_color.alpha(),
            0.0,
            "User mode presents layout chrome through the native accelerated overlay"
        );
        assert_eq!(mat.base.alpha_mode, AlphaMode::Blend);
    }

    #[test]
    fn player_mode_makes_layout_mesh_visible_and_transparent() {
        let mat = layout_material_after_mode(vmux_layout::scene::InteractionMode::Player, 0.0);
        assert_eq!(
            mat.base.base_color.alpha(),
            1.0,
            "Player mode renders the layout via the mesh, so it must be visible"
        );
        assert_eq!(
            mat.base.alpha_mode,
            AlphaMode::Blend,
            "Player uses straight alpha so pages show through the layout's transparent areas"
        );
    }

    #[test]
    fn agent_cli_url_redirects_tab_to_session_id() {
        let mut app = App::new();
        app.add_systems(Update, crate::navigation::sync_page_metadata_to_tab);

        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "vmux://agent/vibe/".to_string(),
                    ..default()
                },
            ))
            .id();
        let child = app
            .world_mut()
            .spawn((
                Browser,
                PageMetadata {
                    url: "vmux://agent/vibe/".to_string(),
                    ..default()
                },
                ChildOf(stack),
            ))
            .id();

        app.update();

        app.world_mut().get_mut::<PageMetadata>(child).unwrap().url =
            "vmux://agent/vibe/abc-123".to_string();

        app.update();

        let stack_url = app.world().get::<PageMetadata>(stack).unwrap().url.clone();
        assert_eq!(stack_url, "vmux://agent/vibe/abc-123");
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
    fn layout_fixed_offsets_use_computed_header_rect() {
        let computed = ComputedNode {
            size: Vec2::new(1_544.0, 168.0),
            inverse_scale_factor: 0.5,
            ..default()
        };
        let transform = UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(
            788.0, 84.0,
        )));

        let offsets =
            layout_fixed_offsets_from_computed(&computed, &transform, 1_600.0).expect("offsets");

        assert_eq!(offsets.left, 8.0);
        assert_eq!(offsets.top, 0.0);
        assert_eq!(offsets.right, 20.0);
        assert_eq!(offsets.height, 84.0);
    }

    pub(crate) fn test_app_settings_with_radius(radius: f32) -> AppSettings {
        AppSettings {
            browser: vmux_setting::BrowserSettings {
                startup_url: "about:blank".to_string(),
                ..Default::default()
            },
            layout: vmux_layout::settings::LayoutSettings {
                radius,
                window: vmux_layout::settings::WindowSettings { padding: 0.0 },
                pane: vmux_layout::settings::PaneSettings { gap: 0.0 },
                side_sheet: vmux_layout::settings::SideSheetSettings::default(),
                focus_ring: vmux_layout::settings::FocusRingSettings::default(),
            },
            shortcuts: vmux_setting::ShortcutSettings::default(),
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
    fn appearance_change_updates_cef_color_scheme() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_app_settings_with_radius(0.0))
            .init_resource::<CefColorScheme>()
            .add_systems(
                Update,
                sync_appearance_to_cef.run_if(resource_changed::<AppSettings>),
            );
        app.update();
        app.world_mut()
            .resource_mut::<AppSettings>()
            .appearance
            .mode = vmux_setting::ColorScheme::Light;
        app.update();
        assert_eq!(
            app.world().resource::<CefColorScheme>().0,
            CefColorMode::Light
        );
    }

    #[test]
    fn windowed_hover_refresh_position_maps_physical_cursor_to_webview_space() {
        let frame = WindowedHoverRefreshFrame {
            left_px: 100.0,
            top_px: 50.0,
            width_px: 400.0,
            height_px: 300.0,
            scale: 2.0,
        };

        assert_eq!(
            windowed_hover_refresh_position(Vec2::new(300.0, 250.0), frame),
            Some(Vec2::new(100.0, 100.0))
        );
    }

    #[test]
    fn windowed_hover_refresh_position_ignores_cursor_outside_frame() {
        let frame = WindowedHoverRefreshFrame {
            left_px: 100.0,
            top_px: 50.0,
            width_px: 400.0,
            height_px: 300.0,
            scale: 2.0,
        };

        assert_eq!(
            windowed_hover_refresh_position(Vec2::new(99.0, 250.0), frame),
            None
        );
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
    fn browser_mode_keeps_layout_and_modal_osr_and_windows_pages_on_macos() {
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
        let terminal = app
            .world_mut()
            .spawn((Browser, Terminal, WebviewSource::new("vmux://terminal/")))
            .id();

        sync_cef_backend_for_interaction_mode(app.world_mut());

        assert!(app.world().get::<WebviewWindowed>(layout).is_none());
        assert_eq!(
            app.world().get::<WebviewNativeOverlay>(layout).is_some(),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            app.world()
                .get::<WebviewNativeDirectOverlay>(layout)
                .is_some(),
            cfg!(target_os = "macos")
        );
        assert!(
            app.world()
                .get::<WebviewNativeDirectOverlay>(modal)
                .is_none()
        );
        assert_eq!(
            app.world().get::<WebviewNativeOverlay>(modal).is_some(),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            app.world().get::<WebviewWindowed>(terminal).is_some(),
            cfg!(target_os = "macos")
        );
        assert!(app.world().get::<WebviewWindowed>(modal).is_none());
        assert_eq!(
            app.world().get::<WebviewWindowed>(page).is_some(),
            cfg!(target_os = "macos")
        );
    }

    #[test]
    fn user_player_user_backend_round_trip() {
        let mut app = App::new();
        app.world_mut().insert_non_send(Browsers::default());
        app.insert_resource(vmux_layout::scene::InteractionMode::User);
        let window = Window {
            resolution: (800, 600).into(),
            ..default()
        };
        let home = vmux_layout::scene::frame_main_camera_transform(&window, 800.0 / 600.0, 0.0);
        app.world_mut().spawn((window, PrimaryWindow));
        app.world_mut()
            .spawn((vmux_layout::scene::MainCamera, home));

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
        app.insert_resource(vmux_layout::scene::InteractionMode::Player);
        sync_cef_backend_for_interaction_mode(app.world_mut());
        app.insert_resource(vmux_layout::scene::InteractionMode::User);
        sync_cef_backend_for_interaction_mode(app.world_mut());

        assert!(app.world().get::<WebviewWindowed>(layout).is_none());
        assert_eq!(
            app.world().get::<WebviewNativeOverlay>(layout).is_some(),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            app.world()
                .get::<WebviewNativeDirectOverlay>(layout)
                .is_some(),
            cfg!(target_os = "macos")
        );
        assert!(app.world().get::<WebviewWindowed>(modal).is_none());
        assert!(
            app.world()
                .get::<WebviewNativeDirectOverlay>(modal)
                .is_none()
        );
        assert_eq!(
            app.world().get::<WebviewNativeOverlay>(modal).is_some(),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            app.world().get::<WebviewWindowed>(page).is_some(),
            cfg!(target_os = "macos")
        );
    }

    #[test]
    fn browser_mode_disables_windowed_pages_when_camera_is_off_axis() {
        let mut app = App::new();
        app.world_mut().insert_non_send(Browsers::default());
        app.insert_resource(vmux_layout::scene::InteractionMode::User);
        app.world_mut().spawn((
            Window {
                resolution: (800, 600).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut().spawn((
            vmux_layout::scene::MainCamera,
            Transform::from_xyz(2.0, 1.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        let page = app
            .world_mut()
            .spawn((
                Browser,
                WebviewWindowed,
                WebviewSource::new("https://example.com/"),
            ))
            .id();

        sync_cef_backend_for_interaction_mode(app.world_mut());
        sync_cef_backend_for_interaction_mode(app.world_mut());

        assert!(app.world().get::<WebviewWindowed>(page).is_none());
    }

    #[test]
    fn browser_mode_keeps_windowed_pages_for_first_resize_camera_mismatch() {
        let mut app = App::new();
        app.world_mut().insert_non_send(Browsers::default());
        app.insert_resource(vmux_layout::scene::InteractionMode::User);
        let old_window = Window {
            resolution: (800, 600).into(),
            ..default()
        };
        let stale_home =
            vmux_layout::scene::frame_main_camera_transform(&old_window, 800.0 / 600.0, 0.0);
        app.world_mut().spawn((
            Window {
                resolution: (1200, 900).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut()
            .spawn((vmux_layout::scene::MainCamera, stale_home));
        let page = app
            .world_mut()
            .spawn((
                Browser,
                WebviewWindowed,
                WebviewSource::new("https://example.com/"),
            ))
            .id();

        sync_cef_backend_for_interaction_mode(app.world_mut());

        assert_eq!(
            app.world().get::<WebviewWindowed>(page).is_some(),
            cfg!(target_os = "macos")
        );
    }

    #[test]
    fn browser_mode_keeps_windowed_pages_when_camera_is_home() {
        let mut app = App::new();
        app.world_mut().insert_non_send(Browsers::default());
        app.insert_resource(vmux_layout::scene::InteractionMode::User);
        let window = Window {
            resolution: (800, 600).into(),
            ..default()
        };
        let home = vmux_layout::scene::frame_main_camera_transform(&window, 800.0 / 600.0, 0.0);
        app.world_mut().spawn((window, PrimaryWindow));
        app.world_mut()
            .spawn((vmux_layout::scene::MainCamera, home));
        let page = app
            .world_mut()
            .spawn((Browser, WebviewSource::new("https://example.com/")))
            .id();

        sync_cef_backend_for_interaction_mode(app.world_mut());

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

    #[derive(Resource, Default)]
    struct ObservedBackendMode(Option<vmux_layout::scene::InteractionMode>);

    fn finish_exit_for_backend_sync_test(mut mode: ResMut<vmux_layout::scene::InteractionMode>) {
        *mode = vmux_layout::scene::InteractionMode::User;
    }

    fn observe_backend_sync_mode(
        mode: Res<vmux_layout::scene::InteractionMode>,
        mut observed: ResMut<ObservedBackendMode>,
    ) {
        observed.0 = Some(*mode);
    }

    #[test]
    fn backend_sync_runs_after_exit_transition_completion() {
        let mut app = App::new();
        app.world_mut().insert_non_send(Browsers::default());
        configure_cef_backend_sync(&mut app)
            .insert_resource(vmux_layout::scene::InteractionMode::Player)
            .init_resource::<ObservedBackendMode>()
            .add_systems(
                Update,
                finish_exit_for_backend_sync_test
                    .in_set(vmux_layout::scene::SceneSystems::CompleteModeTransition),
            )
            .add_systems(
                Update,
                observe_backend_sync_mode
                    .in_set(BrowserSystems::SyncCefBackend)
                    .before(sync_cef_backend_for_interaction_mode),
            );

        app.update();

        assert!(
            app.world().resource::<ObservedBackendMode>().0
                == Some(vmux_layout::scene::InteractionMode::User)
        );
    }

    #[test]
    fn native_layout_pointer_queue_retains_only_latest_sample() {
        let source = include_str!("native_layout/macos.rs");
        let queue = source
            .split("pub fn queue_pointer_move")
            .nth(1)
            .and_then(|tail| tail.split("pub fn flush_pointer_move").next())
            .unwrap_or_default();
        let flush = source
            .split("pub fn flush_pointer_move")
            .nth(1)
            .and_then(|tail| tail.split("pub fn forward_scroll").next())
            .unwrap_or_default();
        let sample = source
            .split("fn queue_sample")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(test)]").next())
            .unwrap_or_default();

        assert!(sample.contains("self.position_px = Some(position)"));
        assert!(sample.contains("self.buttons = buttons"));
        assert!(source.contains("fn queue_sample"));
        assert!(sample.contains("sample_changed"));
        assert!(sample.contains("self.pending = true"));
        assert!(queue.contains("state.queue_sample("));
        assert!(flush.contains("state.pending = false"));
        assert!(flush.contains("presenter.send(position_px / state.scale"));
    }

    #[test]
    fn layout_pointer_regions_match_layout_coordinates() {
        let rect = CefPointerHitRect {
            center: Vec2::new(50.0, 25.0),
            size: Vec2::new(20.0, 10.0),
            interactive: true,
        };

        assert!(cef_pointer_hit_rect_contains(rect, Vec2::new(50.0, 25.0)));
        assert!(!cef_pointer_hit_rect_contains(rect, Vec2::new(39.0, 25.0)));
    }

    #[test]
    fn active_browser_url_wins_over_stale_new_stack_placeholder() {
        let stack = Entity::from_bits(1);
        let rows = [StackRow {
            title: "Google".to_string(),
            url: "https://www.google.com".to_string(),
            icon: vmux_core::PageIcon::None,
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

    #[test]
    fn layout_state_padding_reads_effective_window_node_padding() {
        let node = Node {
            padding: UiRect {
                top: Val::Px(10.0),
                right: Val::Px(11.0),
                bottom: Val::Px(12.0),
                left: Val::Px(13.0),
            },
            ..default()
        };

        assert_eq!(
            layout_window_padding_from_node(&node),
            LayoutWindowPadding {
                top: 10.0,
                right: 11.0,
                bottom: 12.0,
                left: 13.0,
            }
        );
    }

    mod browser_navigate_flow {
        use crate::input::RecentBrowserInteraction;
        use crate::{Browser, PendingNavSnapshots};
        use bevy::ecs::relationship::Relationship;
        use bevy::prelude::*;
        use bevy_cef::prelude::WebviewExtendStandardMaterial;
        use vmux_agent::events::AgentCommandRequest;
        use vmux_agent::plugin::AgentSessionPlugin;
        use vmux_agent::strategy::AgentStrategies;
        use vmux_core::{
            CefPageAttachRequest, LastActivatedAt, PageMetadata, PageOpenError, PageOpenHandled,
            PageOpenId, PageOpenRequest, PageOpenSet, PageOpenTask,
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

        struct ConsumerPlugin;

        impl Plugin for ConsumerPlugin {
            fn build(&self, app: &mut App) {
                app.add_plugins((
                    vmux_layout::LayoutContractPlugin,
                    vmux_terminal::TerminalContractPlugin,
                ))
                .add_message::<PageOpenRequest>()
                .add_message::<CefPageAttachRequest>()
                .add_message::<vmux_setting::SettingsWriteRequest>()
                .add_message::<vmux_space::SpaceCommandRequest>()
                .add_message::<vmux_history::query::HistoryOpenIntent>()
                .init_resource::<crate::PendingNavSnapshots>()
                .init_resource::<crate::input::RecentBrowserInteraction>()
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
                        crate::navigation::handle_browser_navigate_requests
                            .before(PageOpenSet::ResolveTarget),
                        crate::page_open::handle_page_open_requests
                            .in_set(PageOpenSet::ResolveTarget),
                        handle_test_known_page_open.in_set(PageOpenSet::HandleKnownPages),
                        crate::page_open::attach_cef_page_requests.in_set(PageOpenSet::Fallback),
                        crate::page_open::handle_unclaimed_page_open_tasks
                            .in_set(PageOpenSet::Fallback),
                        crate::page_open::respond_page_open_tasks.in_set(PageOpenSet::Respond),
                        vmux_terminal::handle_terminal_send_requests,
                        vmux_terminal::handle_run_shell_requests,
                    ),
                );
            }
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
                    commands.spawn((Browser, Terminal, ChildOf(task.stack)));
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
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
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
                    origin: vmux_service::agent_events::CommandOrigin::User,
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
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
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
                    origin: vmux_service::agent_events::CommandOrigin::User,
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
        fn agent_browser_navigate_stacks_new_page_and_waits_for_snapshot() {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ConsumerPlugin));
            app.insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>();

            let pane = app.world_mut().spawn(Pane).id();
            let first_stack = app
                .world_mut()
                .spawn((
                    vmux_layout::stack::stack_bundle(),
                    LastActivatedAt(1),
                    ChildOf(pane),
                ))
                .id();
            app.world_mut().spawn((Browser, ChildOf(first_stack)));
            let request_id = [7; 16];
            app.world_mut()
                .resource_mut::<Messages<vmux_layout::BrowserNavigateRequest>>()
                .write(vmux_layout::BrowserNavigateRequest {
                    url: "https://second.example".into(),
                    pane: Some(pane.to_bits().to_string()),
                    request_id: Some(request_id),
                    new_stack: true,
                    profile: Some("agent-1".into()),
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let mut stacks = world.query_filtered::<
                (Entity, &PageMetadata, &LastActivatedAt),
                With<vmux_layout::stack::Stack>,
            >();
            let second = stacks
                .iter(world)
                .find(|(_, metadata, _)| metadata.url == "https://second.example")
                .map(|(entity, _, activated)| (entity, activated.0))
                .expect("new browser stack");
            assert_ne!(second.0, first_stack);
            assert!(second.1 > 1);
            assert_eq!(world.resource::<PendingNavSnapshots>().0.len(), 1);
            assert_eq!(
                world
                    .resource::<PendingNavSnapshots>()
                    .0
                    .values()
                    .next()
                    .unwrap()
                    .request_id,
                request_id
            );
        }

        #[test]
        fn agent_browser_navigate_does_not_raise_new_stack_during_user_interaction() {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ConsumerPlugin));
            app.insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>();

            let pane = app.world_mut().spawn(Pane).id();
            let first_stack = app
                .world_mut()
                .spawn((
                    vmux_layout::stack::stack_bundle(),
                    LastActivatedAt(10),
                    ChildOf(pane),
                ))
                .id();
            app.world_mut().spawn((Browser, ChildOf(first_stack)));
            app.insert_resource(RecentBrowserInteraction {
                stack: Some(first_stack),
                at: Some(std::time::Instant::now()),
            });
            app.world_mut()
                .resource_mut::<Messages<vmux_layout::BrowserNavigateRequest>>()
                .write(vmux_layout::BrowserNavigateRequest {
                    url: "https://second.example".into(),
                    pane: Some(pane.to_bits().to_string()),
                    request_id: None,
                    new_stack: true,
                    profile: Some("agent-1".into()),
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let mut stacks = world
            .query_filtered::<(&PageMetadata, &LastActivatedAt), With<vmux_layout::stack::Stack>>();
            let activated = stacks
                .iter(world)
                .find(|(metadata, _)| metadata.url == "https://second.example")
                .map(|(_, activated)| activated.0)
                .expect("new browser stack");
            assert_eq!(activated, 0);
        }

        #[test]
        fn browser_navigate_targets_specific_pane_when_id_provided() {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
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
                    origin: vmux_service::agent_events::CommandOrigin::User,
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
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<WebviewExtendStandardMaterial>>();

            let pane = app.world_mut().spawn(Pane).id();
            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
            let request_id = AgentRequestId::new();

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id,
                    origin: vmux_service::agent_events::CommandOrigin::User,
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
            assert!(
                world
                    .resource::<PendingNavSnapshots>()
                    .0
                    .values()
                    .any(|pending| pending.request_id == request_id.0),
                "terminal navigation should wait for its snapshot"
            );
        }

        #[test]
        fn browser_navigate_with_terminal_url_and_target_pane_uses_target() {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
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
                    origin: vmux_service::agent_events::CommandOrigin::User,
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
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
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
                    origin: vmux_service::agent_events::CommandOrigin::User,
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "vmux://nonsense/".to_string(),
                        pane: None,
                    },
                });

            // One extra update vs. the other navigate tests: the fallback now grants
            // unknown `vmux://` URLs a one-frame grace before rendering the error page.
            app.update();
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
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
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
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
            app.init_resource::<AgentStrategies>()
                .insert_resource(vmux_agent::plugin::AgentExecutableOverride(
                    std::collections::HashMap::from([(vmux_core::agent::AgentKind::Claude, true)]),
                ))
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
                    origin: vmux_service::agent_events::CommandOrigin::User,
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
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
            app.init_resource::<AgentStrategies>()
                .insert_resource(vmux_agent::plugin::AgentExecutableOverride(
                    std::collections::HashMap::from([(vmux_core::agent::AgentKind::Codex, true)]),
                ))
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
                    origin: vmux_service::agent_events::CommandOrigin::User,
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
        use vmux_command::{AppCommand, BrowserCommand, BrowserViewCommand};
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
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                vmux_terminal::TerminalContractPlugin,
                crate::command::CommandPlugin,
            ))
            .add_message::<PageOpenRequest>()
            .add_systems(
                Update,
                capture_page_open_requests.after(vmux_command::ReadAppCommands),
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
            for host in [
                "terminal", "agent", "services", "settings", "team", "spaces",
            ] {
                vmux_core::register_host_spawn(&mut app, host);
            }
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
                        icon: vmux_core::PageIcon::None,
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
        fn in_place_from_plain_vmux_to_web_navigates_in_place() {
            let mut app = build_app();
            build_focused_native_stack(&mut app, "vmux://history/");

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("https://mistral.ai".into()),
                    },
                )));

            app.update();

            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert!(page_opens.0.is_empty());
            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert_eq!(navigates.0, vec!["https://mistral.ai".to_string()]);
        }

        #[test]
        fn in_place_from_web_to_plain_vmux_navigates_in_place() {
            let mut app = build_app();
            build_focused_native_stack(&mut app, "https://example.com/");

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("vmux://history/".into()),
                    },
                )));

            app.update();

            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert!(page_opens.0.is_empty());
            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert_eq!(navigates.0, vec!["vmux://history/".to_string()]);
        }

        #[test]
        fn in_place_to_settings_routes_through_page_open() {
            let mut app = build_app();
            build_focused_native_stack(&mut app, "https://example.com/");

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("vmux://settings/".into()),
                    },
                )));

            app.update();

            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert!(navigates.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert_eq!(page_opens.0.len(), 1);
            assert_eq!(page_opens.0[0].url, "vmux://settings/");
            assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
        }

        #[test]
        fn in_place_to_terminal_routes_through_page_open() {
            let mut app = build_app();
            build_focused_native_stack(&mut app, "vmux://settings/");

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("vmux://terminal/".into()),
                    },
                )));

            app.update();

            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert!(navigates.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert_eq!(page_opens.0.len(), 1);
            assert_eq!(page_opens.0[0].url, "vmux://terminal/");
            assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
        }

        #[test]
        fn in_place_to_file_routes_through_page_open() {
            let mut app = build_app();
            build_focused_native_stack(&mut app, "https://example.com/");

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("file:///tmp/x".into()),
                    },
                )));

            app.update();

            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert!(navigates.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert_eq!(page_opens.0.len(), 1);
            assert_eq!(page_opens.0[0].url, "file:///tmp/x");
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
        fn zoom_in_on_terminal_emits_font_size_increase() {
            use bevy::ecs::message::Messages;

            let mut app = build_app();
            build_focused_terminal_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::View(
                    BrowserViewCommand::ZoomIn,
                )));

            app.update();

            let cmds: Vec<vmux_terminal::TerminalFontSizeCommand> = app
                .world_mut()
                .resource_mut::<Messages<vmux_terminal::TerminalFontSizeCommand>>()
                .drain()
                .collect();
            assert_eq!(cmds, vec![vmux_terminal::TerminalFontSizeCommand::Increase]);
        }

        #[test]
        fn zoom_reset_on_terminal_emits_font_size_reset() {
            use bevy::ecs::message::Messages;

            let mut app = build_app();
            build_focused_terminal_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::View(
                    BrowserViewCommand::ZoomReset,
                )));

            app.update();

            let cmds: Vec<vmux_terminal::TerminalFontSizeCommand> = app
                .world_mut()
                .resource_mut::<Messages<vmux_terminal::TerminalFontSizeCommand>>()
                .drain()
                .collect();
            assert_eq!(cmds, vec![vmux_terminal::TerminalFontSizeCommand::Reset]);
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

#[cfg(test)]
mod update_notice_tests {
    use super::should_emit_update;
    use vmux_layout::UpdateState;

    fn downloading(v: &str) -> UpdateState {
        UpdateState::Downloading {
            version: v.into(),
            downloaded: 1,
            total: 2,
        }
    }

    #[test]
    fn emits_on_change() {
        assert!(should_emit_update(
            &UpdateState::Ready {
                version: "v2".into()
            },
            &None,
            false
        ));
        assert!(should_emit_update(
            &UpdateState::Idle,
            &Some(downloading("v2")),
            false
        ));
    }

    #[test]
    fn no_emit_when_unchanged_and_no_page_ready() {
        assert!(!should_emit_update(
            &UpdateState::Idle,
            &Some(UpdateState::Idle),
            false
        ));
        let r = UpdateState::Ready {
            version: "v2".into(),
        };
        assert!(!should_emit_update(&r, &Some(r.clone()), false));
    }

    #[test]
    fn re_emits_non_idle_on_page_ready() {
        let r = UpdateState::Ready {
            version: "v2".into(),
        };
        assert!(should_emit_update(&r, &Some(r.clone()), true));
        assert!(!should_emit_update(
            &UpdateState::Idle,
            &Some(UpdateState::Idle),
            true
        ));
    }
}
