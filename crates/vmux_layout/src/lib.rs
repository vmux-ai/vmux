//! Hierarchical pane layout tree (runtime), pixel solver, and session snapshot types.
//!
//! Use [`LayoutPlugin`] to register reflected layout types with the Bevy app.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Marks the focused pane (paired with [`Pane`]).
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct Active;

/// Marks a pane entity (e.g. CEF webview).
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct Pane;

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
}

fn remove_leaf_in_place(node: &mut LayoutNode, target: Entity) -> bool {
    match node {
        LayoutNode::Leaf(_) => false,
        LayoutNode::Split { left, right, .. } => {
            match (left.as_mut(), right.as_mut()) {
                (LayoutNode::Leaf(le), _) if *le == target => {
                    let promoted = std::mem::replace(
                        &mut **right,
                        LayoutNode::Leaf(Entity::PLACEHOLDER),
                    );
                    *node = promoted;
                    true
                }
                (_, LayoutNode::Leaf(re)) if *re == target => {
                    let promoted = std::mem::replace(
                        &mut **left,
                        LayoutNode::Leaf(Entity::PLACEHOLDER),
                    );
                    *node = promoted;
                    true
                }
                (l, _) if l.contains_leaf(target) => remove_leaf_in_place(l, target),
                (_, r) if r.contains_leaf(target) => remove_leaf_in_place(r, target),
                _ => false,
            }
        }
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

/// Logical pixel rectangle (origin top-left, +y down) for layout solving.
#[derive(Debug, Clone, Copy)]
pub struct PixelRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

const MIN_PANE_PX: f32 = 48.0;

/// Compute leaf rectangles. Skips dead entities if `entity_alive` returns false.
pub fn solve_layout(
    node: &LayoutNode,
    area: PixelRect,
    entity_alive: impl Fn(Entity) -> bool,
) -> Vec<(Entity, PixelRect)> {
    let mut out = Vec::new();
    solve_layout_inner(node, area, &entity_alive, &mut out);
    out
}

fn solve_layout_inner(
    node: &LayoutNode,
    area: PixelRect,
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
                    let split = (area.w * ratio).clamp(MIN_PANE_PX, area.w - MIN_PANE_PX);
                    let left_rect = PixelRect {
                        x: area.x,
                        y: area.y,
                        w: split,
                        h: area.h,
                    };
                    let right_rect = PixelRect {
                        x: area.x + split,
                        y: area.y,
                        w: area.w - split,
                        h: area.h,
                    };
                    solve_layout_inner(left, left_rect, entity_alive, out);
                    solve_layout_inner(right, right_rect, entity_alive, out);
                }
                LayoutAxis::Vertical => {
                    let split = (area.h * ratio).clamp(MIN_PANE_PX, area.h - MIN_PANE_PX);
                    let top_rect = PixelRect {
                        x: area.x,
                        y: area.y,
                        w: area.w,
                        h: split,
                    };
                    let bot_rect = PixelRect {
                        x: area.x,
                        y: area.y + split,
                        w: area.w,
                        h: area.h - split,
                    };
                    solve_layout_inner(left, top_rect, entity_alive, out);
                    solve_layout_inner(right, bot_rect, entity_alive, out);
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

/// Registers reflected layout types (components + session snapshot resource).
#[derive(Default)]
pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Active>()
            .register_type::<Pane>()
            .register_type::<Root>()
            .register_type::<PaneLastUrl>()
            .register_type::<LayoutAxis>()
            .register_type::<SessionLayoutSnapshot>();
    }
}
