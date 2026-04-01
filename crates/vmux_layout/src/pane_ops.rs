//! Ctrl+Shift split / Tab focus, tmux-style try_* helpers, and session snapshot rebuild.

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::input_root::{AppInputRoot, VmuxPrefixState};
use vmux_core::{SessionSavePath, SessionSaveQueue};

use crate::pane_spawn::{spawn_history_pane, spawn_pane};
use crate::url::{allowed_navigation_url, sanitize_embedded_webview_url};
use crate::{
    Active, History, LayoutAxis, LayoutNode, Layout, LoadingBarMaterial, Pane, Webview,
    PaneChromeLoadingBar, PaneChromeOwner, PaneChromeStrip, PaneLastUrl, PaneSwapDir, PixelRect,
    SavedSessionLeaf, SessionLayoutSnapshot, layout_node_to_saved,
    neighbor_pane_in_direction,
};
use vmux_settings::VmuxAppSettings;

fn webview_source_url(src: &WebviewSource) -> String {
    match src {
        WebviewSource::Url(s) | WebviewSource::InlineHtml(s) => s.clone(),
    }
}

/// Rebuild [`SessionLayoutSnapshot`] from the current layout tree and pane URLs.
pub fn rebuild_session_snapshot(
    tree: &Layout,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    history_panes: &Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    fallback_webview_url: &str,
) -> SessionLayoutSnapshot {
    let root = layout_node_to_saved(&tree.root, |e| {
        let history_pane = history_panes.contains(e);
        if history_pane {
            return SavedSessionLeaf {
                url: String::new(),
                history_pane: true,
            };
        }
        let url = if let Ok(p) = pane_last.get(e) {
            let u = p.0.trim();
            if !u.is_empty() && allowed_navigation_url(u) {
                sanitize_embedded_webview_url(&p.0, fallback_webview_url)
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        let url = if !url.is_empty() {
            url
        } else {
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
        };
        SavedSessionLeaf {
            url,
            history_pane,
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

#[allow(clippy::type_complexity)]
pub fn despawn_chrome_for_pane(
    commands: &mut Commands,
    chrome_or_border: &Query<
        (Entity, &PaneChromeOwner),
        Or<(With<PaneChromeStrip>, With<PaneChromeLoadingBar>)>,
    >,
    pane: Entity,
) {
    for (e, owner) in chrome_or_border.iter() {
        if owner.0 == pane {
            commands.entity(e).despawn();
        }
    }
}

fn clear_zoom_pane(layout_tree: &mut Layout) {
    if layout_tree.zoom_pane.take().is_some() {
        layout_tree.bump();
    }
}

/// Tmux **`resize-pane -Z`**: toggle zoom so the active pane fills the window; not persisted.
pub fn try_toggle_zoom_pane(layout_tree: &mut Layout, active_ent: Entity) -> bool {
    let mut leaves = Vec::new();
    layout_tree.root.collect_leaves(&mut leaves);
    if leaves.len() < 2 {
        if layout_tree.zoom_pane.take().is_some() {
            layout_tree.bump();
        }
        return false;
    }
    if !layout_tree.root.contains_leaf(active_ent) {
        return false;
    }
    let next = match layout_tree.zoom_pane {
        Some(z) if z == active_ent => None,
        _ => Some(active_ent),
    };
    layout_tree.zoom_pane = next;
    layout_tree.bump();
    true
}

#[allow(clippy::too_many_arguments)]
pub fn try_split_active_pane(
    commands: &mut Commands,
    layout_tree: &mut Layout,
    active_ent: Entity,
    axis: LayoutAxis,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    loading_bar_materials: &mut ResMut<Assets<LoadingBarMaterial>>,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    history_panes: &Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    path: Option<&Res<SessionSavePath>>,
    session_queue: &mut SessionSaveQueue,
    default_webview_url: &str,
) {
    clear_zoom_pane(layout_tree);
    let new_pane = spawn_pane(
        commands,
        meshes,
        materials,
        loading_bar_materials,
        default_webview_url,
        false,
    );
    if layout_tree.split_leaf(active_ent, new_pane, axis) {
        commands.entity(new_pane).insert(Active);
        commands.entity(active_ent).remove::<Active>();
        *snapshot = rebuild_session_snapshot(
            layout_tree,
            pane_last,
            webview_src,
            history_panes,
            default_webview_url,
        );
        if let Some(p) = path {
            session_queue.0.push(p.0.clone());
        }
    }
}

/// Split the active leaf 50/50 and put a **history UI** pane in the new slot (see [`crate::spawn_history_pane`]).
#[allow(clippy::too_many_arguments)]
pub fn try_split_active_history_pane(
    commands: &mut Commands,
    layout_tree: &mut Layout,
    active_ent: Entity,
    axis: LayoutAxis,
    chrome_or_border: &Query<
        (Entity, &PaneChromeOwner),
        Or<(With<PaneChromeStrip>, With<PaneChromeLoadingBar>)>,
    >,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    loading_bar_materials: &mut ResMut<Assets<LoadingBarMaterial>>,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    history_panes: &Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    path: Option<&Res<SessionSavePath>>,
    session_queue: &mut SessionSaveQueue,
    default_webview_url: &str,
    history_ui_url: Option<&str>,
) -> bool {
    clear_zoom_pane(layout_tree);
    let new_pane = spawn_history_pane(
        commands,
        meshes,
        materials,
        loading_bar_materials,
        false,
        history_ui_url,
    );
    if layout_tree.split_leaf(active_ent, new_pane, axis) {
        commands.entity(new_pane).insert(Active);
        commands.entity(active_ent).remove::<Active>();
        *snapshot = rebuild_session_snapshot(
            layout_tree,
            pane_last,
            webview_src,
            history_panes,
            default_webview_url,
        );
        if let Some(p) = path {
            session_queue.0.push(p.0.clone());
        }
        true
    } else {
        despawn_chrome_for_pane(commands, chrome_or_border, new_pane);
        commands.entity(new_pane).despawn();
        false
    }
}

/// Same as [`try_split_active_history_pane`], but reuses an already spawned history pane entity
/// (e.g. a startup standby browser) instead of creating a new CEF instance.
#[allow(clippy::too_many_arguments)]
pub fn try_split_active_history_existing_pane(
    commands: &mut Commands,
    layout_tree: &mut Layout,
    active_ent: Entity,
    axis: LayoutAxis,
    history_pane: Entity,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    history_panes: &Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    path: Option<&Res<SessionSavePath>>,
    session_queue: &mut SessionSaveQueue,
    default_webview_url: &str,
) -> bool {
    clear_zoom_pane(layout_tree);
    if !history_panes.contains(history_pane) || layout_tree.root.contains_leaf(history_pane) {
        return false;
    }
    if layout_tree.split_leaf(active_ent, history_pane, axis) {
        commands.entity(history_pane).insert(Active);
        commands.entity(active_ent).remove::<Active>();
        *snapshot = rebuild_session_snapshot(
            layout_tree,
            pane_last,
            webview_src,
            history_panes,
            default_webview_url,
        );
        if let Some(p) = path {
            session_queue.0.push(p.0.clone());
        }
        true
    } else {
        false
    }
}

/// Mirror the two halves of the innermost split that contains the active pane (see [`Layout::mirror_deepest_split_around`]).
pub fn try_mirror_pane_layout(
    layout_tree: &mut Layout,
    active_ent: Entity,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    history_panes: &Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    path: Option<&Res<SessionSavePath>>,
    session_queue: &mut SessionSaveQueue,
    default_webview_url: &str,
) -> bool {
    clear_zoom_pane(layout_tree);
    if !layout_tree.mirror_deepest_split_around(active_ent) {
        return false;
    }
    *snapshot = rebuild_session_snapshot(
        layout_tree,
        pane_last,
        webview_src,
        history_panes,
        default_webview_url,
    );
    if let Some(p) = path {
        session_queue.0.push(p.0.clone());
    }
    true
}

/// Tmux **rotate-window** (`-D` / `-U`): cycle pane entities through layout slots in DFS order; moves
/// focus like tmux (`PREV` of active for `-D`, `NEXT` for `-U` in the pane list before rotation).
pub fn try_rotate_window(
    commands: &mut Commands,
    layout_tree: &mut Layout,
    active_ent: Entity,
    down: bool,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    history_panes: &Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    path: Option<&Res<SessionSavePath>>,
    session_queue: &mut SessionSaveQueue,
    default_webview_url: &str,
) -> bool {
    clear_zoom_pane(layout_tree);
    let mut leaves = Vec::new();
    layout_tree.root.collect_leaves(&mut leaves);
    if leaves.len() < 2 {
        return false;
    }
    let Some(i) = leaves.iter().position(|&e| e == active_ent) else {
        return false;
    };
    let n = leaves.len();
    let new_active = if down {
        leaves[(i + n - 1) % n]
    } else {
        leaves[(i + 1) % n]
    };

    let ok = if down {
        layout_tree.rotate_window_down()
    } else {
        layout_tree.rotate_window_up()
    };
    if !ok {
        return false;
    }

    if new_active != active_ent {
        commands.entity(active_ent).remove::<Active>();
        commands.entity(new_active).insert(Active);
    }
    *snapshot = rebuild_session_snapshot(
        layout_tree,
        pane_last,
        webview_src,
        history_panes,
        default_webview_url,
    );
    if let Some(p) = path {
        session_queue.0.push(p.0.clone());
    }
    true
}

/// Tmux **[select-pane](https://man.openbsd.org/tmux.1#select-pane)** (`-L` / `-R` / `-U` / `-D`): move focus to the adjacent pane in that direction (layout unchanged).
///
/// `prefer_if_valid`: see [`neighbor_pane_in_direction`](crate::neighbor_pane_in_direction).
pub fn try_select_pane_direction(
    commands: &mut Commands,
    layout_tree: &mut Layout,
    active_ent: Entity,
    dir: PaneSwapDir,
    rects: &[(Entity, PixelRect)],
    prefer_if_valid: Option<Entity>,
) -> bool {
    clear_zoom_pane(layout_tree);
    let Some(next) = neighbor_pane_in_direction(rects, active_ent, dir, prefer_if_valid) else {
        return false;
    };
    if next == active_ent {
        return false;
    }
    commands.entity(active_ent).remove::<Active>();
    commands.entity(next).insert(Active);
    true
}

/// Tmux **[swap-pane](https://man.openbsd.org/tmux.1#swap-pane)** (`-L` / `-R` / `-U` / `-D`): exchange the active pane with its neighbor in that direction.
pub fn try_swap_active_pane(
    layout_tree: &mut Layout,
    active_ent: Entity,
    dir: PaneSwapDir,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    history_panes: &Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    path: Option<&Res<SessionSavePath>>,
    session_queue: &mut SessionSaveQueue,
    default_webview_url: &str,
) -> bool {
    clear_zoom_pane(layout_tree);
    if !layout_tree.swap_active_pane(active_ent, dir) {
        return false;
    }
    *snapshot = rebuild_session_snapshot(
        layout_tree,
        pane_last,
        webview_src,
        history_panes,
        default_webview_url,
    );
    if let Some(p) = path {
        session_queue.0.push(p.0.clone());
    }
    true
}

pub fn try_cycle_pane_focus(commands: &mut Commands, layout_tree: &mut Layout, cur: Entity) {
    clear_zoom_pane(layout_tree);
    let mut leaves = Vec::new();
    layout_tree.root.collect_leaves(&mut leaves);
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
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub fn try_kill_active_pane(
    commands: &mut Commands,
    layout_tree: &mut Layout,
    active_ent: Entity,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    loading_bar_materials: &mut ResMut<Assets<LoadingBarMaterial>>,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    history_panes: &Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    chrome_or_border: &Query<
        (Entity, &PaneChromeOwner),
        Or<(With<PaneChromeStrip>, With<PaneChromeLoadingBar>)>,
    >,
    path: Option<&Res<SessionSavePath>>,
    session_queue: &mut SessionSaveQueue,
    default_webview_url: &str,
) -> bool {
    clear_zoom_pane(layout_tree);
    let mut leaves = Vec::new();
    layout_tree.root.collect_leaves(&mut leaves);
    if leaves.len() == 1 {
        if leaves[0] != active_ent {
            return false;
        }
        let new_pane = spawn_pane(
            commands,
            meshes,
            materials,
            loading_bar_materials,
            default_webview_url,
            false,
        );
        layout_tree.root = LayoutNode::Leaf(new_pane);
        layout_tree.bump();
        commands.entity(active_ent).remove::<Active>();
        despawn_chrome_for_pane(commands, chrome_or_border, active_ent);
        commands.entity(active_ent).despawn();
        commands.entity(new_pane).insert(Active);
        *snapshot = rebuild_session_snapshot(
            layout_tree,
            pane_last,
            webview_src,
            history_panes,
            default_webview_url,
        );
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
    despawn_chrome_for_pane(commands, chrome_or_border, active_ent);
    commands.entity(active_ent).despawn();
    *snapshot = rebuild_session_snapshot(
        layout_tree,
        pane_last,
        webview_src,
        history_panes,
        default_webview_url,
    );
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
    mut layout_q: Query<&mut Layout, With<crate::Window>>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut loading_bar_materials: ResMut<Assets<LoadingBarMaterial>>,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    pane_last: Query<&PaneLastUrl>,
    webview_src: Query<&WebviewSource>,
    history_panes: Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
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

    let url = settings.browser.default_webview_url.as_str();
    try_split_active_pane(
        &mut commands,
        &mut tree,
        active_ent,
        axis,
        &mut meshes,
        &mut materials,
        &mut loading_bar_materials,
        &mut snapshot,
        &pane_last,
        &webview_src,
        &history_panes,
        path.as_ref(),
        &mut session_queue,
        url,
    );
}

pub fn cycle_pane_focus(
    keys: Res<ButtonInput<KeyCode>>,
    prefix: Query<&VmuxPrefixState, With<AppInputRoot>>,
    mut layout_q: Query<&mut Layout, With<crate::Window>>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    mut commands: Commands,
) {
    if tmux_prefix_armed(&prefix) {
        return;
    }
    if !ctrl_shift(&keys) || !keys.just_pressed(KeyCode::Tab) {
        return;
    }
    let Ok(mut tree) = layout_q.single_mut() else {
        return;
    };
    let Ok(cur) = active.single() else {
        return;
    };
    try_cycle_pane_focus(&mut commands, &mut tree, cur);
}
