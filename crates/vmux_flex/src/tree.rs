//! The taffy tree the ECS hierarchy is mirrored into.

use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use taffy::TaffyTree;
use taffy::style_helpers;

use crate::node::{AlignItems, Display, FlexDirection, JustifyContent, Node, PositionType, Val};

/// What lengths are resolved against for one frame.
///
/// One per frame for the whole tree, not one per node: the shell has a single window, so there is
/// nothing for a second context to describe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutContext {
    pub scale_factor: f32,
    pub physical_size: Vec2,
}

impl LayoutContext {
    pub fn of(window: &Window) -> Self {
        Self {
            scale_factor: window.resolution.scale_factor(),
            physical_size: Vec2::new(
                window.resolution.physical_width() as f32,
                window.resolution.physical_height() as f32,
            ),
        }
    }

    pub fn style_for(&self, node: &Node) -> taffy::Style {
        taffy::Style {
            display: match node.display {
                Display::Flex => taffy::Display::Flex,
                Display::None => taffy::Display::None,
            },
            position: match node.position_type {
                PositionType::Relative => taffy::Position::Relative,
                PositionType::Absolute => taffy::Position::Absolute,
            },
            flex_direction: match node.flex_direction {
                FlexDirection::Row => taffy::FlexDirection::Row,
                FlexDirection::Column => taffy::FlexDirection::Column,
            },
            align_items: match node.align_items {
                AlignItems::Default => None,
                AlignItems::Stretch => Some(taffy::AlignItems::Stretch),
            },
            justify_content: match node.justify_content {
                JustifyContent::Default => None,
                JustifyContent::Stretch => Some(taffy::JustifyContent::Stretch),
            },
            inset: taffy::Rect {
                left: self.length_percentage_auto(node.left),
                right: self.length_percentage_auto(node.right),
                top: self.length_percentage_auto(node.top),
                bottom: self.length_percentage_auto(node.bottom),
            },
            padding: taffy::Rect {
                left: self.length_percentage(node.padding.left),
                right: self.length_percentage(node.padding.right),
                top: self.length_percentage(node.padding.top),
                bottom: self.length_percentage(node.padding.bottom),
            },
            flex_grow: node.flex_grow,
            flex_shrink: node.flex_shrink,
            flex_basis: self.dimension(node.flex_basis),
            size: taffy::Size {
                width: self.dimension(node.width),
                height: self.dimension(node.height),
            },
            min_size: taffy::Size {
                width: self.dimension(node.min_width),
                height: self.dimension(node.min_height),
            },
            // taffy's `gap` is (main, cross) as (width, height), so the column gap is the width.
            gap: taffy::Size {
                width: self.length_percentage(node.column_gap),
                height: self.length_percentage(node.row_gap),
            },
            ..Default::default()
        }
    }

    fn length_percentage_auto(&self, value: Val) -> taffy::LengthPercentageAuto {
        match value {
            Val::Auto => style_helpers::auto(),
            Val::Px(px) => style_helpers::length(self.scale_factor * px),
            Val::Percent(percent) => style_helpers::percent(percent / 100.0),
        }
    }

    /// `Auto` has no meaning for padding or a gap, and collapses to zero.
    fn length_percentage(&self, value: Val) -> taffy::LengthPercentage {
        match value {
            Val::Auto => style_helpers::length(0.0),
            Val::Px(px) => style_helpers::length(self.scale_factor * px),
            Val::Percent(percent) => style_helpers::percent(percent / 100.0),
        }
    }

    fn dimension(&self, value: Val) -> taffy::Dimension {
        self.length_percentage_auto(value).into()
    }
}

/// `TaffyTree` is not `Send` because a `CompactLength` can carry a pointer to a `calc` expression.
///
/// That only happens with taffy's `calc` feature, which this crate's manifest pins off and explains
/// why. `bevy_ui` carries the identical newtype and justification.
struct TaffyCell(TaffyTree<()>);

#[expect(
    unsafe_code,
    reason = "TaffyTree is Send as long as the calc feature is off"
)]
// SAFETY: taffy is thread-unsafe only through `calc`, which this crate does not enable.
unsafe impl Send for TaffyCell {}

#[expect(
    unsafe_code,
    reason = "TaffyTree is Sync as long as the calc feature is off"
)]
// SAFETY: taffy is thread-unsafe only through `calc`, which this crate does not enable.
unsafe impl Sync for TaffyCell {}

/// A taffy node standing in for one ECS entity, and the implicit viewport wrapping it if it is a
/// root.
#[derive(Clone, Copy)]
struct FlexNode {
    id: taffy::NodeId,
    viewport: Option<taffy::NodeId>,
}

/// The taffy tree, and the mapping back to the entities it mirrors.
#[derive(Resource, Default)]
pub struct FlexTree {
    taffy: TaffyCell,
    entities: EntityHashMap<FlexNode>,
    context: Option<LayoutContext>,
}

impl Default for TaffyCell {
    fn default() -> Self {
        Self(TaffyTree::new())
    }
}

impl FlexTree {
    pub fn contains(&self, entity: Entity) -> bool {
        self.entities.contains_key(&entity)
    }

    /// Create or restyle the taffy node standing in for `entity`.
    pub fn upsert(&mut self, context: &LayoutContext, entity: Entity, node: &Node) {
        let style = context.style_for(node);
        match self.entities.get(&entity) {
            Some(existing) => {
                let _ = self.taffy.0.set_style(existing.id, style);
            }
            None => {
                let Ok(id) = self.taffy.0.new_leaf(style) else {
                    return;
                };
                self.entities
                    .insert(entity, FlexNode { id, viewport: None });
            }
        }
    }

    pub fn set_children(&mut self, entity: Entity, children: &[Entity]) {
        let Some(parent) = self.entities.get(&entity).copied() else {
            return;
        };
        let mut ids = Vec::with_capacity(children.len());
        for child in children {
            if let Some(node) = self.entities.get(child) {
                ids.push(node.id);
            }
        }
        let _ = self.taffy.0.set_children(parent.id, &ids);
    }

    /// Drop the node for `entity`, and the viewport wrapping it if it had one.
    pub fn remove(&mut self, entity: Entity) {
        let Some(node) = self.entities.remove(&entity) else {
            return;
        };
        if let Some(viewport) = node.viewport {
            let _ = self.taffy.0.remove(viewport);
        }
        let _ = self.taffy.0.remove(node.id);
    }

    /// Release a root's viewport wrapper without dropping the node — it has become someone's child.
    pub fn detach_viewport(&mut self, entity: Entity) {
        let Some(node) = self.entities.get_mut(&entity) else {
            return;
        };
        let Some(viewport) = node.viewport.take() else {
            return;
        };
        let _ = self.taffy.0.remove(viewport);
    }

    pub fn has_viewport(&self, entity: Entity) -> bool {
        self.entities
            .get(&entity)
            .is_some_and(|node| node.viewport.is_some())
    }

    /// Lay `entity` out as a root, against the whole render target.
    ///
    /// The root is wrapped in an implicit grid node filling the target rather than handed to taffy
    /// directly, because taffy's root path is a different algorithm: it ignores the root's own
    /// inset and pins its x to zero. Wrapping keeps a root laying out exactly as a child would.
    pub fn compute(&mut self, context: &LayoutContext, entity: Entity) {
        let Some(node) = self.entities.get(&entity).copied() else {
            return;
        };
        let viewport = match node.viewport {
            Some(viewport) => viewport,
            None => {
                let style = taffy::Style {
                    display: taffy::Display::Grid,
                    size: taffy::Size {
                        width: style_helpers::percent(1.0),
                        height: style_helpers::percent(1.0),
                    },
                    align_items: Some(taffy::AlignItems::Start),
                    justify_items: Some(taffy::AlignItems::Start),
                    ..Default::default()
                };
                let Ok(viewport) = self.taffy.0.new_leaf(style) else {
                    return;
                };
                if self.taffy.0.add_child(viewport, node.id).is_err() {
                    let _ = self.taffy.0.remove(viewport);
                    return;
                }
                self.entities.entry(entity).and_modify(|slot| {
                    slot.viewport = Some(viewport);
                });
                viewport
            }
        };
        let available = taffy::Size {
            width: taffy::AvailableSpace::Definite(context.physical_size.x),
            height: taffy::AvailableSpace::Definite(context.physical_size.y),
        };
        let _ = self.taffy.0.compute_layout(viewport, available);
    }

    pub fn layout_of(&self, entity: Entity) -> Option<&taffy::Layout> {
        let node = self.entities.get(&entity)?;
        self.taffy.0.layout(node.id).ok()
    }

    /// The context the tree was last styled against, so a scale-factor change can be noticed.
    pub fn context(&self) -> Option<LayoutContext> {
        self.context
    }

    pub fn set_context(&mut self, context: LayoutContext) {
        self.context = Some(context);
    }

    #[cfg(test)]
    pub(crate) fn node_count(&self) -> usize {
        self.taffy.0.total_node_count()
    }
}
