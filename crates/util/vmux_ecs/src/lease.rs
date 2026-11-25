use std::any::TypeId;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::prelude::*;

pub struct LeasePlugin<K: Clone + Eq + Hash + Send + Sync + 'static>(PhantomData<fn() -> K>);

impl<K: Clone + Eq + Hash + Send + Sync + 'static> Default for LeasePlugin<K> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<K: Clone + Eq + Hash + Send + Sync + 'static> Plugin for LeasePlugin<K> {
    fn build(&self, app: &mut App) {
        let set = LeaseSet::of::<K>();
        app.init_resource::<LeaseClock<K>>()
            .add_observer(stamp::<K>)
            .add_observer(discard_ticket::<K>)
            .add_observer(release::<K>)
            .add_systems(Update, (arbitrate::<K>, ApplyDeferred).chain().in_set(set))
            .add_systems(
                PostUpdate,
                (arbitrate::<K>, ApplyDeferred).chain().in_set(set),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LeaseSet(TypeId);

impl LeaseSet {
    pub fn of<K: Send + Sync + 'static>() -> Self {
        Self(TypeId::of::<K>())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LeaseMode {
    Exclusive,
    Shared,
}

impl LeaseMode {
    fn select_grants(
        has_exclusive: bool,
        mut shared_count: usize,
        waiters: &mut [(u64, Entity, Self)],
    ) -> Vec<(Entity, Self)> {
        if has_exclusive {
            return Vec::new();
        }
        waiters.sort_unstable_by_key(|(order, entity, _)| (*order, *entity));
        let mut grants = Vec::new();
        for &(_, entity, mode) in waiters.iter() {
            match mode {
                Self::Exclusive => {
                    if shared_count == 0 {
                        grants.push((entity, Self::Exclusive));
                    }
                    break;
                }
                Self::Shared => {
                    shared_count += 1;
                    grants.push((entity, Self::Shared));
                }
            }
        }
        grants
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct LeaseRequest<K: Send + Sync + 'static> {
    pub key: K,
    pub mode: LeaseMode,
}

impl<K: Send + Sync + 'static> LeaseRequest<K> {
    pub const fn new(key: K, mode: LeaseMode) -> Self {
        Self { key, mode }
    }

    pub const fn exclusive(key: K) -> Self {
        Self {
            key,
            mode: LeaseMode::Exclusive,
        }
    }

    pub const fn shared(key: K) -> Self {
        Self {
            key,
            mode: LeaseMode::Shared,
        }
    }

    pub const fn key(&self) -> &K {
        &self.key
    }

    pub const fn mode(&self) -> LeaseMode {
        self.mode
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct LeaseGranted<K: Send + Sync + 'static> {
    pub key: K,
    pub mode: LeaseMode,
}

impl<K: Send + Sync + 'static> LeaseGranted<K> {
    pub const fn key(&self) -> &K {
        &self.key
    }

    pub const fn mode(&self) -> LeaseMode {
        self.mode
    }
}

#[derive(Resource)]
struct LeaseClock<K: Send + Sync + 'static> {
    next: u64,
    marker: PhantomData<fn() -> K>,
}

impl<K: Send + Sync + 'static> Default for LeaseClock<K> {
    fn default() -> Self {
        Self {
            next: 1,
            marker: PhantomData,
        }
    }
}

impl<K: Send + Sync + 'static> LeaseClock<K> {
    fn next(&mut self) -> u64 {
        let value = self.next;
        self.next = self.next.saturating_add(1);
        value
    }
}

#[derive(Component)]
struct LeaseTicket<K: Send + Sync + 'static> {
    sequence: u64,
    marker: PhantomData<fn() -> K>,
}

impl<K: Send + Sync + 'static> LeaseTicket<K> {
    const fn new(sequence: u64) -> Self {
        Self {
            sequence,
            marker: PhantomData,
        }
    }
}

type WaitingRequests<'w, 's, K> = Query<
    'w,
    's,
    (
        Entity,
        &'static LeaseRequest<K>,
        Option<&'static LeaseTicket<K>>,
    ),
    Without<LeaseGranted<K>>,
>;

fn stamp<K: Send + Sync + 'static>(
    trigger: On<Insert, LeaseRequest<K>>,
    mut clock: ResMut<LeaseClock<K>>,
    mut commands: Commands,
) {
    let sequence = clock.next();
    commands
        .entity(trigger.event_target())
        .try_insert(LeaseTicket::<K>::new(sequence));
}

fn discard_ticket<K: Send + Sync + 'static>(
    trigger: On<Remove, LeaseRequest<K>>,
    mut commands: Commands,
) {
    commands
        .entity(trigger.event_target())
        .try_remove::<LeaseTicket<K>>();
}

fn release<K: Clone + Eq + Hash + Send + Sync + 'static>(
    trigger: On<Remove, LeaseGranted<K>>,
    granted: Query<(Entity, &LeaseGranted<K>)>,
    waiting: WaitingRequests<K>,
    mut clock: ResMut<LeaseClock<K>>,
    mut commands: Commands,
) {
    let released = trigger.event_target();
    let Ok((_, removed)) = granted.get(released) else {
        return;
    };
    let key = removed.key.clone();
    let mut has_exclusive = false;
    let mut shared_count = 0usize;
    for (entity, held) in &granted {
        if entity == released || held.key != key {
            continue;
        }
        match held.mode {
            LeaseMode::Exclusive => has_exclusive = true,
            LeaseMode::Shared => shared_count += 1,
        }
    }
    let mut queue = Vec::new();
    for (entity, request, ticket) in &waiting {
        if entity == released || request.key != key {
            continue;
        }
        let order = match ticket {
            Some(ticket) => ticket.sequence,
            None => {
                let sequence = clock.next();
                commands
                    .entity(entity)
                    .try_insert(LeaseTicket::<K>::new(sequence));
                sequence
            }
        };
        queue.push((order, entity, request.mode));
    }
    for (entity, mode) in LeaseMode::select_grants(has_exclusive, shared_count, &mut queue) {
        commands
            .entity(entity)
            .try_remove::<(LeaseRequest<K>, LeaseTicket<K>)>()
            .try_insert(LeaseGranted {
                key: key.clone(),
                mode,
            });
    }
}

fn arbitrate<K: Clone + Eq + Hash + Send + Sync + 'static>(
    granted: Query<&LeaseGranted<K>>,
    waiting: WaitingRequests<K>,
    mut clock: ResMut<LeaseClock<K>>,
    mut commands: Commands,
) {
    if waiting.is_empty() {
        return;
    }
    let mut active = HashMap::<&K, (bool, usize)>::new();
    for held in &granted {
        let entry = active.entry(&held.key).or_insert((false, 0));
        match held.mode {
            LeaseMode::Exclusive => entry.0 = true,
            LeaseMode::Shared => entry.1 += 1,
        }
    }
    let mut queues = HashMap::<&K, Vec<(u64, Entity, LeaseMode)>>::new();
    for (entity, request, ticket) in &waiting {
        let order = match ticket {
            Some(ticket) => ticket.sequence,
            None => {
                let sequence = clock.next();
                commands
                    .entity(entity)
                    .try_insert(LeaseTicket::<K>::new(sequence));
                sequence
            }
        };
        queues
            .entry(&request.key)
            .or_default()
            .push((order, entity, request.mode));
    }
    for (key, mut queue) in queues {
        let (has_exclusive, shared_count) = active.get(key).copied().unwrap_or((false, 0));
        for (entity, mode) in LeaseMode::select_grants(has_exclusive, shared_count, &mut queue) {
            commands
                .entity(entity)
                .try_remove::<(LeaseRequest<K>, LeaseTicket<K>)>()
                .try_insert(LeaseGranted {
                    key: key.clone(),
                    mode,
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    struct ResourceKey(&'static str);

    #[test]
    fn exclusive_and_shared_leases_preserve_fifo_fairness_without_writer_starvation() {
        let mut app = App::new();
        app.add_plugins(LeasePlugin::<ResourceKey>::default());

        let reader_a = app
            .world_mut()
            .spawn(LeaseRequest::shared(ResourceKey("doc")))
            .id();
        let reader_b = app
            .world_mut()
            .spawn(LeaseRequest::shared(ResourceKey("doc")))
            .id();
        let writer = app
            .world_mut()
            .spawn(LeaseRequest::exclusive(ResourceKey("doc")))
            .id();
        let late_reader = app
            .world_mut()
            .spawn(LeaseRequest::shared(ResourceKey("doc")))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<LeaseGranted<ResourceKey>>(reader_a),
            Some(&LeaseGranted {
                key: ResourceKey("doc"),
                mode: LeaseMode::Shared,
            })
        );
        assert_eq!(
            app.world().get::<LeaseGranted<ResourceKey>>(reader_b),
            Some(&LeaseGranted {
                key: ResourceKey("doc"),
                mode: LeaseMode::Shared,
            })
        );
        assert!(
            app.world()
                .get::<LeaseGranted<ResourceKey>>(writer)
                .is_none()
        );
        assert!(
            app.world()
                .get::<LeaseGranted<ResourceKey>>(late_reader)
                .is_none()
        );

        app.world_mut()
            .entity_mut(reader_a)
            .remove::<LeaseGranted<ResourceKey>>();
        assert!(
            app.world()
                .get::<LeaseGranted<ResourceKey>>(writer)
                .is_none()
        );
        assert!(
            app.world()
                .get::<LeaseGranted<ResourceKey>>(late_reader)
                .is_none()
        );

        app.world_mut().despawn(reader_b);
        assert_eq!(
            app.world().get::<LeaseGranted<ResourceKey>>(writer),
            Some(&LeaseGranted {
                key: ResourceKey("doc"),
                mode: LeaseMode::Exclusive,
            })
        );
        assert!(
            app.world()
                .get::<LeaseGranted<ResourceKey>>(late_reader)
                .is_none()
        );

        app.world_mut()
            .entity_mut(writer)
            .remove::<LeaseGranted<ResourceKey>>();
        assert_eq!(
            app.world().get::<LeaseGranted<ResourceKey>>(late_reader),
            Some(&LeaseGranted {
                key: ResourceKey("doc"),
                mode: LeaseMode::Shared,
            })
        );
    }

    #[test]
    fn distinct_keys_arbitrate_independently_and_withdrawn_waiters_do_not_block() {
        let mut app = App::new();
        app.add_plugins(LeasePlugin::<ResourceKey>::default());

        let alpha = app
            .world_mut()
            .spawn(LeaseRequest::exclusive(ResourceKey("alpha")))
            .id();
        let beta = app
            .world_mut()
            .spawn(LeaseRequest::exclusive(ResourceKey("beta")))
            .id();
        let withdrawn = app
            .world_mut()
            .spawn(LeaseRequest::exclusive(ResourceKey("alpha")))
            .id();
        let next_alpha = app
            .world_mut()
            .spawn(LeaseRequest::exclusive(ResourceKey("alpha")))
            .id();

        app.update();

        assert!(
            app.world()
                .get::<LeaseGranted<ResourceKey>>(alpha)
                .is_some()
        );
        assert!(app.world().get::<LeaseGranted<ResourceKey>>(beta).is_some());
        assert!(
            app.world()
                .get::<LeaseGranted<ResourceKey>>(withdrawn)
                .is_none()
        );
        assert!(
            app.world()
                .get::<LeaseGranted<ResourceKey>>(next_alpha)
                .is_none()
        );

        app.world_mut()
            .entity_mut(withdrawn)
            .remove::<LeaseRequest<ResourceKey>>();
        assert!(
            app.world()
                .get::<LeaseTicket<ResourceKey>>(withdrawn)
                .is_none()
        );

        app.world_mut().despawn(alpha);
        assert_eq!(
            app.world().get::<LeaseGranted<ResourceKey>>(next_alpha),
            Some(&LeaseGranted {
                key: ResourceKey("alpha"),
                mode: LeaseMode::Exclusive,
            })
        );
    }
}
