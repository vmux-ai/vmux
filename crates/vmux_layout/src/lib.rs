//! Hierarchical pane layout tree (runtime), pixel solver, session snapshot types, and pane layout systems.
//!
//! [`LayoutPlugin`] registers reflected layout types and `PostUpdate` pane layout
//! + CEF resize sync (after Bevy’s [`camera_system`](bevy::render::camera::camera_system)).

mod hosted_web_ui;
mod loading_bar;
mod pane_layout;
mod pane_lifecycle;
mod pane_ops;
mod pane_pointer_focus;
mod pane_spawn;
pub mod tmux;
mod url;

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::render::camera::camera_system;
use bevy_cef::prelude::render_standard_materials;
use serde::{Deserialize, Serialize};
use vmux_core::VmuxPrefixChordSet;

pub use hosted_web_ui::{VmuxHostedWebPlugin, VmuxWebviewSurface};
pub use loading_bar::{
    LOADING_BAR_ANIM_TIME_SCALE, LOADING_BAR_DEPTH_BIAS_ABOVE_CHROME, LOADING_BAR_HEIGHT_PX,
    LoadingBarMaterial, PaneChromeLoadingBar, PendingNavigationLoads,
};
pub use loading_bar::color as loading_bar_color;
pub use pane_layout::{
    CHROME_BORDER_OUTSET_PX, PANE_Z_STRIDE, apply_pane_chrome_layout, apply_pane_layout,
    apply_pane_loading_bar_layout, clamp_webview_backing_size, layout_viewport_for_workspace,
    layout_workspace_pane_rects, pixel_rect_to_world_plane, split_pane_content_and_chrome,
    sync_cef_sizes_after_pane_layout,
};
pub use pane_ops::{
    cycle_pane_focus, rebuild_session_snapshot, split_active_pane, try_cycle_pane_focus,
    try_kill_active_pane, try_mirror_pane_layout, try_rotate_window, try_select_pane_direction,
    try_split_active_history_existing_pane, try_split_active_history_pane, try_split_active_pane, try_swap_active_pane,
    try_toggle_zoom_pane,
};
pub use pane_spawn::{
    CEF_PAGE_ZOOM_LEVEL, TEXT_INPUT_EMACS_BINDINGS_PRELOAD, URL_TRACK_PRELOAD, setup_vmux_panes,
    spawn_history_pane, spawn_pane, spawn_saved_recursive,
};
pub use url::{
    allowed_navigation_url, initial_webview_url, legacy_loopback_embedded_history_ui_url,
    sanitize_embedded_webview_url,
};
pub use vmux_core::Active;
pub use vmux_core::{CAMERA_DISTANCE, VmuxWorldCamera};
pub use vmux_settings::{VmuxAppSettings, default_webview_url};

/// Marks a pane entity (e.g. CEF webview).
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct Pane;

/// Bottom chrome strip webview for a pane (status UI, etc.). Drawn **over** the main pane webview
/// at the bottom of the tile; **visible only** when the owner pane has [`Active`]
/// ([`apply_pane_chrome_layout`]). Main panes use the full tile height ([`apply_pane_layout`]).
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct PaneChromeStrip;

/// Chrome entity is tied to this pane [`Entity`] (despawn chrome before the pane).
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct PaneChromeOwner(pub Entity);

/// Set until the status UI base URL is applied by `vmux_webview`.
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct PaneChromeNeedsUrl;

/// Primary CEF webview on a tile: the leaf entity has [`Pane`] + [`Tab`] + [`Webview`] (and chrome is separate).
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct Webview;

/// History pane leaf: [`Pane`] + [`Webview`] + this marker ([`VmuxWebviewSurface::HistoryPane`]).
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct History;

/// Off-layout history pane kept warm for first toggle: same entity as [`History`], plus [`Pane`], [`Webview`], and this marker.
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct HistoryPaneStandby;

/// Set until the history UI base URL is applied from the embedded server or env.
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct HistoryPaneNeedsUrl;

/// Wall time when the history pane leaf was spawned (used to log host→pane emit latency).
#[derive(Component)]
pub struct HistoryPaneOpenedAt(pub std::time::Instant);

/// Default height in **layout pixels** of the [`PaneChromeStrip`] overlay at the bottom of each pane tile.
/// Sized for the ~11px status bar (`vmux_status_bar`) with a little padding.
pub const DEFAULT_PANE_CHROME_HEIGHT_PX: f32 = 28.0;

/// Top-level workspace: holds [`Window`] (tiling + [`Layout`]) and [`Profile`] (identity / storage scope).
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct Workspace;

/// Tiling surface inside a [`Workspace`]; owns the [`Layout`] (splits reference [`Pane`] leaves).
///
/// This is **not** [`bevy::window::Window`] (the OS window); import this type as `vmux_layout::Window`
/// or `crate::Window` in layout code so it is not confused with Bevy’s window component.
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct Window;

/// Browser-style profile (ex-session): identity, prefs, future account scope. Sibling of [`Window`] under [`Workspace`].
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct Profile;

/// Navigable surface inside a [`Pane`] (v1: colocated on the same entity as [`Pane`] + [`Webview`]; may become a child entity for multi-tab).
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct Tab;

/// Deprecated alias for [`Workspace`]; prefer [`Workspace`] in new code.
pub type Root = Workspace;

/// Last known document URL for session persistence (updated from JS emit).
#[derive(Component, Default, Debug, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct PaneLastUrl(pub String);

/// Split orientation: first child is **left** (horizontal) or **top** (vertical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect, Serialize, Deserialize, Default)]
#[reflect(Default)]
pub enum LayoutAxis {
    #[default]
    Horizontal,
    Vertical,
}

/// Direction for tmux **[select-pane](https://man.openbsd.org/tmux.1#select-pane)** / **[swap-pane](https://man.openbsd.org/tmux.1#swap-pane)** (`-L` / `-R` / `-U` / `-D`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[reflect(Serialize, Deserialize)]
pub enum PaneSwapDir {
    Left,
    Right,
    Up,
    Down,
}

/// Runtime layout node: leaves reference pane [`Entity`] values.
#[derive(Debug, Clone)]
pub enum LayoutNode {
    Split {
        axis: LayoutAxis,
        /// Fraction (0..1) of parent **width** (horizontal) or **height** (vertical) for the first child.
        ratio: f32,
        left: Box<LayoutNode>,
        right: Box<LayoutNode>,
    },
    Leaf(Entity),
}

impl LayoutNode {
    pub fn leaf(entity: Entity) -> Self {
        Self::Leaf(entity)
    }

    /// DFS leaf entities.
    pub fn collect_leaves(&self, out: &mut Vec<Entity>) {
        match self {
            LayoutNode::Split { left, right, .. } => {
                left.collect_leaves(out);
                right.collect_leaves(out);
            }
            LayoutNode::Leaf(e) => out.push(*e),
        }
    }

    pub fn contains_leaf(&self, target: Entity) -> bool {
        match self {
            LayoutNode::Leaf(e) => *e == target,
            LayoutNode::Split { left, right, .. } => {
                left.contains_leaf(target) || right.contains_leaf(target)
            }
        }
    }

    /// Replace first matching leaf; returns whether replaced.
    pub fn replace_leaf(&mut self, target: Entity, replacement: LayoutNode) -> bool {
        match self {
            LayoutNode::Leaf(e) if *e == target => {
                *self = replacement;
                true
            }
            LayoutNode::Split { left, right, .. } => {
                if left.contains_leaf(target) {
                    left.replace_leaf(target, replacement)
                } else if right.contains_leaf(target) {
                    right.replace_leaf(target, replacement)
                } else {
                    false
                }
            }
            LayoutNode::Leaf(_) => false,
        }
    }
}

/// Layout tree on a [`Window`] (tiling host) entity.
#[derive(Component, Debug, Clone)]
pub struct Layout {
    pub root: LayoutNode,
    pub revision: u64,
    /// When set, [`solve_layout`] assigns the full root area to this pane only (tmux **`resize-pane -Z`**).
    /// Not persisted in session snapshots.
    pub zoom_pane: Option<Entity>,
}

impl Layout {
    pub fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Replace `active` leaf with a 50/50 split: old pane + `new_pane`.
    pub fn split_leaf(&mut self, active: Entity, new_pane: Entity, axis: LayoutAxis) -> bool {
        if !self.root.contains_leaf(active) {
            return false;
        }
        let split = LayoutNode::Split {
            axis,
            ratio: 0.5,
            left: Box::new(LayoutNode::Leaf(active)),
            right: Box::new(LayoutNode::Leaf(new_pane)),
        };
        if !self.root.replace_leaf(active, split) {
            return false;
        }
        self.bump();
        true
    }

    /// Remove `target` pane from the tree, promoting its sibling subtree (tmux **kill-pane**).
    ///
    /// Returns `false` if `target` is missing, not a leaf, or it is the last remaining pane.
    pub fn remove_leaf(&mut self, target: Entity) -> bool {
        let mut leaves = Vec::new();
        self.root.collect_leaves(&mut leaves);
        if leaves.len() <= 1 || !leaves.contains(&target) {
            return false;
        }
        if remove_leaf_in_place(&mut self.root, target) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Swap the two children of the deepest split on the path to `active` (where `active` is a
    /// direct child leaf of that split): mirror across that split’s axis (left↔right or top↔bottom).
    pub fn mirror_deepest_split_around(&mut self, active: Entity) -> bool {
        if !self.root.contains_leaf(active) {
            return false;
        }
        let mut leaves = Vec::new();
        self.root.collect_leaves(&mut leaves);
        if leaves.len() < 2 {
            return false;
        }
        if mirror_deepest_split_in_place(&mut self.root, active) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// [`rotate-window -D`](https://man.openbsd.org/tmux.1#rotate-window): move the last pane in
    /// DFS order to the front; each pane takes the next pane’s layout slot (tree shape unchanged).
    pub fn rotate_window_down(&mut self) -> bool {
        let mut leaves = Vec::new();
        self.root.collect_leaves(&mut leaves);
        if leaves.len() < 2 {
            return false;
        }
        let n = leaves.len();
        let mut rotated = Vec::with_capacity(n);
        rotated.push(leaves[n - 1]);
        rotated.extend_from_slice(&leaves[0..n - 1]);
        assign_leaves_in_dfs_order(&mut self.root, &rotated, &mut 0);
        self.bump();
        true
    }

    /// [`rotate-window -U`](https://man.openbsd.org/tmux.1#rotate-window): move the first pane in
    /// DFS order to the end (counter-rotate).
    pub fn rotate_window_up(&mut self) -> bool {
        let mut leaves = Vec::new();
        self.root.collect_leaves(&mut leaves);
        if leaves.len() < 2 {
            return false;
        }
        let n = leaves.len();
        let mut rotated = Vec::with_capacity(n);
        rotated.extend_from_slice(&leaves[1..n]);
        rotated.push(leaves[0]);
        assign_leaves_in_dfs_order(&mut self.root, &rotated, &mut 0);
        self.bump();
        true
    }

    /// Tmux **[swap-pane](https://man.openbsd.org/tmux.1#swap-pane)** (`-L` / `-R` / `-U` / `-D`): swap the active pane with the adjacent pane in that direction (deepest applicable split).
    pub fn swap_active_pane(&mut self, active: Entity, dir: PaneSwapDir) -> bool {
        if !self.root.contains_leaf(active) {
            return false;
        }
        let mut leaves = Vec::new();
        self.root.collect_leaves(&mut leaves);
        if leaves.len() < 2 {
            return false;
        }
        if !swap_pane_in_direction(&mut self.root, active, dir) {
            return false;
        }
        self.bump();
        true
    }
}

fn swap_pane_in_direction(node: &mut LayoutNode, active: Entity, dir: PaneSwapDir) -> bool {
    match node {
        LayoutNode::Leaf(_) => false,
        LayoutNode::Split {
            axis, left, right, ..
        } => {
            let left_contains = left.contains_leaf(active);
            let right_contains = right.contains_leaf(active);
            if !left_contains && !right_contains {
                return false;
            }
            let only_left = left_contains && !right_contains;
            let only_right = right_contains && !left_contains;

            match (*axis, dir) {
                (LayoutAxis::Horizontal, PaneSwapDir::Right) if only_left => {
                    if swap_pane_in_direction(left.as_mut(), active, dir) {
                        return true;
                    }
                    std::mem::swap(left, right);
                    true
                }
                (LayoutAxis::Horizontal, PaneSwapDir::Left) if only_right => {
                    if swap_pane_in_direction(right.as_mut(), active, dir) {
                        return true;
                    }
                    std::mem::swap(left, right);
                    true
                }
                (LayoutAxis::Vertical, PaneSwapDir::Down) if only_left => {
                    if swap_pane_in_direction(left.as_mut(), active, dir) {
                        return true;
                    }
                    std::mem::swap(left, right);
                    true
                }
                (LayoutAxis::Vertical, PaneSwapDir::Up) if only_right => {
                    if swap_pane_in_direction(right.as_mut(), active, dir) {
                        return true;
                    }
                    std::mem::swap(left, right);
                    true
                }
                _ => {
                    if left_contains {
                        swap_pane_in_direction(left.as_mut(), active, dir)
                    } else {
                        swap_pane_in_direction(right.as_mut(), active, dir)
                    }
                }
            }
        }
    }
}

fn assign_leaves_in_dfs_order(node: &mut LayoutNode, entities: &[Entity], idx: &mut usize) {
    match node {
        LayoutNode::Leaf(e) => {
            *e = entities[*idx];
            *idx += 1;
        }
        LayoutNode::Split { left, right, .. } => {
            assign_leaves_in_dfs_order(left, entities, idx);
            assign_leaves_in_dfs_order(right, entities, idx);
        }
    }
}

fn mirror_deepest_split_in_place(node: &mut LayoutNode, active: Entity) -> bool {
    match node {
        LayoutNode::Leaf(_) => false,
        LayoutNode::Split { left, right, .. } => {
            let left_contains = left.contains_leaf(active);
            if !left_contains && !right.contains_leaf(active) {
                return false;
            }
            if left_contains {
                match left.as_mut() {
                    LayoutNode::Leaf(e) if *e == active => {
                        std::mem::swap(left, right);
                        true
                    }
                    _ => mirror_deepest_split_in_place(left.as_mut(), active),
                }
            } else {
                match right.as_mut() {
                    LayoutNode::Leaf(e) if *e == active => {
                        std::mem::swap(left, right);
                        true
                    }
                    _ => mirror_deepest_split_in_place(right.as_mut(), active),
                }
            }
        }
    }
}

fn remove_leaf_in_place(node: &mut LayoutNode, target: Entity) -> bool {
    match node {
        LayoutNode::Leaf(_) => false,
        LayoutNode::Split { left, right, .. } => match (left.as_mut(), right.as_mut()) {
            (LayoutNode::Leaf(le), _) if *le == target => {
                let promoted =
                    std::mem::replace(&mut **right, LayoutNode::Leaf(Entity::PLACEHOLDER));
                *node = promoted;
                true
            }
            (_, LayoutNode::Leaf(re)) if *re == target => {
                let promoted =
                    std::mem::replace(&mut **left, LayoutNode::Leaf(Entity::PLACEHOLDER));
                *node = promoted;
                true
            }
            (l, _) if l.contains_leaf(target) => remove_leaf_in_place(l, target),
            (_, r) if r.contains_leaf(target) => remove_leaf_in_place(r, target),
            _ => false,
        },
    }
}

/// Serializable layout for session file (no [`Entity`] ids). Stored as RON inside [`SessionLayoutSnapshot`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SavedLayoutNode {
    Split {
        axis: LayoutAxis,
        ratio: f32,
        left: Box<SavedLayoutNode>,
        right: Box<SavedLayoutNode>,
    },
    Leaf {
        url: String,
        /// When true, restore with [`spawn_history_pane`](crate::spawn_history_pane) so the embedded
        /// UI picks up a fresh loopback base URL after restart.
        #[serde(default)]
        history_pane: bool,
    },
}

/// Per-leaf metadata when building [`SavedLayoutNode`] from the live layout tree.
#[derive(Debug, Clone)]
pub struct SavedSessionLeaf {
    pub url: String,
    pub history_pane: bool,
}

impl SavedLayoutNode {
    pub fn leaf_url(url: impl Into<String>) -> Self {
        SavedLayoutNode::Leaf {
            url: url.into(),
            history_pane: false,
        }
    }
}

fn leaf_looks_like_restored_history_placeholder(url: &str, default_url: &str) -> bool {
    let u = url.trim();
    u.is_empty()
        || u.eq_ignore_ascii_case("about:blank")
        || legacy_loopback_embedded_history_ui_url(u)
        || u.to_ascii_lowercase().starts_with("data:text/html")
        || u.eq_ignore_ascii_case(default_url.trim())
}

fn leaf_is_other_live_browsing_url(url: &str, default_url: &str) -> bool {
    let u = url.trim();
    !u.is_empty()
        && allowed_navigation_url(u)
        && !u.eq_ignore_ascii_case(default_url.trim())
}

/// Older session writes stored the history slot as [`vmux_settings::VmuxBrowserSettings::default_webview_url`] with
/// `history_pane: false` (history URLs are not [`allowed_navigation_url`]). Re-label that leaf on restore.
fn fix_mislabeled_history_panes(node: SavedLayoutNode, default_url: &str) -> SavedLayoutNode {
    match node {
        SavedLayoutNode::Split {
            axis,
            ratio,
            left,
            right,
        } => {
            let left = fix_mislabeled_history_panes(*left, default_url);
            let right = fix_mislabeled_history_panes(*right, default_url);
            if let (
                SavedLayoutNode::Leaf {
                    url: u1,
                    history_pane: h1,
                },
                SavedLayoutNode::Leaf {
                    url: u2,
                    history_pane: h2,
                },
            ) = (&left, &right)
            {
                if !*h1 && !*h2 {
                    if legacy_loopback_embedded_history_ui_url(u1.as_str()) {
                        return SavedLayoutNode::Split {
                            axis,
                            ratio,
                            left: Box::new(SavedLayoutNode::Leaf {
                                url: String::new(),
                                history_pane: true,
                            }),
                            right: Box::new(right),
                        };
                    }
                    if legacy_loopback_embedded_history_ui_url(u2.as_str()) {
                        return SavedLayoutNode::Split {
                            axis,
                            ratio,
                            left: Box::new(left),
                            right: Box::new(SavedLayoutNode::Leaf {
                                url: String::new(),
                                history_pane: true,
                            }),
                        };
                    }
                    let p1 = leaf_looks_like_restored_history_placeholder(u1, default_url);
                    let p2 = leaf_looks_like_restored_history_placeholder(u2, default_url);
                    let r1 = leaf_is_other_live_browsing_url(u1, default_url);
                    let r2 = leaf_is_other_live_browsing_url(u2, default_url);
                    if p1 && r2 && !p2 {
                        return SavedLayoutNode::Split {
                            axis,
                            ratio,
                            left: Box::new(SavedLayoutNode::Leaf {
                                url: String::new(),
                                history_pane: true,
                            }),
                            right: Box::new(right),
                        };
                    }
                    if p2 && r1 && !p1 {
                        return SavedLayoutNode::Split {
                            axis,
                            ratio,
                            left: Box::new(left),
                            right: Box::new(SavedLayoutNode::Leaf {
                                url: String::new(),
                                history_pane: true,
                            }),
                        };
                    }
                }
            }
            SavedLayoutNode::Split {
                axis,
                ratio,
                left: Box::new(left),
                right: Box::new(right),
            }
        }
        leaf => leaf,
    }
}

/// Persisted session: layout + per-leaf URLs embedded in [`SavedLayoutNode`] leaves.
///
/// `layout_ron` holds `ron` for [`SavedLayoutNode`]; empty string means no saved layout.
#[derive(Resource, Default, Clone, Reflect)]
#[reflect(Resource, Default)]
pub struct SessionLayoutSnapshot {
    pub layout_ron: String,
}

impl SessionLayoutSnapshot {
    pub fn set_root(&mut self, root: &SavedLayoutNode) {
        self.layout_ron = ron::to_string(root).unwrap_or_default();
    }

    pub fn clear_root(&mut self) {
        self.layout_ron.clear();
    }

    pub fn parsed_root(&self) -> Option<SavedLayoutNode> {
        let s = self.layout_ron.trim();
        if s.is_empty() {
            return None;
        }
        match ron::from_str::<SavedLayoutNode>(s) {
            Ok(n) => Some(n),
            Err(e) => {
                warn!("SessionLayoutSnapshot: invalid layout_ron: {e}");
                None
            }
        }
    }

    /// Like [`parsed_root`](Self::parsed_root), then corrects history panes mis-saved as the default
    /// start page (see [`fix_mislabeled_history_panes`]).
    pub fn parsed_root_for_restore(&self, default_webview_url: &str) -> Option<SavedLayoutNode> {
        self.parsed_root()
            .map(|n| fix_mislabeled_history_panes(n, default_webview_url))
    }
}

/// Last document URL for the primary webview; persisted with moonshine-save (see `vmux` crate).
#[derive(Resource, Default, Clone, Reflect)]
#[reflect(Resource)]
pub struct LastVisitedUrl(pub String);

/// Logical pixel rectangle (origin top-left, +y down) for layout solving.
#[derive(Debug, Clone, Copy)]
pub struct PixelRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// When focus lands on a pane, the pane entity we switched **from** (used to break geometric ties
/// in [`neighbor_pane_in_direction`], e.g. returning to the last-used pane on the right column).
#[derive(Resource, Default)]
pub struct PaneFocusIncoming(pub HashMap<Entity, Entity>);

#[derive(Resource, Default)]
struct PaneFocusPrev(Option<Entity>);

fn track_pane_focus_incoming(
    active: Query<Entity, (With<Pane>, With<Active>)>,
    mut prev: ResMut<PaneFocusPrev>,
    mut incoming: ResMut<PaneFocusIncoming>,
) {
    let Ok(cur) = active.single() else {
        return;
    };
    if prev.0 == Some(cur) {
        return;
    }
    if let Some(old) = prev.0 {
        incoming.0.insert(cur, old);
    }
    prev.0 = Some(cur);
}

fn collect_directional_neighbors(
    rects: &[(Entity, PixelRect)],
    active: Entity,
    dir: PaneSwapDir,
) -> Vec<(Entity, f32, f32)> {
    let Some(ar) = rects.iter().find(|(e, _)| *e == active).map(|(_, r)| *r) else {
        return Vec::new();
    };
    const TOL: f32 = 4.0;

    #[inline]
    fn overlap_1d(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
        let left = a0.max(b0);
        let right = a1.min(b1);
        (right - left).max(0.0)
    }

    let mut out = Vec::new();
    for &(e, o) in rects {
        if e == active {
            continue;
        }
        let (overlap, ok, gap) = match dir {
            PaneSwapDir::Left => {
                if o.x + o.w > ar.x + TOL {
                    continue;
                }
                let ov = overlap_1d(o.y, o.y + o.h, ar.y, ar.y + ar.h);
                let gap = ar.x - (o.x + o.w);
                (ov, ov > 1.0, gap)
            }
            PaneSwapDir::Right => {
                if o.x < ar.x + ar.w - TOL {
                    continue;
                }
                let ov = overlap_1d(o.y, o.y + o.h, ar.y, ar.y + ar.h);
                let gap = o.x - (ar.x + ar.w);
                (ov, ov > 1.0, gap)
            }
            PaneSwapDir::Up => {
                if o.y + o.h > ar.y + TOL {
                    continue;
                }
                let ov = overlap_1d(o.x, o.x + o.w, ar.x, ar.x + ar.w);
                let gap = ar.y - (o.y + o.h);
                (ov, ov > 1.0, gap)
            }
            PaneSwapDir::Down => {
                if o.y < ar.y + ar.h - TOL {
                    continue;
                }
                let ov = overlap_1d(o.x, o.x + o.w, ar.x, ar.x + ar.w);
                let gap = o.y - (ar.y + ar.h);
                (ov, ov > 1.0, gap)
            }
        };
        if ok {
            out.push((e, overlap, gap));
        }
    }
    out
}

fn pick_best_directional_neighbor(candidates: &[(Entity, f32, f32)]) -> Option<Entity> {
    let mut best: Option<(Entity, f32, f32)> = None;
    for &(e, overlap, gap) in candidates {
        let take = match best {
            None => true,
            Some((_, bo, bg)) => {
                overlap > bo + 1.0e-3 || (overlap - bo).abs() <= 1.0e-3 && gap < bg
            }
        };
        if take {
            best = Some((e, overlap, gap));
        }
    }
    best.map(|(e, _, _)| e)
}

/// Tmux **[select-pane](https://man.openbsd.org/tmux.1#select-pane)** (`-L` / `-R` / `-U` / `-D`): pick the adjacent pane in that direction from solved layout rects.
///
/// `prefer_if_valid`: if set and that pane is still a valid neighbor in `dir`, it is chosen instead
/// of the geometric tie-break (same overlap + gap as another candidate).
pub fn neighbor_pane_in_direction(
    rects: &[(Entity, PixelRect)],
    active: Entity,
    dir: PaneSwapDir,
    prefer_if_valid: Option<Entity>,
) -> Option<Entity> {
    let candidates = collect_directional_neighbors(rects, active, dir);
    if candidates.is_empty() {
        return None;
    }
    if let Some(p) = prefer_if_valid {
        if p != active && candidates.iter().any(|(e, _, _)| *e == p) {
            return Some(p);
        }
    }
    pick_best_directional_neighbor(&candidates)
}

const MIN_PANE_PX: f32 = 48.0;

/// Compute leaf rectangles. Skips dead entities if `entity_alive` returns false.
///
/// `pane_border_spacing_px` is the gap **between the two children** of each split (clamped per split so
/// minimum pane sizes can still be satisfied).
///
/// When `zoom_pane` is `Some(z)` and `z` is a live leaf of `node`, returns only `(z, area)` (tmux zoom).
pub fn solve_layout(
    node: &LayoutNode,
    area: PixelRect,
    entity_alive: impl Fn(Entity) -> bool,
    pane_border_spacing_px: f32,
    zoom_pane: Option<Entity>,
) -> Vec<(Entity, PixelRect)> {
    if let Some(z) = zoom_pane {
        if node.contains_leaf(z) && entity_alive(z) && area.w > 0.0 && area.h > 0.0 {
            return vec![(z, area)];
        }
    }
    let mut out = Vec::new();
    solve_layout_inner(node, area, pane_border_spacing_px, &entity_alive, &mut out);
    out
}

#[inline]
fn clamp_split_gap(requested: f32, span: f32) -> f32 {
    if !requested.is_finite() || requested <= 0.0 || !span.is_finite() {
        return 0.0;
    }
    let max_g = (span - 2.0 * MIN_PANE_PX).max(0.0);
    requested.min(max_g)
}

fn solve_layout_inner(
    node: &LayoutNode,
    area: PixelRect,
    pane_border_spacing_px: f32,
    entity_alive: &impl Fn(Entity) -> bool,
    out: &mut Vec<(Entity, PixelRect)>,
) {
    match node {
        LayoutNode::Split {
            axis,
            ratio,
            left,
            right,
        } => {
            let ratio = ratio.clamp(0.05, 0.95);
            match axis {
                LayoutAxis::Horizontal => {
                    let g = clamp_split_gap(pane_border_spacing_px, area.w);
                    let inner_w = area.w - g;
                    let split = (inner_w * ratio).clamp(MIN_PANE_PX, inner_w - MIN_PANE_PX);
                    let left_rect = PixelRect {
                        x: area.x,
                        y: area.y,
                        w: split,
                        h: area.h,
                    };
                    let right_rect = PixelRect {
                        x: area.x + split + g,
                        y: area.y,
                        w: inner_w - split,
                        h: area.h,
                    };
                    solve_layout_inner(left, left_rect, pane_border_spacing_px, entity_alive, out);
                    solve_layout_inner(
                        right,
                        right_rect,
                        pane_border_spacing_px,
                        entity_alive,
                        out,
                    );
                }
                LayoutAxis::Vertical => {
                    let g = clamp_split_gap(pane_border_spacing_px, area.h);
                    let inner_h = area.h - g;
                    let split = (inner_h * ratio).clamp(MIN_PANE_PX, inner_h - MIN_PANE_PX);
                    let top_rect = PixelRect {
                        x: area.x,
                        y: area.y,
                        w: area.w,
                        h: split,
                    };
                    let bot_rect = PixelRect {
                        x: area.x,
                        y: area.y + split + g,
                        w: area.w,
                        h: inner_h - split,
                    };
                    solve_layout_inner(left, top_rect, pane_border_spacing_px, entity_alive, out);
                    solve_layout_inner(right, bot_rect, pane_border_spacing_px, entity_alive, out);
                }
            }
        }
        LayoutNode::Leaf(e) => {
            if entity_alive(*e) && area.w > 0.0 && area.h > 0.0 {
                out.push((*e, area));
            }
        }
    }
}

/// Build a session snapshot from the runtime tree and per-entity URL resolution.
pub fn layout_node_to_saved<F>(node: &LayoutNode, mut leaf_for: F) -> SavedLayoutNode
where
    F: FnMut(Entity) -> SavedSessionLeaf,
{
    layout_node_to_saved_inner(node, &mut leaf_for)
}

fn layout_node_to_saved_inner(
    node: &LayoutNode,
    leaf_for: &mut dyn FnMut(Entity) -> SavedSessionLeaf,
) -> SavedLayoutNode {
    match node {
        LayoutNode::Split {
            axis,
            ratio,
            left,
            right,
        } => SavedLayoutNode::Split {
            axis: *axis,
            ratio: *ratio,
            left: Box::new(layout_node_to_saved_inner(left, leaf_for)),
            right: Box::new(layout_node_to_saved_inner(right, leaf_for)),
        },
        LayoutNode::Leaf(e) => {
            let meta = leaf_for(*e);
            SavedLayoutNode::Leaf {
                url: meta.url,
                history_pane: meta.history_pane,
            }
        }
    }
}

/// Registers reflected layout types (components + session snapshot resource) and pane layout systems.
#[derive(Default)]
pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(loading_bar::LoadingBarPlugin)
            .register_type::<Active>()
            .register_type::<Pane>()
            .register_type::<PaneChromeStrip>()
            .register_type::<PaneChromeLoadingBar>()
            .register_type::<PaneChromeOwner>()
            .register_type::<PaneChromeNeedsUrl>()
            .register_type::<Webview>()
            .register_type::<History>()
            .register_type::<HistoryPaneStandby>()
            .register_type::<HistoryPaneNeedsUrl>()
            .register_type::<Workspace>()
            .register_type::<Window>()
            .register_type::<Profile>()
            .register_type::<Tab>()
            .register_type::<PaneLastUrl>()
            .register_type::<LayoutAxis>()
            .register_type::<SessionLayoutSnapshot>()
            .register_type::<LastVisitedUrl>()
            .init_resource::<LastVisitedUrl>()
            .init_resource::<PaneFocusIncoming>()
            .init_resource::<PaneFocusPrev>()
            .init_resource::<PendingNavigationLoads>()
            .add_observer(pane_lifecycle::warn_if_pane_despawn_still_in_layout)
            .add_systems(
                PostUpdate,
                (
                    pane_pointer_focus::update_active_pane_under_cursor
                        .after(camera_system),
                    track_pane_focus_incoming
                        .after(pane_pointer_focus::update_active_pane_under_cursor),
                    apply_pane_layout
                        .after(track_pane_focus_incoming)
                        .before(render_standard_materials),
                    apply_pane_loading_bar_layout
                        .after(apply_pane_layout)
                        .before(render_standard_materials),
                    apply_pane_chrome_layout
                        .after(apply_pane_loading_bar_layout)
                        .before(render_standard_materials),
                    sync_cef_sizes_after_pane_layout
                        .after(apply_pane_chrome_layout)
                        .before(render_standard_materials),
                ),
            )
            .add_systems(
                Update,
                (
                    split_active_pane.after(VmuxPrefixChordSet),
                    cycle_pane_focus.after(VmuxPrefixChordSet),
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_pane_focus_incoming_system_records_previous_focus() {
        let mut app = App::new();
        app.init_resource::<PaneFocusIncoming>();
        app.init_resource::<PaneFocusPrev>();
        app.add_systems(Update, track_pane_focus_incoming);

        let a = app.world_mut().spawn((Pane, Active)).id();
        let b = app.world_mut().spawn(Pane).id();

        app.update();
        assert!(app
            .world()
            .resource::<PaneFocusIncoming>()
            .0
            .get(&a)
            .is_none());

        app.world_mut().entity_mut(a).remove::<Active>();
        app.world_mut().entity_mut(b).insert(Active);
        app.update();

        assert_eq!(
            app.world().resource::<PaneFocusIncoming>().0.get(&b).copied(),
            Some(a)
        );
    }
}
