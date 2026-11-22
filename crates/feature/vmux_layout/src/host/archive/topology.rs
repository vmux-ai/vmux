use bevy::ecs::system::{SystemParam, SystemState};
use bevy::prelude::*;
use moonshine_save::prelude::Save;
use vmux_ecs::store::StoreAppExt;
use vmux_ecs::{ArchivedPage, ArchivedPagePosition, ArchivedTabPage, CreatedAt, Order, SplitAxis};

pub(super) struct TopologyPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct TopologySet;

impl Plugin for TopologyPlugin {
    fn build(&self, app: &mut App) {
        app.register_stored::<Root>()
            .register_stored::<TabGroup>()
            .register_stored::<Origin>()
            .register_stored::<TabDirectory>()
            .register_stored::<Id>()
            .register_stored::<Split>()
            .register_stored::<Leaf>()
            .register_stored::<Slot>()
            .register_stored::<Weight>()
            .register_stored::<Materialized>()
            .register_stored::<InvalidTopology>()
            .add_systems(
                Update,
                materialize
                    .in_set(TopologySet)
                    .after(super::CaptureEarlySet)
                    .after(super::CaptureLateSet)
                    .before(super::restore::RestoreAdmissionSet),
            );
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_layout::archive"]
pub(super) struct Root;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_layout::archive"]
pub(super) struct TabGroup;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_layout::archive"]
pub(super) struct Origin {
    pub(super) space_id: String,
    pub(super) tab_index: Option<usize>,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_layout::archive"]
pub(super) struct TabDirectory(pub(super) Option<String>);

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_layout::archive"]
pub(super) struct Id(pub(super) String);

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_layout::archive"]
pub(super) struct Split(pub(super) crate::pane::PaneSplitDirection);

impl Split {
    pub(super) fn new(axis: SplitAxis) -> Self {
        match axis {
            SplitAxis::Row => Self(crate::pane::PaneSplitDirection::Row),
            SplitAxis::Column => Self(crate::pane::PaneSplitDirection::Column),
        }
    }

    pub(super) fn axis(&self) -> SplitAxis {
        match self.0 {
            crate::pane::PaneSplitDirection::Row => SplitAxis::Row,
            crate::pane::PaneSplitDirection::Column => SplitAxis::Column,
        }
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_layout::archive"]
pub(super) struct Leaf;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_layout::archive"]
pub(super) struct Slot;

#[derive(Component, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_layout::archive"]
pub(super) struct Weight(pub(super) f32);

impl Default for Weight {
    fn default() -> Self {
        Self(1.0)
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_layout::archive"]
pub(super) struct Materialized;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_layout::archive"]
pub(super) struct InvalidTopology(pub(super) String);

#[derive(SystemParam)]
struct Materializer<'w, 's> {
    pages: Query<
        'w,
        's,
        (
            Entity,
            &'static ArchivedPage,
            Option<&'static ArchivedTabPage>,
            Option<&'static ArchivedPagePosition>,
        ),
        (
            With<ArchivedPage>,
            Without<Materialized>,
            Without<InvalidTopology>,
        ),
    >,
}

fn materialize(world: &mut World) {
    let mut state = SystemState::<Materializer>::new(world);
    let pending = state
        .get(world)
        .pages
        .iter()
        .map(|(page, metadata, tab, position)| {
            (
                page,
                metadata.clone(),
                tab.cloned(),
                position.cloned().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    for (page, metadata, tab, position) in pending {
        Materializer::materialize_page(world, page, metadata, tab, position);
        world.flush();
    }
}

impl Root {
    pub(super) const CAPACITY: usize = 25;
    pub(super) const TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

    pub(super) fn validate_position(position: &ArchivedPagePosition) -> Result<(), String> {
        if position.stack_index > u32::MAX as usize {
            return Err("archive stack order exceeds supported range".into());
        }
        if position.pane_path.is_empty() {
            return Ok(());
        }
        if position.leaf_pane_id.is_empty() {
            return Err("archive leaf identity is missing".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for step in &position.pane_path {
            if step.split_id.is_empty() || !seen.insert(step.split_id.as_str()) {
                return Err("archive split path is cyclic or missing identity".into());
            }
            if step.child_index >= step.flex_weights.len()
                || step
                    .flex_weights
                    .iter()
                    .any(|weight| !weight.is_finite() || *weight <= 0.0)
            {
                return Err("archive split order or weight is invalid".into());
            }
        }
        if seen.contains(position.leaf_pane_id.as_str()) {
            return Err("archive leaf is also a split".into());
        }
        Ok(())
    }
}

impl Materializer<'_, '_> {
    fn materialize_page(
        world: &mut World,
        page: Entity,
        metadata: ArchivedPage,
        tab: Option<ArchivedTabPage>,
        position: ArchivedPagePosition,
    ) {
        let group_id = tab
            .as_ref()
            .map(|tab| tab.group_id.as_str())
            .filter(|id| !id.is_empty());
        let mut roots = world.query_filtered::<(Entity, &Id), With<Root>>();
        let existing = group_id.and_then(|group| {
            roots
                .iter(world)
                .find_map(|(root, id)| (id.0 == group).then_some(root))
        });
        let root = match existing {
            Some(root) => root,
            None => {
                let root = world
                    .spawn((
                        Root,
                        Id(group_id
                            .map(str::to_owned)
                            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())),
                        Name::new(
                            tab.as_ref()
                                .map(|tab| tab.tab_name.clone())
                                .unwrap_or_else(|| metadata.title.clone()),
                        ),
                        CreatedAt(metadata.closed_at),
                        Origin {
                            space_id: metadata.space_id.clone(),
                            tab_index: metadata.tab_index,
                        },
                        Materialized,
                    ))
                    .id();
                if let Some(tab) = &tab {
                    world
                        .entity_mut(root)
                        .insert((TabGroup, TabDirectory(tab.tab_startup_dir.clone())));
                }
                root
            }
        };
        let validation = Root::validate_position(&position)
            .and_then(|()| Self::validate_membership(world, root, &metadata, tab.as_ref()))
            .and_then(|()| Self::validate_existing(world, root, &position));
        if let Err(error) = validation {
            world
                .entity_mut(root)
                .insert(InvalidTopology(error.clone()));
            world
                .entity_mut(page)
                .insert((ChildOf(root), InvalidTopology(error)));
            return;
        }
        let leaf = match Self::materialize_position(world, root, &position) {
            Ok(leaf) => leaf,
            Err(error) => {
                world
                    .entity_mut(root)
                    .insert(InvalidTopology(error.clone()));
                world
                    .entity_mut(page)
                    .insert((ChildOf(root), InvalidTopology(error)));
                return;
            }
        };
        world.entity_mut(page).insert((
            ChildOf(leaf),
            Order(position.stack_index as u32),
            Materialized,
        ));
        if tab.as_ref().is_none_or(|tab| tab.active) {
            world.entity_mut(page).insert(vmux_ecs::Active);
        }
        world
            .entity_mut(page)
            .remove::<(ArchivedPagePosition, ArchivedTabPage)>();
    }

    fn validate_membership(
        world: &World,
        root: Entity,
        page: &ArchivedPage,
        tab: Option<&ArchivedTabPage>,
    ) -> Result<(), String> {
        let Some(origin) = world.get::<Origin>(root) else {
            return Err("archive origin is missing".into());
        };
        if origin.space_id != page.space_id
            || origin.tab_index != page.tab_index
            || world
                .get::<CreatedAt>(root)
                .is_none_or(|time| time.0 != page.closed_at)
            || world.get::<InvalidTopology>(root).is_some()
        {
            return Err("archive group membership conflicts".into());
        }
        if let Some(tab) = tab
            && (world
                .get::<Name>(root)
                .is_none_or(|name| name.as_str() != tab.tab_name)
                || world
                    .get::<TabDirectory>(root)
                    .is_none_or(|directory| directory.0 != tab.tab_startup_dir))
        {
            return Err("archive tab metadata conflicts".into());
        }
        Ok(())
    }

    fn validate_existing(
        world: &mut World,
        root: Entity,
        position: &ArchivedPagePosition,
    ) -> Result<(), String> {
        let mut parent = Some(root);
        for (depth, step) in position.pane_path.iter().enumerate() {
            let existing = Self::find_id(world, root, &step.split_id);
            if let Some(split) = existing {
                if world
                    .get::<Split>(split)
                    .is_none_or(|split| split.axis() != step.axis)
                    || world
                        .get::<ChildOf>(split)
                        .map(|relation| relation.parent())
                        != parent
                {
                    return Err("archive split has conflicting axis or parent".into());
                }
                if depth > 0 {
                    let previous = &position.pane_path[depth - 1];
                    if world
                        .get::<Order>(split)
                        .is_none_or(|order| order.0 != previous.child_index as u32)
                    {
                        return Err("archive split order conflicts".into());
                    }
                }
                let children = Self::ordered_children(world, split);
                if children.len() != step.flex_weights.len() {
                    return Err("archive split child count conflicts".into());
                }
                for child in children {
                    let Some(order) = world.get::<Order>(child) else {
                        return Err("archive node order is missing".into());
                    };
                    let Some(weight) = step.flex_weights.get(order.0 as usize) else {
                        return Err("archive node order exceeds split".into());
                    };
                    if world
                        .get::<Weight>(child)
                        .is_none_or(|current| current.0 != *weight)
                    {
                        return Err("archive sibling weight conflicts".into());
                    }
                }
            }
            if let Some(parent) = parent {
                let order = if depth == 0 {
                    0
                } else {
                    position.pane_path[depth - 1].child_index as u32
                };
                Self::validate_slot_replacement(world, parent, order, existing)?;
            }
            parent = existing;
        }
        let existing = Self::find_id(world, root, &position.leaf_pane_id);
        if let Some(leaf) = existing {
            if world.get::<Leaf>(leaf).is_none()
                || world.get::<ChildOf>(leaf).map(|relation| relation.parent()) != parent
            {
                return Err("archive leaf has conflicting identity or parent".into());
            }
            let order = position
                .pane_path
                .last()
                .map_or(0, |step| step.child_index as u32);
            if world
                .get::<Order>(leaf)
                .is_none_or(|current| current.0 != order)
            {
                return Err("archive leaf order conflicts".into());
            }
            if Self::ordered_children(world, leaf).iter().any(|page| {
                world
                    .get::<Order>(*page)
                    .is_some_and(|order| order.0 == position.stack_index as u32)
            }) {
                return Err("archive page order conflicts".into());
            }
        }
        if let Some(parent) = parent {
            let order = position
                .pane_path
                .last()
                .map_or(0, |step| step.child_index as u32);
            Self::validate_slot_replacement(world, parent, order, existing)?;
        }
        Ok(())
    }

    fn ordered_children(world: &World, parent: Entity) -> Vec<Entity> {
        let mut children = world
            .get::<Children>(parent)
            .map(|children| children.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        children.sort_by_key(|child| {
            (
                world.get::<Order>(*child).map_or(u32::MAX, |order| order.0),
                child.to_bits(),
            )
        });
        children
    }

    fn materialize_position(
        world: &mut World,
        root: Entity,
        position: &ArchivedPagePosition,
    ) -> Result<Entity, String> {
        let mut parent = root;
        for (depth, step) in position.pane_path.iter().enumerate() {
            let split = Self::find_id(world, root, &step.split_id);
            let split = match split {
                Some(split) => {
                    if world
                        .get::<Split>(split)
                        .is_none_or(|split| split.axis() != step.axis)
                        || world
                            .get::<ChildOf>(split)
                            .is_none_or(|relation| relation.parent() != parent)
                    {
                        return Err("archive split has conflicting axis or parent".into());
                    }
                    split
                }
                None => world
                    .spawn((
                        Id(step.split_id.clone()),
                        Split::new(step.axis),
                        ChildOf(parent),
                        Order(0),
                    ))
                    .id(),
            };
            if depth > 0 {
                let previous = &position.pane_path[depth - 1];
                Self::replace_slot(world, parent, previous.child_index as u32, split);
                world.entity_mut(split).insert((
                    Order(previous.child_index as u32),
                    Weight(previous.flex_weights[previous.child_index]),
                ));
            }
            for (index, weight) in step.flex_weights.iter().enumerate() {
                Self::ensure_slot(world, split, index as u32, *weight)?;
            }
            parent = split;
        }
        let id = if position.leaf_pane_id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            position.leaf_pane_id.clone()
        };
        let leaf = match Self::find_id(world, root, &id) {
            Some(leaf) => {
                if world.get::<Leaf>(leaf).is_none()
                    || world
                        .get::<ChildOf>(leaf)
                        .is_none_or(|relation| relation.parent() != parent)
                {
                    return Err("archive leaf has conflicting identity or parent".into());
                }
                leaf
            }
            None => world
                .spawn((Id(id), Leaf, ChildOf(parent), Order(0), Weight::default()))
                .id(),
        };
        if let Some(step) = position.pane_path.last() {
            Self::replace_slot(world, parent, step.child_index as u32, leaf);
            world.entity_mut(leaf).insert((
                Order(step.child_index as u32),
                Weight(step.flex_weights[step.child_index]),
            ));
        }
        Ok(leaf)
    }

    fn find_id(world: &mut World, root: Entity, id: &str) -> Option<Entity> {
        let mut nodes = world.query::<(Entity, &Id)>();
        let candidates = nodes
            .iter(world)
            .filter_map(|(node, value)| (value.0 == id && node != root).then_some(node))
            .collect::<Vec<_>>();
        for node in candidates {
            let mut current = node;
            for _ in 0..128 {
                let Some(parent) = world.get::<ChildOf>(current) else {
                    break;
                };
                current = parent.parent();
                if current == root {
                    return Some(node);
                }
            }
        }
        None
    }

    fn validate_slot_replacement(
        world: &World,
        parent: Entity,
        order: u32,
        node: Option<Entity>,
    ) -> Result<(), String> {
        for child in Self::ordered_children(world, parent) {
            if world
                .get::<Order>(child)
                .is_some_and(|value| value.0 == order)
                && Some(child) != node
                && world.get::<Slot>(child).is_none()
            {
                return Err("archive siblings have conflicting identity or order".into());
            }
        }
        Ok(())
    }

    fn ensure_slot(
        world: &mut World,
        parent: Entity,
        order: u32,
        weight: f32,
    ) -> Result<(), String> {
        let children = world
            .get::<Children>(parent)
            .map(|children| children.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for child in children {
            if world
                .get::<Order>(child)
                .is_some_and(|value| value.0 == order)
            {
                if world
                    .get::<Weight>(child)
                    .is_some_and(|value| value.0 != weight)
                {
                    return Err("archive sibling weight conflicts".into());
                }
                return Ok(());
            }
        }
        world.spawn((Slot, Order(order), Weight(weight), ChildOf(parent)));
        Ok(())
    }

    fn replace_slot(world: &mut World, parent: Entity, order: u32, node: Entity) {
        let children = world
            .get::<Children>(parent)
            .map(|children| children.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for child in children {
            if child != node
                && world.get::<Slot>(child).is_some()
                && world
                    .get::<Order>(child)
                    .is_some_and(|value| value.0 == order)
            {
                world.despawn(child);
            }
        }
    }
}
