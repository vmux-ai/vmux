//! Hierarchical pane layout tree (runtime), pixel solver, session snapshot types, and pane layout systems.
//!
//! [`LayoutPlugin`] registers reflected layout types, [`VmuxWorldCamera`], and `PostUpdate` pane layout
//! + CEF resize sync (after Bevy’s [`camera_system`](bevy::render::camera::camera_system)).

mod hosted_web_ui;
mod loading_bar;
mod pane_layout;
mod pane_lifecycle;
mod pane_ops;
mod pane_spawn;
pub mod tmux;
mod url;

use bevy::prelude::*;
use bevy::render::camera::camera_system;
use bevy_cef::prelude::render_standard_materials;
use serde::{Deserialize, Serialize};
use vmux_core::VmuxPrefixChordSet;

pub use loading_bar::{
    LoadingBarMaterial, LOADING_BAR_ANIM_TIME_SCALE, LOADING_BAR_DEPTH_BIAS_ABOVE_PANE,
    LOADING_BAR_HEIGHT_PX, PaneChromeLoadingBar,
    PendingNavigationLoads,
};
pub use hosted_web_ui::{VmuxHostedWebPlugin, VmuxWebviewSurface};
pub use pane_layout::{
    CHROME_BORDER_OUTSET_PX, PANE_Z_STRIDE, apply_pane_chrome_layout, apply_pane_layout,
    apply_pane_loading_bar_layout, clamp_webview_backing_size, layout_viewport_for_workspace,
    layout_workspace_pane_rects, pixel_rect_to_world_plane, split_pane_content_and_chrome,
    sync_cef_sizes_after_pane_layout,
};
pub use pane_ops::{
    cycle_pane_focus, rebuild_session_snapshot, split_active_pane, try_cycle_pane_focus,
    try_kill_active_pane, try_mirror_pane_layout, try_rotate_window, try_select_pane_direction,
    try_split_active_pane, try_swap_active_pane, try_toggle_zoom_pane,
};
pub use pane_spawn::{
    CEF_PAGE_ZOOM_LEVEL, TEXT_INPUT_EMACS_BINDINGS_PRELOAD, URL_TRACK_PRELOAD, VmuxWebview,
    setup_vmux_panes, spawn_pane, spawn_saved_recursive,
};
pub use url::{allowed_navigation_url, initial_webview_url, sanitize_embedded_webview_url};
pub use vmux_core::Active;
pub use vmux_settings::{VmuxAppSettings, default_webview_url};

/// Z distance of the world camera from the webview plane at z = 0 (used for frustum sizing).
pub const CAMERA_DISTANCE: f32 = 3.0;

/// Marker for the vmux world-facing camera used to size the webview plane.
#[derive(Component)]
pub struct VmuxWorldCamera;

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

/// Default height in **layout pixels** of the [`PaneChromeStrip`] overlay at the bottom of each pane tile.
/// Sized for the ~11px status bar (`vmux_status_bar`) with a little padding.
pub const DEFAULT_PANE_CHROME_HEIGHT_PX: f32 = 28.0;

/// Singleton anchor for subsystems (e.g. layout host with [`LayoutTree`]).
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct Root;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect, Serialize, Deserialize)]
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

/// Layout tree on the [`Root`] entity.
#[derive(Component, Debug, Clone)]
pub struct LayoutTree {
    pub root: LayoutNode,
    pub revision: u64,
    /// When set, [`solve_layout`] assigns the full root area to this pane only (tmux **`resize-pane -Z`**).
    /// Not persisted in session snapshots.
    pub zoom_pane: Option<Entity>,
}

impl LayoutTree {
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
    },
}

impl SavedLayoutNode {
    pub fn leaf_url(url: impl Into<String>) -> Self {
        SavedLayoutNode::Leaf { url: url.into() }
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
        ron::from_str(s).ok()
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

/// Tmux **[select-pane](https://man.openbsd.org/tmux.1#select-pane)** (`-L` / `-R` / `-U` / `-D`): pick the adjacent pane in that direction from solved layout rects.
pub fn neighbor_pane_in_direction(
    rects: &[(Entity, PixelRect)],
    active: Entity,
    dir: PaneSwapDir,
) -> Option<Entity> {
    let ar = rects.iter().find(|(e, _)| *e == active).map(|(_, r)| *r)?;
    // Pixel tolerance for gaps and float edges (matches typical pane-border spacing scale).
    const TOL: f32 = 4.0;

    #[inline]
    fn overlap_1d(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
        let left = a0.max(b0);
        let right = a1.min(b1);
        (right - left).max(0.0)
    }

    let mut best: Option<(Entity, f32, f32)> = None;

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
        if !ok {
            continue;
        }
        let take = match best {
            None => true,
            Some((_, bo, bg)) => overlap > bo + 1.0e-3 || (overlap - bo).abs() <= 1.0e-3 && gap < bg,
        };
        if take {
            best = Some((e, overlap, gap));
        }
    }
    best.map(|(e, _, _)| e)
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
pub fn layout_node_to_saved<F>(node: &LayoutNode, mut url_for: F) -> SavedLayoutNode
where
    F: FnMut(Entity) -> String,
{
    layout_node_to_saved_inner(node, &mut url_for)
}

fn layout_node_to_saved_inner(
    node: &LayoutNode,
    url_for: &mut dyn FnMut(Entity) -> String,
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
            left: Box::new(layout_node_to_saved_inner(left, url_for)),
            right: Box::new(layout_node_to_saved_inner(right, url_for)),
        },
        LayoutNode::Leaf(e) => SavedLayoutNode::Leaf { url: url_for(*e) },
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
            .register_type::<Root>()
            .register_type::<PaneLastUrl>()
            .register_type::<LayoutAxis>()
            .register_type::<SessionLayoutSnapshot>()
            .register_type::<LastVisitedUrl>()
            .init_resource::<LastVisitedUrl>()
            .init_resource::<PendingNavigationLoads>()
            .add_observer(pane_lifecycle::warn_if_pane_despawn_still_in_layout)
            .add_systems(Startup, setup_vmux_panes)
            .add_systems(
                PostUpdate,
                (
                    apply_pane_layout
                        .after(camera_system)
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
