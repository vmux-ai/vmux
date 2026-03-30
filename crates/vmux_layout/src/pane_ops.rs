//! Ctrl+Shift split / Tab focus, tmux-style try_* helpers, and session snapshot rebuild.

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::input_root::{AppInputRoot, VmuxPrefixState};
use vmux_core::{SessionSavePath, SessionSaveQueue};

use crate::pane_spawn::spawn_pane;
use crate::url::{allowed_navigation_url, sanitize_embedded_webview_url};
use crate::{
    Active, LayoutAxis, LayoutNode, LayoutTree, Pane, PaneLastUrl, Root, SessionLayoutSnapshot,
    layout_node_to_saved,
};
use vmux_settings::VmuxAppSettings;

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
    fallback_webview_url: &str,
) -> SessionLayoutSnapshot {
    let root = layout_node_to_saved(&tree.root, |e| {
        if let Ok(p) = pane_last.get(e) {
            let u = p.0.trim();
            if !u.is_empty() && allowed_navigation_url(u) {
                return sanitize_embedded_webview_url(&p.0, fallback_webview_url);
            }
        }
        let raw = webview_src
            .get(e)
            .map(webview_source_url)
            .unwrap_or_else(|_| fallback_webview_url.to_string());
        let u = raw.trim();
        if !u.is_empty() && allowed_navigation_url(u) {
            sanitize_embedded_webview_url(&raw, fallback_webview_url)
        } else {
            fallback_webview_url.to_string()
        }
    });
    let mut snap = SessionLayoutSnapshot::default();
    snap.set_root(&root);
    snap
}

fn ctrl_shift(keys: &ButtonInput<KeyCode>) -> bool {
    (keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight))
        && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight))
}

fn tmux_prefix_armed(prefix: &Query<&VmuxPrefixState, With<AppInputRoot>>) -> bool {
    prefix.single().map(|p| p.awaiting).unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
pub fn try_split_active_pane(
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
    session_queue: &mut SessionSaveQueue,
    default_webview_url: &str,
) {
    let new_pane = spawn_pane(commands, meshes, materials, default_webview_url, false);
    if layout_tree.split_leaf(active_ent, new_pane, axis) {
        commands.entity(new_pane).insert(Active);
        commands.entity(active_ent).remove::<Active>();
        *snapshot =
            rebuild_session_snapshot(layout_tree, pane_last, webview_src, default_webview_url);
        if let Some(p) = path {
            session_queue.0.push(p.0.clone());
        }
    }
}

pub fn try_cycle_pane_focus(commands: &mut Commands, tree: &LayoutTree, cur: Entity) {
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

/// Tmux **kill-pane** (`kill-pane`): close the active pane. If it is the last pane, replace it with a
/// fresh pane at [`default_webview_url`].
pub fn try_kill_active_pane(
    commands: &mut Commands,
    layout_tree: &mut LayoutTree,
    active_ent: Entity,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    path: Option<&Res<SessionSavePath>>,
    session_queue: &mut SessionSaveQueue,
    default_webview_url: &str,
) -> bool {
    let mut leaves = Vec::new();
    layout_tree.root.collect_leaves(&mut leaves);
    if leaves.len() == 1 {
        if leaves[0] != active_ent {
            return false;
        }
        let new_pane = spawn_pane(commands, meshes, materials, default_webview_url, false);
        layout_tree.root = LayoutNode::Leaf(new_pane);
        layout_tree.bump();
        commands.entity(active_ent).remove::<Active>();
        commands.entity(active_ent).despawn();
        commands.entity(new_pane).insert(Active);
        *snapshot =
            rebuild_session_snapshot(layout_tree, pane_last, webview_src, default_webview_url);
        if let Some(p) = path {
            session_queue.0.push(p.0.clone());
        }
        return true;
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
    *snapshot = rebuild_session_snapshot(layout_tree, pane_last, webview_src, default_webview_url);
    if let Some(p) = path {
        session_queue.0.push(p.0.clone());
    }
    true
}

#[allow(clippy::too_many_arguments)]
pub fn split_active_pane(
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
    mut session_queue: ResMut<SessionSaveQueue>,
    settings: Res<VmuxAppSettings>,
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

    let url = settings.default_webview_url.as_str();
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
        &mut session_queue,
        url,
    );
}

pub fn cycle_pane_focus(
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
