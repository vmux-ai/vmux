use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_api::Id;

pub struct IdentityIndexPlugin<T, M = T>(PhantomData<fn() -> (T, M)>);

impl<T, M> Default for IdentityIndexPlugin<T, M> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send + Sync + 'static, M: Component> Plugin for IdentityIndexPlugin<T, M> {
    fn build(&self, app: &mut App) {
        app.init_resource::<IdentityIndex<T>>()
            .init_resource::<IndexedIdentities<T, M>>();
    }
}

#[derive(Resource)]
pub struct IdentityIndex<T> {
    entities: HashMap<Id<T>, Entity>,
    duplicates: HashSet<Id<T>>,
}

impl<T> Default for IdentityIndex<T> {
    fn default() -> Self {
        Self {
            entities: HashMap::new(),
            duplicates: HashSet::new(),
        }
    }
}

impl<T> IdentityIndex<T> {
    pub fn entity(&self, id: &Id<T>) -> Result<Option<Entity>, IdentityLookupError<T>> {
        if self.duplicates.contains(id) {
            return Err(IdentityLookupError::Duplicate(id.clone()));
        }
        Ok(self.entities.get(id).copied())
    }

    fn include(&mut self, id: Id<T>, entity: Entity) {
        if self.entities.insert(id.clone(), entity).is_some() {
            self.duplicates.insert(id);
        }
    }
}

pub enum IdentityLookupError<T> {
    Duplicate(Id<T>),
}

impl<T> Clone for IdentityLookupError<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Duplicate(id) => Self::Duplicate(id.clone()),
        }
    }
}

impl<T> std::fmt::Debug for IdentityLookupError<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate(id) => formatter.debug_tuple("Duplicate").field(id).finish(),
        }
    }
}

impl<T> PartialEq for IdentityLookupError<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Duplicate(left), Self::Duplicate(right)) => left == right,
        }
    }
}

impl<T> Eq for IdentityLookupError<T> {}

impl<T> std::fmt::Display for IdentityLookupError<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate(id) => write!(formatter, "duplicate ID `{id}`"),
        }
    }
}

#[derive(Resource)]
struct IndexedIdentities<T, M> {
    entities: HashSet<Entity>,
    marker: PhantomData<fn() -> (T, M)>,
}

impl<T, M> Default for IndexedIdentities<T, M> {
    fn default() -> Self {
        Self {
            entities: HashSet::new(),
            marker: PhantomData,
        }
    }
}

#[derive(SystemParam)]
pub struct IdentityIndexWriter<'w, 's, T: Send + Sync + 'static, M: Component = T> {
    entities: Query<'w, 's, (Entity, &'static Id<T>), With<M>>,
    changed: Query<'w, 's, (), (With<M>, Or<(Added<M>, Changed<Id<T>>)>)>,
    removed_markers: RemovedComponents<'w, 's, M>,
    removed_ids: RemovedComponents<'w, 's, Id<T>>,
    current: ResMut<'w, IdentityIndex<T>>,
    indexed: ResMut<'w, IndexedIdentities<T, M>>,
}

impl<T: Send + Sync + 'static, M: Component> IdentityIndexWriter<'_, '_, T, M> {
    pub fn rebuild(&mut self) {
        let removed_marker = self
            .removed_markers
            .read()
            .any(|entity| self.indexed.entities.contains(&entity));
        let removed_id = self
            .removed_ids
            .read()
            .any(|entity| self.indexed.entities.contains(&entity));
        if self.changed.is_empty() && !removed_marker && !removed_id {
            return;
        }

        let mut next = IdentityIndex::default();
        let mut next_indexed = HashSet::new();
        for (entity, id) in &self.entities {
            next_indexed.insert(entity);
            next.include(id.clone(), entity);
        }
        *self.current = next;
        self.indexed.entities = next_indexed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_app::Update;

    #[derive(Component)]
    struct Item;

    fn rebuild(mut index: IdentityIndexWriter<Item>) {
        index.rebuild();
    }

    #[test]
    fn duplicate_and_removed_identities_repair_the_index() {
        let mut app = App::new();
        app.add_plugins(IdentityIndexPlugin::<Item>::default())
            .add_systems(Update, rebuild);
        let id = Id::<Item>::new("item");
        let first = app.world_mut().spawn((Item, id.clone())).id();
        let second = app.world_mut().spawn((Item, id.clone())).id();

        app.update();

        assert_eq!(
            app.world().resource::<IdentityIndex<Item>>().entity(&id),
            Err(IdentityLookupError::Duplicate(id.clone()))
        );

        app.world_mut().entity_mut(second).despawn();
        app.update();

        assert_eq!(
            app.world().resource::<IdentityIndex<Item>>().entity(&id),
            Ok(Some(first))
        );
    }
}
