use std::collections::BTreeMap;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::prelude::*;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};

pub struct OrderPlugin;

impl Plugin for OrderPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Order>()
            .add_observer(move_sibling)
            .add_systems(Update, (ApplyDeferred, normalize).chain().in_set(OrderSet))
            .add_systems(
                PostUpdate,
                (ApplyDeferred, normalize).chain().in_set(OrderSet),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OrderSet;

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq, PartialOrd, Ord)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Order(pub u32);

impl Order {
    pub const END: Self = Self(u32::MAX);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderPosition {
    Index(u32),
    Before(Entity),
    After(Entity),
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReorderRequest {
    #[event_target]
    pub entity: Entity,
    pub position: OrderPosition,
}

impl ReorderRequest {
    pub const fn to_index(entity: Entity, index: u32) -> Self {
        Self {
            entity,
            position: OrderPosition::Index(index),
        }
    }

    pub const fn before(entity: Entity, target: Entity) -> Self {
        Self {
            entity,
            position: OrderPosition::Before(target),
        }
    }

    pub const fn after(entity: Entity, target: Entity) -> Self {
        Self {
            entity,
            position: OrderPosition::After(target),
        }
    }
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reordered {
    #[event_target]
    pub entity: Entity,
    pub order: u32,
}

fn move_sibling(
    trigger: On<ReorderRequest>,
    mut ordered: Query<(Entity, &mut Order, Option<&ChildOf>)>,
    mut commands: Commands,
) {
    let request = *trigger.event();
    let Ok((_, current_order, parent)) = ordered.get(request.entity) else {
        return;
    };
    let previous = current_order.0;
    let parent = parent.map(ChildOf::parent);
    let mut siblings: Vec<(Entity, u32)> = ordered
        .iter()
        .filter(|(entity, _, candidate_parent)| {
            *entity != request.entity && candidate_parent.map(ChildOf::parent) == parent
        })
        .map(|(entity, order, _)| (entity, order.0))
        .collect();
    siblings.sort_by_key(|(entity, order)| (*order, *entity));
    let target_index = match request.position {
        OrderPosition::Index(index) => (index as usize).min(siblings.len()),
        OrderPosition::Before(target) => {
            if target == request.entity {
                return;
            }
            let Some(index) = siblings.iter().position(|(entity, _)| *entity == target) else {
                return;
            };
            index
        }
        OrderPosition::After(target) => {
            if target == request.entity {
                return;
            }
            let Some(index) = siblings.iter().position(|(entity, _)| *entity == target) else {
                return;
            };
            index + 1
        }
    };
    let mut arranged: Vec<Entity> = siblings.into_iter().map(|(entity, _)| entity).collect();
    arranged.insert(target_index, request.entity);
    for (index, entity) in arranged.into_iter().enumerate() {
        let next = index as u32;
        if let Ok((_, mut order, _)) = ordered.get_mut(entity)
            && order.0 != next
        {
            order.0 = next;
        }
    }
    let next = target_index as u32;
    if next != previous {
        commands.trigger(Reordered {
            entity: request.entity,
            order: next,
        });
    }
}

fn normalize(
    mut removed_orders: RemovedComponents<Order>,
    mut removed_parents: RemovedComponents<ChildOf>,
    mut ordered: Query<(Entity, &mut Order, Option<Ref<ChildOf>>)>,
) {
    let mut dirty = removed_orders.read().count() > 0;
    dirty |= removed_parents.read().count() > 0;
    if !dirty {
        for (_, order, parent) in ordered.iter_mut() {
            if order.is_added()
                || order.is_changed()
                || parent.as_ref().is_some_and(|parent| parent.is_changed())
            {
                dirty = true;
                break;
            }
        }
    }
    if !dirty {
        return;
    }
    let mut groups = BTreeMap::<Option<Entity>, Vec<(u32, Entity)>>::new();
    for (entity, order, parent) in &ordered {
        groups
            .entry(parent.as_deref().map(ChildOf::parent))
            .or_default()
            .push((order.0, entity));
    }
    for mut siblings in groups.into_values() {
        siblings.sort();
        for (index, (_, entity)) in siblings.into_iter().enumerate() {
            let next = index as u32;
            if let Ok((_, mut order, _)) = ordered.get_mut(entity)
                && order.0 != next
            {
                order.0 = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Recorded(Vec<(Entity, u32)>);

    fn record(trigger: On<Reordered>, mut recorded: ResMut<Recorded>) {
        let event = trigger.event();
        recorded.0.push((event.entity, event.order));
    }

    #[test]
    fn insert_and_removal_normalize_siblings_per_parent() {
        let mut app = App::new();
        app.add_plugins(OrderPlugin);
        let first_parent = app.world_mut().spawn_empty().id();
        let second_parent = app.world_mut().spawn_empty().id();
        let a = app
            .world_mut()
            .spawn((Order(10), ChildOf(first_parent)))
            .id();
        let b = app
            .world_mut()
            .spawn((Order(5), ChildOf(first_parent)))
            .id();
        let c = app
            .world_mut()
            .spawn((Order::END, ChildOf(first_parent)))
            .id();
        let other = app
            .world_mut()
            .spawn((Order(99), ChildOf(second_parent)))
            .id();

        app.update();

        assert_eq!(app.world().get::<Order>(b), Some(&Order(0)));
        assert_eq!(app.world().get::<Order>(a), Some(&Order(1)));
        assert_eq!(app.world().get::<Order>(c), Some(&Order(2)));
        assert_eq!(app.world().get::<Order>(other), Some(&Order(0)));

        app.world_mut().despawn(a);
        app.update();

        assert_eq!(app.world().get::<Order>(b), Some(&Order(0)));
        assert_eq!(app.world().get::<Order>(c), Some(&Order(1)));
        assert_eq!(app.world().get::<Order>(other), Some(&Order(0)));
    }

    #[test]
    fn reorder_supports_index_before_after_and_ignores_cross_parent_targets() {
        let mut app = App::new();
        app.add_plugins(OrderPlugin)
            .init_resource::<Recorded>()
            .add_observer(record);
        let parent = app.world_mut().spawn_empty().id();
        let foreign_parent = app.world_mut().spawn_empty().id();
        let a = app.world_mut().spawn((Order(0), ChildOf(parent))).id();
        let b = app.world_mut().spawn((Order(1), ChildOf(parent))).id();
        let c = app.world_mut().spawn((Order(2), ChildOf(parent))).id();
        let foreign = app
            .world_mut()
            .spawn((Order(0), ChildOf(foreign_parent)))
            .id();
        app.update();

        app.world_mut().trigger(ReorderRequest::to_index(c, 0));
        app.world_mut().flush();
        assert_eq!(app.world().get::<Order>(c), Some(&Order(0)));
        assert_eq!(app.world().get::<Order>(a), Some(&Order(1)));
        assert_eq!(app.world().get::<Order>(b), Some(&Order(2)));

        app.world_mut().trigger(ReorderRequest::after(c, b));
        app.world_mut().flush();
        assert_eq!(app.world().get::<Order>(a), Some(&Order(0)));
        assert_eq!(app.world().get::<Order>(b), Some(&Order(1)));
        assert_eq!(app.world().get::<Order>(c), Some(&Order(2)));

        app.world_mut().trigger(ReorderRequest::before(b, a));
        app.world_mut().flush();
        assert_eq!(app.world().get::<Order>(b), Some(&Order(0)));
        assert_eq!(app.world().get::<Order>(a), Some(&Order(1)));
        assert_eq!(app.world().get::<Order>(c), Some(&Order(2)));

        app.world_mut().trigger(ReorderRequest::before(a, foreign));
        app.world_mut().flush();
        assert_eq!(app.world().get::<Order>(b), Some(&Order(0)));
        assert_eq!(app.world().get::<Order>(a), Some(&Order(1)));
        assert_eq!(app.world().get::<Order>(c), Some(&Order(2)));
        assert_eq!(
            app.world().resource::<Recorded>().0,
            vec![(c, 0), (c, 2), (b, 0)]
        );
    }

    #[test]
    fn reparenting_normalizes_both_previous_and_next_parents() {
        let mut app = App::new();
        app.add_plugins(OrderPlugin);
        let first = app.world_mut().spawn_empty().id();
        let second = app.world_mut().spawn_empty().id();
        let a = app.world_mut().spawn((Order(0), ChildOf(first))).id();
        let b = app.world_mut().spawn((Order(1), ChildOf(first))).id();
        let c = app.world_mut().spawn((Order(0), ChildOf(second))).id();
        app.update();

        app.world_mut()
            .entity_mut(a)
            .insert((ChildOf(second), Order::END));
        app.update();

        assert_eq!(app.world().get::<Order>(b), Some(&Order(0)));
        assert_eq!(app.world().get::<Order>(c), Some(&Order(0)));
        assert_eq!(app.world().get::<Order>(a), Some(&Order(1)));
    }
}
