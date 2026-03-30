//! CEF pane spawn (mesh + webview) for layout leaves.

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::{SessionSavePath, SessionSaveQueue};
use vmux_settings::VmuxAppSettings;

use crate::{
    Active, LastVisitedUrl, LayoutNode, LayoutTree, Pane, PaneChromeNeedsUrl, PaneChromeOwner,
    PaneChromeStrip, PaneLastUrl, Root, SavedLayoutNode, SessionLayoutSnapshot,
    allowed_navigation_url, initial_webview_url, sanitize_embedded_webview_url,
};

/// Marker for the primary vmux webview entity.
#[derive(Component)]
pub struct VmuxWebview;

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

/// Keep [`Active`] and [`CefKeyboardTarget`] aligned with the pane under the pointer **before** CEF
/// receives keys. `Pointer<Move>` alone misses click-to-focus without a move; `Pointer<Press>` covers that.
fn apply_active_pane_for_pointer(
    ent: Entity,
    commands: &mut Commands,
    active: &Query<Entity, (With<Pane>, With<Active>)>,
) {
    if active.contains(ent) {
        commands.entity(ent).insert(CefKeyboardTarget);
        return;
    }
    for e in active.iter() {
        commands.entity(e).remove::<Active>();
        commands.entity(e).remove::<CefKeyboardTarget>();
    }
    commands.entity(ent).insert((Active, CefKeyboardTarget));
}

fn activate_pane_on_pointer_move(
    trigger: On<Pointer<Move>>,
    mut commands: Commands,
    active: Query<Entity, (With<Pane>, With<Active>)>,
) {
    apply_active_pane_for_pointer(trigger.entity, &mut commands, &active);
}

fn activate_pane_on_pointer_press(
    trigger: On<Pointer<Press>>,
    mut commands: Commands,
    active: Query<Entity, (With<Pane>, With<Active>)>,
) {
    apply_active_pane_for_pointer(trigger.entity, &mut commands, &active);
}

fn activate_owner_pane_on_pointer_move(
    trigger: On<Pointer<Move>>,
    mut commands: Commands,
    owner: Query<&PaneChromeOwner>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
) {
    let Ok(o) = owner.get(trigger.entity) else {
        return;
    };
    apply_active_pane_for_pointer(o.0, &mut commands, &active);
}

fn activate_owner_pane_on_pointer_press(
    trigger: On<Pointer<Press>>,
    mut commands: Commands,
    owner: Query<&PaneChromeOwner>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
) {
    let Ok(o) = owner.get(trigger.entity) else {
        return;
    };
    apply_active_pane_for_pointer(o.0, &mut commands, &active);
}

fn spawn_pane_chrome(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WebviewExtendStandardMaterial>,
    pane: Entity,
) {
    commands
        .spawn((
            PaneChromeStrip,
            PaneChromeOwner(pane),
            PaneChromeNeedsUrl,
            WebviewSource::inline(CHROME_LOADING_HTML),
            PreloadScripts::default(),
            ZoomLevel(CEF_PAGE_ZOOM_LEVEL),
            Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE))),
            MeshMaterial3d(materials.add(WebviewExtendStandardMaterial {
                base: StandardMaterial {
                    unlit: true,
                    base_color: Color::WHITE,
                    depth_bias: 1_000_000.0,
                    ..default()
                },
                extension: WebviewMaterial::default(),
            })),
        ))
        .observe(activate_owner_pane_on_pointer_move)
        .observe(activate_owner_pane_on_pointer_press);
}

pub fn spawn_pane(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WebviewExtendStandardMaterial>,
    start_url: &str,
    with_active: bool,
) -> Entity {
    let mut b = commands.spawn((
        VmuxWebview,
        Pane,
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
                ..default()
            },
            extension: WebviewMaterial::default(),
        })),
    ));
    if with_active {
        b.insert((Active, CefKeyboardTarget));
    }
    b.observe(activate_pane_on_pointer_move)
        .observe(activate_pane_on_pointer_press);
    let pane_id = b.id();
    spawn_pane_chrome(commands, meshes, materials, pane_id);
    pane_id
}

pub fn spawn_saved_recursive(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WebviewExtendStandardMaterial>,
    node: &SavedLayoutNode,
    first_active: &mut bool,
    default_webview_url: &str,
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
                left,
                first_active,
                default_webview_url,
            )),
            right: Box::new(spawn_saved_recursive(
                commands,
                meshes,
                materials,
                right,
                first_active,
                default_webview_url,
            )),
        },
        SavedLayoutNode::Leaf { url } => {
            let u = url.trim();
            let start = if !u.is_empty() && allowed_navigation_url(u) {
                sanitize_embedded_webview_url(u, default_webview_url)
            } else {
                default_webview_url.to_string()
            };
            let active = *first_active;
            *first_active = false;
            LayoutNode::leaf(spawn_pane(commands, meshes, materials, &start, active))
        }
    }
}

pub fn setup_vmux_panes(
    mut commands: Commands,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    last: Option<Res<LastVisitedUrl>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    path: Option<Res<SessionSavePath>>,
    mut session_queue: ResMut<SessionSaveQueue>,
    settings: Res<VmuxAppSettings>,
) {
    let fallback = settings.default_webview_url.as_str();
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
    let root_node = if let Some(saved) = snapshot.parsed_root() {
        let mut first_active = true;
        spawn_saved_recursive(
            &mut commands,
            &mut meshes,
            &mut materials,
            &saved,
            &mut first_active,
            fallback,
        )
    } else {
        let start_url = initial_webview_url(last.as_deref(), fallback);
        LayoutNode::leaf(spawn_pane(
            &mut commands,
            &mut meshes,
            &mut materials,
            &start_url,
            true,
        ))
    };

    commands.spawn((
        Root,
        LayoutTree {
            root: root_node,
            revision: 0,
        },
    ));

    if migrated && let Some(p) = path.as_ref() {
        session_queue.0.push(p.0.clone());
    }
}
