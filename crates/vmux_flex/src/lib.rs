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

pub mod prelude {
    pub use crate::computed::{ComputedNode, Insets};
    pub use crate::node::{
        AlignItems, Display, FlexDirection, JustifyContent, Node, PositionType, UiRect, Val,
    };
    pub use crate::visibility::Visibility;
    pub use crate::{FlexPlugin, FlexViewport, LayoutSystems};
}

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use write::GeometryWalk;

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

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlexViewport(pub Entity);

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutSystems {
    Layout,
    PostLayout,
}

type NodeQuery<'w, 's> = Query<'w, 's, (Entity, Ref<'static, Node>)>;
type RootQuery<'w, 's> =
    Query<'w, 's, (Entity, Option<&'static FlexViewport>), (With<Node>, Without<ChildOf>)>;

fn compute_layout(
    mut tree: ResMut<FlexTree>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    nodes: NodeQuery,
    added: Query<(), Added<Node>>,
    children_q: Query<&Children>,
    changed_children: Query<(), Changed<Children>>,
    roots: RootQuery,
    mut removed_nodes: RemovedComponents<Node>,
    mut removed_children: RemovedComponents<Children>,
    mut out: Query<&mut ComputedNode>,
) {
    for entity in removed_children.read() {
        tree.set_children(entity, &[]);
    }
    for entity in removed_nodes.read() {
        if !nodes.contains(entity) {
            tree.remove(entity);
        }
    }

    for (root, viewport) in roots.iter() {
        let window_entity = viewport
            .map(|viewport| viewport.0)
            .or_else(|| primary_window.single().ok());
        let Some(window) = window_entity.and_then(|entity| windows.get(entity).ok()) else {
            continue;
        };
        let context = LayoutContext::from(window);
        if context.physical_size.x <= 0.0 || context.physical_size.y <= 0.0 {
            continue;
        }
        let context_changed = tree.context(root) != Some(context);
        tree.set_context(root, context);
        sync_nodes_recursively(
            &mut tree,
            root,
            &context,
            context_changed,
            &nodes,
            &children_q,
            &added,
            &changed_children,
        );
        tree.compute(&context, root);
        let walk = GeometryWalk {
            tree: &tree,
            inverse_scale_factor: context.scale_factor.recip(),
        };
        walk.descend(root, Vec2::ZERO, Vec2::ZERO, &children_q, &mut out);
    }
}

fn sync_nodes_recursively(
    tree: &mut FlexTree,
    entity: Entity,
    context: &LayoutContext,
    context_changed: bool,
    nodes: &NodeQuery,
    children_q: &Query<&Children>,
    added: &Query<(), Added<Node>>,
    changed_children: &Query<(), Changed<Children>>,
) {
    let Ok((_, node)) = nodes.get(entity) else {
        return;
    };
    if context_changed || node.is_changed() || added.contains(entity) {
        tree.upsert(context, entity, &node);
    }
    let Ok(children) = children_q.get(entity) else {
        return;
    };
    for child in children.iter() {
        sync_nodes_recursively(
            tree,
            child,
            context,
            context_changed,
            nodes,
            children_q,
            added,
            changed_children,
        );
    }
    if tree.contains(entity) {
        let gained_a_child = children.iter().any(|child| added.contains(child));
        if added.contains(entity) || changed_children.contains(entity) || gained_a_child {
            let owned: Vec<Entity> = children.iter().collect();
            tree.set_children(entity, &owned);
            for child in &owned {
                if tree.has_viewport(*child) {
                    tree.detach_viewport(*child);
                }
            }
        }
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

    #[test]
    fn roots_use_their_own_window_size() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(FlexPlugin);
        let first_window = app
            .world_mut()
            .spawn(Window {
                resolution: (800, 600).into(),
                ..default()
            })
            .id();
        let second_window = app
            .world_mut()
            .spawn(Window {
                resolution: (1200, 900).into(),
                ..default()
            })
            .id();
        let first = app
            .world_mut()
            .spawn((fill(), FlexViewport(first_window)))
            .id();
        let second = app
            .world_mut()
            .spawn((fill(), FlexViewport(second_window)))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<ComputedNode>(first).unwrap().size,
            Vec2::new(800.0, 600.0)
        );
        assert_eq!(
            app.world().get::<ComputedNode>(second).unwrap().size,
            Vec2::new(1200.0, 900.0)
        );
    }
}
