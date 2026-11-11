use bevy_app::{App, Plugin, PostUpdate};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_math::Vec2;

pub use computed::{ComputedNode, Insets};
pub use node::{
    AlignItems, Display, FlexDirection, JustifyContent, Node, PositionType, UiRect, Val,
};
pub use tree::{FlexTree, LayoutContext};
pub use visibility::Visibility;
#[cfg(feature = "window")]
pub use window::WindowViewportPlugin;
use write::GeometryWriter;

pub struct FlexPlugin;

impl Plugin for FlexPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlexTree>()
            .configure_sets(
                PostUpdate,
                (
                    LayoutSystems::Viewport,
                    LayoutSystems::Layout,
                    LayoutSystems::PostLayout,
                )
                    .chain(),
            )
            .add_systems(PostUpdate, compute.in_set(LayoutSystems::Layout));
    }
}

pub mod computed;
pub mod node;
pub mod tree;
pub mod visibility;
#[cfg(feature = "window")]
pub mod window;
mod write;

pub mod prelude {
    #[cfg(feature = "window")]
    pub use crate::WindowViewportPlugin;
    pub use crate::{
        AlignItems, ComputedNode, Display, FlexDirection, FlexPlugin, FlexViewport, Insets,
        JustifyContent, LayoutContext, LayoutSystems, Node, PositionType, PrimaryViewport, UiRect,
        Val, Visibility,
    };
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlexViewport(pub Entity);

#[derive(Component)]
pub struct PrimaryViewport;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutSystems {
    Viewport,
    Layout,
    PostLayout,
}

type NodeQuery<'w, 's> = Query<'w, 's, (Ref<'static, Node>, Option<Ref<'static, ChildOf>>)>;
type RootQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static FlexViewport>,
        Option<&'static ChildOf>,
    ),
    With<Node>,
>;

#[derive(SystemParam)]
struct Viewports<'w, 's> {
    contexts: Query<'w, 's, &'static LayoutContext>,
    primary: Query<'w, 's, Entity, (With<PrimaryViewport>, With<LayoutContext>)>,
}

impl Viewports<'_, '_> {
    fn root(&self, root: Entity, viewport: Option<&FlexViewport>) -> Option<LayoutContext> {
        if let Some(viewport) = viewport {
            return self.contexts.get(viewport.0).ok().copied();
        }
        if let Ok(local) = self.contexts.get(root) {
            return Some(*local);
        }
        let primary = self.primary.single().ok()?;
        self.contexts.get(primary).ok().copied()
    }

    fn explicit(&self, root: Entity, viewport: Option<&FlexViewport>) -> bool {
        viewport.is_some() || self.contexts.contains(root)
    }
}

fn compute(
    mut tree: ResMut<FlexTree>,
    viewports: Viewports,
    nodes: NodeQuery,
    roots: RootQuery,
    mut removed_nodes: RemovedComponents<Node>,
    mut removed_children: RemovedComponents<Children>,
    mut geometry: GeometryWriter,
) {
    for entity in removed_children.read() {
        tree.set_children(entity, &[]);
    }
    for entity in removed_nodes.read() {
        if !nodes.contains(entity) {
            tree.remove(entity);
        }
    }
    for (root, viewport, parent) in &roots {
        if let Some(parent) = parent {
            if nodes.contains(parent.parent()) {
                tree.detach_viewport(root);
                continue;
            }
            if !viewports.explicit(root, viewport) {
                tree.invalidate(root);
                continue;
            }
        }
        let Some(context) = viewports.root(root, viewport) else {
            tree.invalidate(root);
            continue;
        };
        if !context.is_valid() {
            tree.invalidate(root);
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
            &geometry.children,
        );
        tree.compute(&context, root);
        geometry.descend(
            &tree,
            context.scale_factor.recip(),
            root,
            Vec2::ZERO,
            Vec2::ZERO,
        );
    }
}

fn sync_nodes_recursively(
    tree: &mut FlexTree,
    entity: Entity,
    context: &LayoutContext,
    context_changed: bool,
    nodes: &NodeQuery,
    children_q: &Query<Ref<Children>>,
) {
    let Ok((node, parent)) = nodes.get(entity) else {
        return;
    };
    let context_changed = context_changed || parent.is_some_and(|parent| parent.is_changed());
    if context_changed || node.is_changed() {
        tree.upsert(context, entity, &node);
    }
    let Ok(children) = children_q.get(entity) else {
        return;
    };
    for child in children.iter() {
        sync_nodes_recursively(tree, child, context, context_changed, nodes, children_q);
    }
    if !tree.contains(entity) {
        return;
    }
    let mut gained_a_child = false;
    for child in children.iter() {
        if let Ok((node, _)) = nodes.get(child)
            && node.is_added()
        {
            gained_a_child = true;
            break;
        }
    }
    if context_changed || node.is_added() || children.is_changed() || gained_a_child {
        let owned: Vec<Entity> = children.iter().collect();
        tree.set_children(entity, &owned);
        for child in &owned {
            if tree.has_viewport(*child) {
                tree.detach_viewport(*child);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Harness(App);

    impl Harness {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(FlexPlugin);
            Self(app)
        }

        fn root(&mut self, size: Vec2, scale: f32, node: Node) -> Entity {
            self.0
                .world_mut()
                .spawn((
                    node,
                    LayoutContext {
                        physical_size: size,
                        scale_factor: scale,
                    },
                ))
                .id()
        }

        fn fill() -> Node {
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..Default::default()
            }
        }

        fn rect(&self, entity: Entity) -> ComputedNode {
            *self.0.world().get::<ComputedNode>(entity).unwrap()
        }
    }

    #[test]
    fn independent_viewports_preserve_physical_and_logical_geometry() {
        let mut fixture = Harness::new();
        let first = fixture.root(Vec2::new(800.0, 600.0), 1.0, Harness::fill());
        let viewport = fixture
            .0
            .world_mut()
            .spawn(LayoutContext {
                physical_size: Vec2::new(1200.0, 900.0),
                scale_factor: 2.0,
            })
            .id();
        let second = fixture
            .0
            .world_mut()
            .spawn((Harness::fill(), FlexViewport(viewport)))
            .id();
        fixture.0.update();
        assert_eq!(fixture.rect(first).size, Vec2::new(800.0, 600.0));
        assert_eq!(fixture.rect(second).size, Vec2::new(1200.0, 900.0));
        assert_eq!(
            fixture.rect(second).to_logical().size,
            Vec2::new(600.0, 450.0)
        );
        fixture
            .0
            .world_mut()
            .get_mut::<LayoutContext>(viewport)
            .unwrap()
            .physical_size = Vec2::new(1600.0, 1000.0);
        fixture.0.update();
        assert_eq!(
            fixture.rect(second).to_logical().size,
            Vec2::new(800.0, 500.0)
        );
        assert_eq!(fixture.rect(first).size, Vec2::new(800.0, 600.0));
        let valid = *fixture.0.world().get::<LayoutContext>(viewport).unwrap();
        for invalid in [
            LayoutContext {
                scale_factor: 0.0,
                ..valid
            },
            LayoutContext {
                scale_factor: f32::NAN,
                ..valid
            },
            LayoutContext {
                physical_size: Vec2::splat(f32::INFINITY),
                ..valid
            },
        ] {
            fixture.0.world_mut().entity_mut(viewport).insert(invalid);
            fixture.0.update();
            assert_eq!(fixture.rect(second).size, Vec2::new(1600.0, 1000.0));
            assert_eq!(
                fixture.rect(second).to_logical().size,
                Vec2::new(800.0, 500.0)
            );
        }
    }

    #[test]
    fn reparenting_refreshes_subtree_dpi_and_releases_the_old_viewport() {
        let mut fixture = Harness::new();
        let source = fixture.root(
            Vec2::splat(1000.0),
            2.0,
            Node {
                width: Val::Px(100.0),
                height: Val::Px(50.0),
                ..Default::default()
            },
        );
        let child = fixture
            .0
            .world_mut()
            .spawn((
                Node {
                    width: Val::Px(20.0),
                    height: Val::Px(10.0),
                    ..Default::default()
                },
                ChildOf(source),
            ))
            .id();
        let first = fixture.root(Vec2::splat(1000.0), 1.0, Harness::fill());
        let second = fixture.root(Vec2::splat(1000.0), 3.0, Harness::fill());
        fixture.0.update();
        let count = fixture.0.world().resource::<FlexTree>().node_count();
        assert_eq!(fixture.rect(child).size, Vec2::new(40.0, 20.0));
        for (parent, scale) in [(first, 1.0), (second, 3.0)] {
            fixture
                .0
                .world_mut()
                .entity_mut(source)
                .insert(ChildOf(parent));
            fixture.0.update();
            assert_eq!(fixture.rect(child).size, Vec2::new(20.0, 10.0) * scale);
            assert_eq!(fixture.rect(child).to_logical().size, Vec2::new(20.0, 10.0));
            assert_eq!(
                fixture.0.world().resource::<FlexTree>().node_count(),
                count - 1
            );
            assert!(
                fixture
                    .0
                    .world()
                    .resource::<FlexTree>()
                    .context(source)
                    .is_none()
            );
        }
        fixture.0.world_mut().entity_mut(source).remove::<ChildOf>();
        fixture.0.update();
        assert_eq!(fixture.rect(child).size, Vec2::new(40.0, 20.0));
        fixture.0.world_mut().entity_mut(source).despawn();
        fixture.0.update();
        assert_eq!(
            fixture.0.world().resource::<FlexTree>().node_count(),
            count - 3
        );
    }

    #[test]
    fn explicit_context_supports_a_non_layout_lifecycle_parent() {
        let mut fixture = Harness::new();
        let owner = fixture.0.world_mut().spawn(Name::new("Application")).id();
        let root = fixture.root(Vec2::new(640.0, 480.0), 2.0, Harness::fill());
        fixture
            .0
            .world_mut()
            .entity_mut(root)
            .insert(ChildOf(owner));
        fixture.0.update();
        assert_eq!(
            fixture.rect(root).to_logical().size,
            Vec2::new(320.0, 240.0)
        );
    }

    #[test]
    fn resuming_a_paused_viewport_adopts_style_and_hierarchy_changes() {
        let mut fixture = Harness::new();
        let root = fixture.root(Vec2::splat(400.0), 1.0, Harness::fill());
        let child = fixture
            .0
            .world_mut()
            .spawn((
                Node {
                    width: Val::Px(30.0),
                    height: Val::Px(10.0),
                    ..Default::default()
                },
                ChildOf(root),
            ))
            .id();
        fixture.0.update();
        fixture
            .0
            .world_mut()
            .get_mut::<LayoutContext>(root)
            .unwrap()
            .scale_factor = 0.0;
        fixture.0.world_mut().get_mut::<Node>(child).unwrap().width = Val::Px(70.0);
        let added = fixture
            .0
            .world_mut()
            .spawn((
                Node {
                    width: Val::Px(20.0),
                    height: Val::Px(10.0),
                    ..Default::default()
                },
                ChildOf(root),
            ))
            .id();
        fixture.0.update();
        assert_eq!(fixture.rect(child).size.x, 30.0);
        fixture
            .0
            .world_mut()
            .get_mut::<LayoutContext>(root)
            .unwrap()
            .scale_factor = 1.0;
        fixture.0.update();
        assert_eq!(fixture.rect(child).size.x, 70.0);
        assert_eq!(fixture.rect(added).size.x, 20.0);
        assert_eq!(fixture.rect(added).min().x, 70.0);
    }

    #[test]
    fn ambiguous_fallback_pauses_and_explicit_missing_viewports_do_not_fall_back() {
        let mut fixture = Harness::new();
        let first = fixture
            .0
            .world_mut()
            .spawn((
                PrimaryViewport,
                LayoutContext {
                    physical_size: Vec2::splat(500.0),
                    scale_factor: 1.0,
                },
            ))
            .id();
        let root = fixture.0.world_mut().spawn(Harness::fill()).id();
        fixture.0.update();
        fixture.0.world_mut().spawn((
            PrimaryViewport,
            LayoutContext {
                physical_size: Vec2::splat(1000.0),
                scale_factor: 1.0,
            },
        ));
        fixture.0.update();
        assert_eq!(fixture.rect(root).size, Vec2::splat(500.0));
        fixture.0.world_mut().entity_mut(first).despawn();
        fixture.0.update();
        assert_eq!(fixture.rect(root).size, Vec2::splat(1000.0));
        fixture
            .0
            .world_mut()
            .entity_mut(root)
            .insert(FlexViewport(first));
        fixture.0.world_mut().get_mut::<Node>(root).unwrap().width = Val::Px(10.0);
        fixture.0.update();
        assert_eq!(fixture.rect(root).size, Vec2::splat(1000.0));
    }
}
