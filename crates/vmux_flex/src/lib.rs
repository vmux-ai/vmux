//! Flexbox layout for the desktop shell.
//!
//! The shell draws nothing through Bevy: every surface is a native view that AppKit composites.
//! What it needs from a layout engine is rectangles — where each pane, header and side sheet ends
//! up — so this crate mirrors the ECS hierarchy into a [`taffy`] tree, computes it against the
//! window, and writes the result back as [`ComputedNode`].
//!
//! Lengths go in as logical pixels and come out physical: [`Val::Px`] is multiplied by the render
//! target's scale factor before taffy sees it, so every computed rectangle is already in the space
//! the pointer, the framebuffer and `setFrame:` all use.
#![allow(clippy::too_many_arguments)]

pub mod computed;
pub mod node;
pub mod tree;
pub mod visibility;
mod write;

pub use computed::{ComputedNode, Insets};
pub use node::{
    AlignItems, Display, FlexDirection, JustifyContent, Node, PositionType, UiRect, Val,
};
pub use tree::{FlexTree, LayoutContext};
pub use visibility::Visibility;

/// The names a crate laying nodes out needs, to be glob-imported the way `bevy::prelude` used to
/// supply them.
pub mod prelude {
    pub use crate::computed::{ComputedNode, Insets};
    pub use crate::node::{
        AlignItems, Display, FlexDirection, JustifyContent, Node, PositionType, UiRect, Val,
    };
    pub use crate::visibility::Visibility;
    pub use crate::{FlexPlugin, LayoutSystems};
}

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use write::GeometryWalk;

/// Computes [`ComputedNode`] from [`Node`] every frame, in `PostUpdate`.
///
/// Anything that writes a [`Node`] belongs before [`LayoutSystems::Layout`]; anything that reads a
/// [`ComputedNode`] belongs after it.
pub struct FlexPlugin;

impl Plugin for FlexPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlexTree>()
            .configure_sets(
                PostUpdate,
                (LayoutSystems::Layout, LayoutSystems::PostLayout).chain(),
            )
            .add_systems(PostUpdate, compute_layout.in_set(LayoutSystems::Layout));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutSystems {
    /// Styles are read and geometry is produced. Mutating a [`Node`] here is too late.
    Layout,
    /// Geometry is settled and can be read.
    PostLayout,
}

type NodeQuery<'w, 's> = Query<'w, 's, (Entity, Ref<'static, Node>)>;

fn compute_layout(
    mut tree: ResMut<FlexTree>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    nodes: NodeQuery,
    added: Query<(), Added<Node>>,
    children_q: Query<&Children>,
    changed_children: Query<(), Changed<Children>>,
    roots: Query<Entity, (With<Node>, Without<ChildOf>)>,
    mut removed_nodes: RemovedComponents<Node>,
    mut removed_children: RemovedComponents<Children>,
    mut out: Query<&mut ComputedNode>,
) {
    let Some(window) = window else {
        return;
    };
    let context = LayoutContext::of(&window);
    if context.physical_size.x <= 0.0 || context.physical_size.y <= 0.0 {
        return;
    }
    // A scale-factor change restyles everything, because `Val::Px` is baked into taffy at the
    // factor that was current when it was written. Without this a Retina move leaves every
    // pixel length at the old scale, which is invisible at a factor of 1.
    let context_changed = tree.context() != Some(context);
    tree.set_context(context);

    for (entity, node) in nodes.iter() {
        if context_changed || node.is_changed() {
            tree.upsert(&context, entity, &node);
        }
    }

    for entity in removed_children.read() {
        tree.set_children(entity, &[]);
    }
    // A `Node` removed and re-inserted in the same frame still emits a removal, so a live entity
    // has to survive it.
    for entity in removed_nodes.read() {
        if !nodes.contains(entity) {
            tree.remove(entity);
        }
    }

    for root in roots.iter() {
        sync_children_recursively(&mut tree, root, &children_q, &added, &changed_children);
        tree.compute(&context, root);
        let walk = GeometryWalk {
            tree: &tree,
            inverse_scale_factor: context.scale_factor.recip(),
        };
        walk.descend(root, Vec2::ZERO, Vec2::ZERO, &children_q, &mut out);
    }
}

/// Re-parent in taffy wherever a `Children` list changed or a child gained a `Node`.
///
/// Recursion does not stop at an entity without a `Node`: a `Node` nested under a plain entity is
/// still reached, matching how the hierarchy is built when a whole subtree is spawned at once and
/// the intermediate entities gain their components in the same command buffer.
fn sync_children_recursively(
    tree: &mut FlexTree,
    entity: Entity,
    children_q: &Query<&Children>,
    added: &Query<(), Added<Node>>,
    changed_children: &Query<(), Changed<Children>>,
) {
    let Ok(children) = children_q.get(entity) else {
        return;
    };
    if tree.contains(entity) {
        let gained_a_child = children.iter().any(|child| added.contains(child));
        if added.contains(entity) || changed_children.contains(entity) || gained_a_child {
            let owned: Vec<Entity> = children.iter().collect();
            tree.set_children(entity, &owned);
            // Anything now parented is no longer a root, so its viewport wrapper would leak.
            for child in &owned {
                if tree.has_viewport(*child) {
                    tree.detach_viewport(*child);
                }
            }
        }
    }
    for child in children.iter() {
        sync_children_recursively(tree, child, children_q, added, changed_children);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut resolution = bevy::window::WindowResolution::default();
        resolution.set_scale_factor_override(Some(2.0));
        resolution.set_physical_resolution(1280, 800);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(bevy::window::WindowPlugin {
                primary_window: Some(Window {
                    resolution,
                    ..default()
                }),
                ..default()
            })
            .add_plugins(FlexPlugin);
        app
    }

    fn fill() -> Node {
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        }
    }

    /// Every root carries an implicit viewport node. The pane tree reparents on every split, close
    /// and archive restore, so a viewport that outlives its root grows the taffy tree without bound.
    #[test]
    fn a_root_that_becomes_a_child_gives_up_its_viewport() {
        let mut app = app();
        let a = app.world_mut().spawn(fill()).id();
        let b = app.world_mut().spawn(fill()).id();
        app.update();
        let two_roots = app.world().resource::<FlexTree>().node_count();

        app.world_mut().entity_mut(b).insert(ChildOf(a));
        app.update();
        let one_root = app.world().resource::<FlexTree>().node_count();

        assert_eq!(
            one_root,
            two_roots - 1,
            "parenting a root should free exactly its viewport node"
        );

        app.world_mut().entity_mut(b).despawn();
        app.update();
        assert_eq!(
            app.world().resource::<FlexTree>().node_count(),
            one_root - 1,
            "despawning a node should free it"
        );
    }
}
