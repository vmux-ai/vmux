//! CEF pane spawn (mesh + webview) for layout leaves.

use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use bevy_cef::prelude::*;
use vmux_core::{SessionSavePath, SessionSaveQueue};
use vmux_settings::VmuxAppSettings;

use crate::loading_bar::{LoadingBarMaterial, PaneChromeLoadingBar};
use crate::{
    Active, History, HistoryPaneNeedsUrl, HistoryPaneOpenedAt, LastVisitedUrl, LayoutNode,
    Layout, Pane, Profile, Tab, Webview, Workspace,
    PaneChromeNeedsUrl, PaneChromeOwner, PaneChromeStrip, PaneLastUrl, SavedLayoutNode,
    SessionLayoutSnapshot, allowed_navigation_url, initial_webview_url,
    legacy_loopback_embedded_history_ui_url, sanitize_embedded_webview_url,
};

/// CEF page zoom; `0.0` matches typical desktop browsers at 100%.
pub const CEF_PAGE_ZOOM_LEVEL: f64 = 0.0;

/// Reports **top-frame** `location.href` to Bevy via `window.cef.emit({ url })` (pageshow, SPA history, retry until `cef` exists).
///
/// Iframes (e.g. YouTube embeds on Google) must not emit: they would overwrite session URLs with the
/// embed origin while the visible page stays on Google.
pub const URL_TRACK_PRELOAD: &str = r#"(function(){function e(){try{if(window.self!==window.top)return;if(typeof window!=="undefined"&&window.cef&&typeof window.cef.emit==="function")window.cef.emit({url:location.href});}catch(_){}}function t(){e()}var n=history.pushState,r=history.replaceState;history.pushState=function(){n.apply(history,arguments);setTimeout(t,0)};history.replaceState=function(){r.apply(history,arguments);setTimeout(t,0)};window.addEventListener("popstate",function(){setTimeout(t,0)});window.addEventListener("pageshow",function(){setTimeout(t,0)});var i=0,o=setInterval(function(){e();(window.cef&&window.cef.emit||++i>200)&&clearInterval(o)},50)})();"#;

/// Emacs-style readline bindings for `<input>` / `<textarea>` (source: [`vmux_input::TEXT_INPUT_EMACS_BINDINGS_PRELOAD`]).
pub const TEXT_INPUT_EMACS_BINDINGS_PRELOAD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../vmux_input/src/text_input_emacs_bindings.js"
));

/// Placeholder until `vmux_webview` sets the real status URL from the embedded server or env.
const CHROME_LOADING_HTML: &str = r#"<!DOCTYPE html><html><head><meta charset="utf-8"/><style>html,body{margin:0;background:#1a1a1a;height:100%;}</style></head><body></body></html>"#;

fn spawn_pane_chrome(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WebviewExtendStandardMaterial>,
    loading_bar_materials: &mut Assets<LoadingBarMaterial>,
    pane: Entity,
) {
    let chrome_mesh = meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE));
    let loading_mesh = meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE));
    commands
        .spawn((
            PaneChromeStrip,
            PaneChromeOwner(pane),
            Visibility::Visible,
            PaneChromeNeedsUrl,
            WebviewSource::inline(CHROME_LOADING_HTML),
            PreloadScripts::default(),
            ZoomLevel(CEF_PAGE_ZOOM_LEVEL),
            Mesh3d(chrome_mesh),
            MeshMaterial3d(materials.add(WebviewExtendStandardMaterial {
                base: StandardMaterial {
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    base_color: Color::WHITE,
                    depth_bias: 1_000_000.0,
                    ..default()
                },
                extension: WebviewMaterial::default(),
            })),
        ));
    commands.spawn((
        PaneChromeLoadingBar,
        PaneChromeOwner(pane),
        Visibility::Hidden,
        Mesh3d(loading_mesh),
        MeshMaterial3d(loading_bar_materials.add(LoadingBarMaterial::default())),
    ));
}

pub fn spawn_pane(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WebviewExtendStandardMaterial>,
    loading_bar_materials: &mut Assets<LoadingBarMaterial>,
    start_url: &str,
    with_active: bool,
) -> Entity {
    let mut b = commands.spawn((
        Webview,
        Pane,
        Tab,
        Visibility::Visible,
        PaneLastUrl(start_url.to_string()),
        WebviewSource::new(start_url.to_string()),
        PreloadScripts::from([
            URL_TRACK_PRELOAD.to_string(),
            TEXT_INPUT_EMACS_BINDINGS_PRELOAD.to_string(),
        ]),
        ZoomLevel(CEF_PAGE_ZOOM_LEVEL),
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE))),
        MeshMaterial3d(materials.add(WebviewExtendStandardMaterial {
            base: StandardMaterial {
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            extension: WebviewMaterial::default(),
        })),
    ));
    if with_active {
        b.insert((Active, CefKeyboardTarget));
    }
    let pane_id = b.id();
    spawn_pane_chrome(commands, meshes, materials, loading_bar_materials, pane_id);
    pane_id
}

/// New layout leaf for the Dioxus history UI: **no** [`URL_TRACK_PRELOAD`] (avoids polluting [`NavigationHistory`](vmux_core::NavigationHistory)).
///
/// When `initial_url` is **None** (or empty), uses `about:blank` until [`HistoryPaneNeedsUrl`] is
/// cleared by `vmux_history` so the pane never depends on `cef://localhost/__inline__/…` (secondary
/// panes can hit `ERR_UNKNOWN_URL_SCHEME` for that scheme).
///
/// When `initial_url` is set (embedded HTTP base from startup drain or `VMUX_HISTORY_UI_URL`), loads
/// the UI immediately — same readiness model as the status bar chrome.
pub fn spawn_history_pane(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WebviewExtendStandardMaterial>,
    loading_bar_materials: &mut Assets<LoadingBarMaterial>,
    with_active: bool,
    initial_url: Option<&str>,
) -> Entity {
    let trimmed = initial_url.map(str::trim).filter(|s| !s.is_empty());
    let needs_placeholder = trimmed.is_none();
    let start = trimmed.unwrap_or("about:blank").to_string();
    let mut b = commands.spawn((
        Webview,
        Pane,
        Tab,
        History,
        HistoryPaneOpenedAt(std::time::Instant::now()),
        Visibility::Visible,
        PaneLastUrl(start.clone()),
        WebviewSource::new(start),
        PreloadScripts::from([TEXT_INPUT_EMACS_BINDINGS_PRELOAD.to_string()]),
        ZoomLevel(CEF_PAGE_ZOOM_LEVEL),
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE))),
        MeshMaterial3d(materials.add(WebviewExtendStandardMaterial {
            base: StandardMaterial {
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            extension: WebviewMaterial::default(),
        })),
    ));
    if needs_placeholder {
        b.insert(HistoryPaneNeedsUrl);
    }
    if with_active {
        b.insert((Active, CefKeyboardTarget));
    }
    let pane_id = b.id();
    spawn_pane_chrome(commands, meshes, materials, loading_bar_materials, pane_id);
    pane_id
}

pub fn spawn_saved_recursive(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WebviewExtendStandardMaterial>,
    loading_bar_materials: &mut Assets<LoadingBarMaterial>,
    node: &SavedLayoutNode,
    first_active: &mut bool,
    default_webview_url: &str,
    history_ui_base: Option<&str>,
) -> LayoutNode {
    match node {
        SavedLayoutNode::Split {
            axis,
            ratio,
            left,
            right,
        } => LayoutNode::Split {
            axis: *axis,
            ratio: *ratio,
            left: Box::new(spawn_saved_recursive(
                commands,
                meshes,
                materials,
                loading_bar_materials,
                left,
                first_active,
                default_webview_url,
                history_ui_base,
            )),
            right: Box::new(spawn_saved_recursive(
                commands,
                meshes,
                materials,
                loading_bar_materials,
                right,
                first_active,
                default_webview_url,
                history_ui_base,
            )),
        },
        SavedLayoutNode::Leaf { url, history_pane } => {
            let active = *first_active;
            *first_active = false;
            let u = url.trim();
            const DATA_HTML: &str = "data:text/html";
            let inline_history_html = u.len() >= DATA_HTML.len()
                && u[..DATA_HTML.len()].eq_ignore_ascii_case(DATA_HTML);
            if *history_pane
                || legacy_loopback_embedded_history_ui_url(u)
                || u.eq_ignore_ascii_case("about:blank")
                || inline_history_html
            {
                return LayoutNode::leaf(spawn_history_pane(
                    commands,
                    meshes,
                    materials,
                    loading_bar_materials,
                    active,
                    history_ui_base,
                ));
            }
            let start = if !u.is_empty() && allowed_navigation_url(u) {
                sanitize_embedded_webview_url(u, default_webview_url)
            } else {
                default_webview_url.to_string()
            };
            LayoutNode::leaf(spawn_pane(
                commands,
                meshes,
                materials,
                loading_bar_materials,
                &start,
                active,
            ))
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Called from `vmux_webview` after embedded UI URLs are drained (`EmbeddedServeDirStartup::DrainChannels`).
pub fn setup_vmux_panes(
    mut commands: Commands,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    last: Option<Res<LastVisitedUrl>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut loading_bar_materials: ResMut<Assets<LoadingBarMaterial>>,
    path: Option<Res<SessionSavePath>>,
    mut session_queue: ResMut<SessionSaveQueue>,
    settings: Res<VmuxAppSettings>,
    history_ui_base: Option<&str>,
) {
    let fallback = settings.browser.default_webview_url.as_str();
    let mut migrated = false;
    if snapshot.parsed_root().is_none()
        && let Some(last) = last.as_ref()
    {
        let u = last.0.trim();
        if !u.is_empty() && allowed_navigation_url(u) {
            let migrated_url = sanitize_embedded_webview_url(u, fallback);
            snapshot.set_root(&SavedLayoutNode::leaf_url(migrated_url));
            migrated = true;
        }
    }
    let root_node = if let Some(saved) = snapshot.parsed_root_for_restore(fallback) {
        let mut first_active = true;
        spawn_saved_recursive(
            &mut commands,
            &mut meshes,
            &mut materials,
            &mut loading_bar_materials,
            &saved,
            &mut first_active,
            fallback,
            history_ui_base,
        )
    } else {
        let start_url = initial_webview_url(last.as_deref(), fallback);
        LayoutNode::leaf(spawn_pane(
            &mut commands,
            &mut meshes,
            &mut materials,
            &mut loading_bar_materials,
            &start_url,
            true,
        ))
    };

    commands.spawn(Workspace).with_children(|parent| {
        parent.spawn((
            crate::Window,
            Layout {
                root: root_node,
                revision: 0,
                zoom_pane: None,
            },
        ));
        parent.spawn(Profile);
    });

    if migrated && let Some(p) = path.as_ref() {
        session_queue.0.push(p.0.clone());
    }
}
