use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use vmux_ecs::types::AppExt;

#[cfg(feature = "geometry")]
pub use geometry::{PANE_GAP_PX, PaneSplitGaps};
pub use hierarchy::Hierarchy;
#[cfg(feature = "store")]
pub use store::{Schema as PaneStore, StorePlugin};

#[vmux_ecs::types(Pane, PaneId, PaneSplit, PaneSplitDirection, PaneSize, SpawnSeq)]
pub struct PanePlugin;

impl Plugin for PanePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::PrimitivesPlugin>() {
            app.add_plugins(vmux_ecs::PrimitivesPlugin);
        }
        app.register_types::<Self>()
            .add_message::<SelectRequest>()
            .add_message::<SelectionChanged>()
            .configure_sets(Update, (PaneSet::Select, PaneSet::AfterSelect).chain())
            .add_systems(Update, select.in_set(PaneSet::Select));
    }
}

#[cfg(feature = "geometry")]
mod geometry;
mod hierarchy;
#[cfg(feature = "store")]
mod store;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
pub struct Pane;

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq, Hash)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
pub struct PaneId(pub String);

#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
pub struct SpawnSeq(pub u64);

#[derive(Component, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
pub struct PaneSplit {
    pub direction: PaneSplitDirection,
}

#[derive(Reflect, Clone, Copy, PartialEq, Eq, Default, Debug)]
#[type_path = "vmux_desktop::layout::pane"]
pub enum PaneSplitDirection {
    #[default]
    Row,
    Column,
}

#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
pub struct PaneSize {
    pub flex_grow: f32,
}

impl Default for PaneSize {
    fn default() -> Self {
        Self { flex_grow: 1.0 }
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaneSet {
    Select,
    AfterSelect,
}

#[derive(Message, Clone, Copy)]
pub struct SelectRequest {
    pub root: Entity,
    pub pane: Entity,
}

#[derive(Message, Clone, Copy)]
pub struct SelectionChanged {
    pub root: Entity,
    pub pane: Entity,
}

fn select(
    mut requests: MessageReader<SelectRequest>,
    hierarchy: Hierarchy,
    mut accepted: MessageWriter<SelectionChanged>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !hierarchy.is_leaf(request.pane) || !hierarchy.contains(request.root, request.pane) {
            continue;
        }
        commands.trigger(vmux_ecs::ActivateRequest {
            entity: request.pane,
        });
        accepted.write(SelectionChanged {
            root: request.root,
            pane: request.pane,
        });
    }
}

#[cfg(feature = "contracts")]
impl From<vmux_api::open_target::PaneDirection> for PaneSplitDirection {
    fn from(direction: vmux_api::open_target::PaneDirection) -> Self {
        match direction {
            vmux_api::open_target::PaneDirection::Left
            | vmux_api::open_target::PaneDirection::Right => Self::Row,
            vmux_api::open_target::PaneDirection::Top
            | vmux_api::open_target::PaneDirection::Bottom => Self::Column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::SystemState;

    struct Fixture {
        app: App,
        root: Entity,
        other: Entity,
        split: Entity,
        first: Entity,
        second: Entity,
        foreign: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(PanePlugin);
            let root = app.world_mut().spawn(Name::new("First root")).id();
            let other = app.world_mut().spawn(Name::new("Second root")).id();
            let split = app
                .world_mut()
                .spawn((
                    Pane,
                    PaneSplit {
                        direction: PaneSplitDirection::Row,
                    },
                    ChildOf(root),
                ))
                .id();
            let first = app
                .world_mut()
                .spawn((Pane, PaneId("first".into()), ChildOf(split)))
                .id();
            let second = app
                .world_mut()
                .spawn((Pane, PaneId("second".into()), ChildOf(split)))
                .id();
            let foreign = app
                .world_mut()
                .spawn((Pane, PaneId("first".into()), ChildOf(other)))
                .id();
            Self {
                app,
                root,
                other,
                split,
                first,
                second,
                foreign,
            }
        }

        fn select(&mut self, root: Entity, pane: Entity) {
            self.app
                .world_mut()
                .write_message(SelectRequest { root, pane });
            self.app.update();
        }
    }

    #[test]
    fn selection_activates_only_a_current_leaf_inside_its_root() {
        let mut fixture = Fixture::new();
        fixture.select(fixture.root, fixture.foreign);
        fixture.select(fixture.root, fixture.split);
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::LastActivatedAt>(fixture.foreign)
                .is_none()
        );
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<SelectionChanged>>()
                .drain()
                .next()
                .is_none()
        );
        fixture.select(fixture.root, fixture.second);
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::LastActivatedAt>(fixture.second)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::LastActivatedAt>(fixture.first)
                .is_none()
        );
        let selected: Vec<_> = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<SelectionChanged>>()
            .drain()
            .collect();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].pane, fixture.second);
    }

    #[test]
    fn reparenting_before_selection_refuses_the_stale_owner() {
        let mut fixture = Fixture::new();
        fixture.app.world_mut().write_message(SelectRequest {
            root: fixture.root,
            pane: fixture.first,
        });
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.first)
            .insert(ChildOf(fixture.other));
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::LastActivatedAt>(fixture.first)
                .is_none()
        );
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<SelectionChanged>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn neighbors_do_not_escape_a_scope_or_target_non_pane_children() {
        let mut fixture = Fixture::new();
        let ignored = fixture
            .app
            .world_mut()
            .spawn((Name::new("Metadata"), ChildOf(fixture.split)))
            .id();
        let mut state = SystemState::<Hierarchy>::new(fixture.app.world_mut());
        let hierarchy = state.get(fixture.app.world()).unwrap();
        assert_eq!(
            hierarchy.neighbor_in(fixture.root, fixture.first, PaneSplitDirection::Row, true),
            Some(fixture.second)
        );
        assert_eq!(
            hierarchy.neighbor_in(fixture.root, fixture.second, PaneSplitDirection::Row, true),
            None
        );
        assert_eq!(
            hierarchy.neighbor_in(fixture.first, fixture.first, PaneSplitDirection::Row, true),
            None
        );
        assert_eq!(hierarchy.first_leaf_in(fixture.other, fixture.split), None);
        assert_eq!(
            hierarchy.first_leaf_in(fixture.root, fixture.split),
            Some(fixture.first)
        );
        assert!(!hierarchy.is_leaf(ignored));
    }
}
