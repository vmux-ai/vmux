//! Hierarchical pane layout host, spawn, solver application, and split/focus controls.

use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::Browsers;
use moonshine_save::prelude::*;

use vmux_core::{
    CAMERA_DISTANCE, LastVisitedUrl, SessionSavePath, VmuxWorldCamera, allowed_navigation_url,
    initial_webview_url,
};
use vmux_input::{AppInputRoot, VmuxPrefixState};
use vmux_layout::{
    Active, LayoutAxis, LayoutNode, LayoutTree, Pane, PaneLastUrl, PixelRect, Root,
    SavedLayoutNode, SessionLayoutSnapshot, layout_node_to_saved, solve_layout,
};

use crate::system::URL_TRACK_PRELOAD;
use crate::{CEF_PAGE_ZOOM_LEVEL, VmuxWebview, WEBVIEW_URL};

/// World-space Z separation between pane planes (camera at +Z; larger Z is closer to the camera).
const PANE_Z_STRIDE: f32 = 0.05;

/// Per-pane [`StandardMaterial::depth_bias`] step so stacked panes win the depth test (see `apply_pane_layout`).
const PANE_DEPTH_BIAS_STRIDE: f32 = 250.0;

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
        PreloadScripts::from([URL_TRACK_PRELOAD.to_string()]),
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
        b.insert(Active);
    }
    b.id()
}

fn spawn_saved_recursive(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WebviewExtendStandardMaterial>,
    node: &SavedLayoutNode,
    first_active: &mut bool,
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
            )),
            right: Box::new(spawn_saved_recursive(
                commands,
                meshes,
                materials,
                right,
                first_active,
            )),
        },
        SavedLayoutNode::Leaf { url } => {
            let u = url.trim();
            let start = if !u.is_empty() && allowed_navigation_url(u) {
                u.to_string()
            } else {
                WEBVIEW_URL.to_string()
            };
            let active = *first_active;
            *first_active = false;
            LayoutNode::leaf(spawn_pane(commands, meshes, materials, &start, active))
        }
    }
}

pub(crate) fn setup_vmux_panes(
    mut commands: Commands,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    last: Option<Res<LastVisitedUrl>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    path: Option<Res<SessionSavePath>>,
) {
    let mut migrated = false;
    if snapshot.parsed_root().is_none()
        && let Some(last) = last.as_ref()
    {
        let u = last.0.trim();
        if !u.is_empty() && allowed_navigation_url(u) {
            snapshot.set_root(&SavedLayoutNode::leaf_url(last.0.clone()));
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
        )
    } else {
        let start_url = initial_webview_url(last.as_deref(), WEBVIEW_URL);
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
        commands.trigger_save(
            SaveWorld::default_into_file(p.0.clone()).include_resource::<SessionLayoutSnapshot>(),
        );
    }
}

fn webview_source_url(src: &WebviewSource) -> String {
    match src {
        WebviewSource::Url(s) | WebviewSource::InlineHtml(s) => s.clone(),
    }
}

/// Rebuild [`SessionLayoutSnapshot`] from the current layout tree and pane URLs.
pub fn rebuild_session_snapshot(
    tree: &LayoutTree,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
) -> SessionLayoutSnapshot {
    let root = layout_node_to_saved(&tree.root, |e| {
        if let Ok(p) = pane_last.get(e) {
            let u = p.0.trim();
            if !u.is_empty() && allowed_navigation_url(u) {
                return p.0.clone();
            }
        }
        webview_src
            .get(e)
            .map(webview_source_url)
            .unwrap_or_else(|_| WEBVIEW_URL.to_string())
    });
    let mut snap = SessionLayoutSnapshot::default();
    snap.set_root(&root);
    snap
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_pane_layout(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &Projection), (With<Camera3d>, With<VmuxWorldCamera>)>,
    layout_q: Query<&LayoutTree, With<Root>>,
    panes: Query<Entity, With<Pane>>,
    mut transforms: Query<&mut Transform, With<Pane>>,
    mut sizes: Query<&mut WebviewSize, With<Pane>>,
    mesh_mat: Query<&MeshMaterial3d<WebviewExtendStandardMaterial>, With<Pane>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let Ok((camera, projection)) = camera.single() else {
        return;
    };
    let Ok(layout) = layout_q.single() else {
        return;
    };

    let Projection::Perspective(perspective) = projection else {
        return;
    };

    let vw = window.width();
    let vh = window.height();
    if !(vw.is_finite() && vh.is_finite()) || vw <= 0.0 || vh <= 0.0 {
        return;
    }

    let aspect = camera
        .logical_viewport_size()
        .filter(|s| s.x > 0.0 && s.y > 0.0 && s.x.is_finite() && s.y.is_finite())
        .map(|s| s.x / s.y)
        .unwrap_or(vw / vh);

    let tan_half_fov = (perspective.fov * 0.5).tan();
    let half_h = CAMERA_DISTANCE * tan_half_fov;
    let half_w = half_h * aspect;

    let entity_alive = |e: Entity| panes.contains(e);
    let area = PixelRect {
        x: 0.0,
        y: 0.0,
        w: vw,
        h: vh,
    };
    let mut rects = solve_layout(&layout.root, area, entity_alive);
    // Depth order: panes higher on the screen (smaller layout Y) must sit *closer* to the camera
    // (larger world Z) so stacked neighbors cannot win the depth test and erase them with clear
    // color. DFS order used to put the bottom stack child last → highest Z → drawn on top.
    rects.sort_by(|a, b| {
        let cy_a = a.1.y + a.1.h * 0.5;
        let cy_b = b.1.y + b.1.h * 0.5;
        cy_b.partial_cmp(&cy_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let cx_a = a.1.x + a.1.w * 0.5;
                let cx_b = b.1.x + b.1.w * 0.5;
                cx_b.partial_cmp(&cx_a).unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut z_eps = 0.0_f32;
    for (i, (entity, pr)) in rects.into_iter().enumerate() {
        let Ok(mut tf) = transforms.get_mut(entity) else {
            continue;
        };
        let Ok(mut ws) = sizes.get_mut(entity) else {
            continue;
        };

        let cx = pr.x + pr.w * 0.5;
        let cy = pr.y + pr.h * 0.5;
        let nx = cx / vw;
        let ny = cy / vh;

        let wx = (nx - 0.5) * 2.0 * half_w;
        let wy = (0.5 - ny) * 2.0 * half_h;

        let scale_x = (pr.w / vw) * half_w;
        let scale_y = (pr.h / vh) * half_h;

        tf.translation = Vec3::new(wx, wy, z_eps);
        tf.scale = Vec3::new(scale_x.max(1.0e-4), scale_y.max(1.0e-4), 1.0);
        ws.0 = Vec2::new(pr.w.max(1.0), pr.h.max(1.0));
        z_eps += PANE_Z_STRIDE;

        if let Ok(handle) = mesh_mat.get(entity)
            && let Some(mat) = materials.get_mut(handle.id())
        {
            // Higher index = higher on screen (after sort) → must win the depth test over panes below.
            mat.base.depth_bias = i as f32 * PANE_DEPTH_BIAS_STRIDE;
        }
    }
}

/// Push [`WebviewSize`] to CEF in `PostUpdate` right after layout. The stock `resize` system runs in
/// `Update` on `Changed<WebviewSize>`, so it can lag one frame behind `apply_pane_layout` and leave a
/// browser painting into the wrong backing size (gray bands on stacked panes).
pub(crate) fn sync_cef_sizes_after_pane_layout(
    browsers: NonSend<Browsers>,
    mut last: Local<HashMap<Entity, (Vec2, f32)>>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    host_window: Query<&HostWindow>,
    panes: Query<(Entity, &WebviewSize), With<Pane>>,
) {
    let Ok(primary_e) = primary_window.single() else {
        return;
    };
    let default_scale = windows
        .get(primary_e)
        .ok()
        .map(|w| w.resolution.scale_factor())
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(1.0);

    let alive: HashSet<Entity> = panes.iter().map(|(e, _)| e).collect();
    last.retain(|e, _| alive.contains(e));

    for (entity, size) in &panes {
        if !browsers.has_browser(entity) {
            continue;
        }
        let scale = host_window
            .get(entity)
            .ok()
            .and_then(|h| windows.get(h.0).ok())
            .map(|w| w.resolution.scale_factor())
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(default_scale);

        let sz = size.0;
        let unchanged = last
            .get(&entity)
            .is_some_and(|(pz, ps)| *pz == sz && (*ps - scale).abs() < 1.0e-4);
        if unchanged {
            continue;
        }
        last.insert(entity, (sz, scale));
        browsers.resize(&entity, sz, scale);
    }
}

fn ctrl_shift(keys: &ButtonInput<KeyCode>) -> bool {
    (keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight))
        && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight))
}

fn tmux_prefix_armed(prefix: &Query<&VmuxPrefixState, With<AppInputRoot>>) -> bool {
    prefix.single().map(|p| p.awaiting).unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn try_split_active_pane(
    commands: &mut Commands,
    layout_tree: &mut LayoutTree,
    active_ent: Entity,
    axis: LayoutAxis,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    path: Option<&Res<SessionSavePath>>,
) {
    let new_pane = spawn_pane(commands, meshes, materials, WEBVIEW_URL, false);
    if layout_tree.split_leaf(active_ent, new_pane, axis) {
        commands.entity(new_pane).insert(Active);
        commands.entity(active_ent).remove::<Active>();
        *snapshot = rebuild_session_snapshot(layout_tree, pane_last, webview_src);
        if let Some(p) = path {
            commands.trigger_save(
                SaveWorld::default_into_file(p.0.clone())
                    .include_resource::<SessionLayoutSnapshot>(),
            );
        }
    }
}

pub(crate) fn try_cycle_pane_focus(commands: &mut Commands, tree: &LayoutTree, cur: Entity) {
    let mut leaves = Vec::new();
    tree.root.collect_leaves(&mut leaves);
    if leaves.len() < 2 {
        return;
    }
    let pos = leaves.iter().position(|&e| e == cur).unwrap_or(0);
    let next = leaves[(pos + 1) % leaves.len()];
    if next != cur {
        commands.entity(cur).remove::<Active>();
        commands.entity(next).insert(Active);
    }
}

/// Tmux **kill-pane** (`kill-pane`): close the active pane when at least one other remains.
pub(crate) fn try_kill_active_pane(
    commands: &mut Commands,
    layout_tree: &mut LayoutTree,
    active_ent: Entity,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    path: Option<&Res<SessionSavePath>>,
) -> bool {
    let mut leaves = Vec::new();
    layout_tree.root.collect_leaves(&mut leaves);
    if leaves.len() <= 1 {
        return false;
    }
    if !layout_tree.remove_leaf(active_ent) {
        return false;
    }
    let mut new_leaves = Vec::new();
    layout_tree.root.collect_leaves(&mut new_leaves);
    let Some(&survivor) = new_leaves.first() else {
        return false;
    };
    for &e in &new_leaves {
        commands.entity(e).remove::<Active>();
    }
    commands.entity(survivor).insert(Active);
    commands.entity(active_ent).despawn();
    *snapshot = rebuild_session_snapshot(layout_tree, pane_last, webview_src);
    if let Some(p) = path {
        commands.trigger_save(
            SaveWorld::default_into_file(p.0.clone()).include_resource::<SessionLayoutSnapshot>(),
        );
    }
    true
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn split_active_pane(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    prefix: Query<&VmuxPrefixState, With<AppInputRoot>>,
    mut layout_q: Query<&mut LayoutTree, With<Root>>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    pane_last: Query<&PaneLastUrl>,
    webview_src: Query<&WebviewSource>,
    path: Option<Res<SessionSavePath>>,
) {
    if tmux_prefix_armed(&prefix) {
        return;
    }
    if !ctrl_shift(&keys) {
        return;
    }
    let axis = if keys.just_pressed(KeyCode::KeyV) {
        LayoutAxis::Horizontal
    } else if keys.just_pressed(KeyCode::KeyH) {
        LayoutAxis::Vertical
    } else {
        return;
    };

    let Ok(mut tree) = layout_q.single_mut() else {
        return;
    };
    let Ok(active_ent) = active.single() else {
        return;
    };

    try_split_active_pane(
        &mut commands,
        &mut tree,
        active_ent,
        axis,
        &mut meshes,
        &mut materials,
        &mut snapshot,
        &pane_last,
        &webview_src,
        path.as_ref(),
    );
}

pub(crate) fn cycle_pane_focus(
    keys: Res<ButtonInput<KeyCode>>,
    prefix: Query<&VmuxPrefixState, With<AppInputRoot>>,
    layout_q: Query<&LayoutTree, With<Root>>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    mut commands: Commands,
) {
    if tmux_prefix_armed(&prefix) {
        return;
    }
    if !ctrl_shift(&keys) || !keys.just_pressed(KeyCode::Tab) {
        return;
    }
    let Ok(tree) = layout_q.single() else {
        return;
    };
    let Ok(cur) = active.single() else {
        return;
    };
    try_cycle_pane_focus(&mut commands, tree, cur);
}
