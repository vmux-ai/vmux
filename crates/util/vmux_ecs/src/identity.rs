use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::marker::PhantomData;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

pub struct IdentityIndexPlugin<I: Component, M: Component = I>(PhantomData<fn() -> (I, M)>);

impl<I: Component, M: Component> Default for IdentityIndexPlugin<I, M> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<I: Component + Clone + Eq + Hash, M: Component> Plugin for IdentityIndexPlugin<I, M> {
    fn build(&self, app: &mut App) {
        let set = IdentityIndexSet::of::<I, M>();
        app.init_resource::<IdentityIndex<I, M>>()
            .init_resource::<IndexedIdentities<I, M>>()
            .add_systems(Update, (ApplyDeferred, sync::<I, M>).chain().in_set(set))
            .add_systems(
                PostUpdate,
                (ApplyDeferred, sync::<I, M>).chain().in_set(set),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IdentityIndexSet(TypeId, TypeId);

impl IdentityIndexSet {
    pub fn of<I: Component, M: Component>() -> Self {
        Self(TypeId::of::<I>(), TypeId::of::<M>())
    }
}

#[derive(Resource)]
pub struct IdentityIndex<I: Component, M: Component = I> {
    identities: IdentityMap<I>,
    scoped: IdentityMap<(Entity, I)>,
    marker: PhantomData<fn() -> M>,
}

impl<I: Component, M: Component> Default for IdentityIndex<I, M> {
    fn default() -> Self {
        Self {
            identities: IdentityMap::default(),
            scoped: IdentityMap::default(),
            marker: PhantomData,
        }
    }
}

impl<I: Component + Clone + Eq + Hash, M: Component> IdentityIndex<I, M> {
    pub fn entity(&self, id: &I) -> Result<Option<Entity>, IdentityLookupError<I>> {
        self.identities.entity(id)
    }

    pub fn entity_in(
        &self,
        owner: Entity,
        id: &I,
    ) -> Result<Option<Entity>, IdentityLookupError<I>> {
        self.scoped.entity(&(owner, id.clone())).map_err(|error| match error {
            IdentityLookupError::Duplicate((_, id)) => IdentityLookupError::Duplicate(id),
        })
    }

    fn include(&mut self, id: I, entity: Entity, owner: Option<Entity>) {
        if let Some(owner) = owner {
            self.scoped.include((owner, id.clone()), entity);
        }
        self.identities.include(id, entity);
    }
}

pub(crate) struct IdentityMap<I> {
    entities: HashMap<I, Entity>,
    duplicates: HashSet<I>,
}

impl<I> Default for IdentityMap<I> {
    fn default() -> Self {
        Self {
            entities: HashMap::new(),
            duplicates: HashSet::new(),
        }
    }
}

impl<I: Clone + Eq + Hash> IdentityMap<I> {
    pub(crate) fn entity(&self, id: &I) -> Result<Option<Entity>, IdentityLookupError<I>> {
        if self.duplicates.contains(id) {
            return Err(IdentityLookupError::Duplicate(id.clone()));
        }
        Ok(self.entities.get(id).copied())
    }

    pub(crate) fn include(&mut self, id: I, entity: Entity) {
        if self.entities.insert(id.clone(), entity).is_some() {
            self.duplicates.insert(id);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityLookupError<I> {
    Duplicate(I),
}

impl<I: std::fmt::Debug> std::fmt::Display for IdentityLookupError<I> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate(id) => write!(formatter, "duplicate ID {id:?}"),
        }
    }
}

#[derive(Resource)]
struct IndexedIdentities<I: Component, M: Component> {
    entities: HashSet<Entity>,
    marker: PhantomData<fn() -> (I, M)>,
}

impl<I: Component, M: Component> Default for IndexedIdentities<I, M> {
    fn default() -> Self {
        Self {
            entities: HashSet::new(),
            marker: PhantomData,
        }
    }
}

#[derive(SystemParam)]
pub struct IdentityIndexWriter<'w, 's, I: Component + Clone + Eq + Hash, M: Component = I> {
    entities: Query<'w, 's, (Entity, &'static I, Option<&'static ChildOf>), With<M>>,
    changed: ChangedIdentities<'w, 's, I, M>,
    removed_markers: RemovedComponents<'w, 's, M>,
    removed_ids: RemovedComponents<'w, 's, I>,
    removed_owners: RemovedComponents<'w, 's, ChildOf>,
    current: ResMut<'w, IdentityIndex<I, M>>,
    indexed: ResMut<'w, IndexedIdentities<I, M>>,
}

type ChangedIdentities<'w, 's, I, M> =
    Query<'w, 's, (), (With<M>, Or<(Added<M>, Changed<I>, Changed<ChildOf>)>)>;

impl<I: Component + Clone + Eq + Hash, M: Component> IdentityIndexWriter<'_, '_, I, M> {
    pub fn rebuild(&mut self) {
        let mut changed = !self.changed.is_empty();
        for entity in self.removed_markers.read() {
            if self.indexed.entities.contains(&entity) {
                changed = true;
            }
        }
        for entity in self.removed_ids.read() {
            if self.indexed.entities.contains(&entity) {
                changed = true;
            }
        }
        for entity in self.removed_owners.read() {
            if self.indexed.entities.contains(&entity) {
                changed = true;
            }
        }
        if !changed {
            return;
        }
        let mut next = IdentityIndex::default();
        let mut indexed = HashSet::new();
        for (entity, id, owner) in &self.entities {
            indexed.insert(entity);
            next.include(id.clone(), entity, owner.map(ChildOf::parent));
        }
        *self.current = next;
        self.indexed.entities = indexed;
    }
}

fn sync<I: Component + Clone + Eq + Hash, M: Component>(mut index: IdentityIndexWriter<I, M>) {
    index.rebuild();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
    struct Key(&'static str);

    #[derive(Component)]
    struct Primary;

    #[derive(Component)]
    struct Secondary;

    #[test]
    fn changed_removed_and_duplicate_ids_reconcile_without_stale_lookups() {
        let mut app = App::new();
        app.add_plugins(IdentityIndexPlugin::<Key, Primary>::default());
        let first = app.world_mut().spawn((Primary, Key("first"))).id();
        app.update();
        assert_eq!(
            app.world()
                .resource::<IdentityIndex<Key, Primary>>()
                .entity(&Key("first")),
            Ok(Some(first))
        );

        app.world_mut().entity_mut(first).insert(Key("renamed"));
        app.update();
        let index = app.world().resource::<IdentityIndex<Key, Primary>>();
        assert_eq!(index.entity(&Key("first")), Ok(None));
        assert_eq!(index.entity(&Key("renamed")), Ok(Some(first)));

        let duplicate = app.world_mut().spawn((Primary, Key("renamed"))).id();
        app.update();
        assert_eq!(
            app.world()
                .resource::<IdentityIndex<Key, Primary>>()
                .entity(&Key("renamed")),
            Err(IdentityLookupError::Duplicate(Key("renamed")))
        );

        app.world_mut().entity_mut(duplicate).remove::<Key>();
        app.update();
        assert_eq!(
            app.world()
                .resource::<IdentityIndex<Key, Primary>>()
                .entity(&Key("renamed")),
            Ok(Some(first))
        );

        app.world_mut().entity_mut(first).remove::<Primary>();
        app.update();
        assert_eq!(
            app.world()
                .resource::<IdentityIndex<Key, Primary>>()
                .entity(&Key("renamed")),
            Ok(None)
        );

        app.world_mut().entity_mut(first).insert(Primary);
        app.update();
        assert_eq!(
            app.world()
                .resource::<IdentityIndex<Key, Primary>>()
                .entity(&Key("renamed")),
            Ok(Some(first))
        );
        app.world_mut().despawn(first);
        app.update();
        assert_eq!(
            app.world()
                .resource::<IdentityIndex<Key, Primary>>()
                .entity(&Key("renamed")),
            Ok(None)
        );
    }

    #[test]
    fn owner_scopes_reconcile_reparenting_and_local_duplicates() {
        let mut app = App::new();
        app.add_plugins(IdentityIndexPlugin::<Key, Primary>::default());
        let first_root = app.world_mut().spawn_empty().id();
        let second_root = app.world_mut().spawn_empty().id();
        let first = app
            .world_mut()
            .spawn((Primary, Key("shared"), ChildOf(first_root)))
            .id();
        let second = app
            .world_mut()
            .spawn((Primary, Key("shared"), ChildOf(second_root)))
            .id();
        app.update();
        let index = app.world().resource::<IdentityIndex<Key, Primary>>();
        assert_eq!(index.entity_in(first_root, &Key("shared")), Ok(Some(first)));
        assert_eq!(
            index.entity_in(second_root, &Key("shared")),
            Ok(Some(second))
        );
        assert_eq!(
            index.entity(&Key("shared")),
            Err(IdentityLookupError::Duplicate(Key("shared")))
        );
        app.world_mut()
            .entity_mut(first)
            .insert(ChildOf(second_root));
        app.update();
        let index = app.world().resource::<IdentityIndex<Key, Primary>>();
        assert_eq!(index.entity_in(first_root, &Key("shared")), Ok(None));
        assert_eq!(
            index.entity_in(second_root, &Key("shared")),
            Err(IdentityLookupError::Duplicate(Key("shared")))
        );
        app.world_mut().entity_mut(first).remove::<ChildOf>();
        app.update();
        let index = app.world().resource::<IdentityIndex<Key, Primary>>();
        assert_eq!(
            index.entity_in(second_root, &Key("shared")),
            Ok(Some(second))
        );
        app.world_mut().entity_mut(second).remove::<Primary>();
        app.update();
        assert_eq!(
            app.world()
                .resource::<IdentityIndex<Key, Primary>>()
                .entity_in(second_root, &Key("shared")),
            Ok(None)
        );
    }

    #[test]
    fn markers_keep_indexes_for_the_same_id_type_independent() {
        let mut app = App::new();
        app.add_plugins((
            IdentityIndexPlugin::<Key, Primary>::default(),
            IdentityIndexPlugin::<Key, Secondary>::default(),
        ));
        let primary = app.world_mut().spawn((Primary, Key("shared"))).id();
        let secondary = app.world_mut().spawn((Secondary, Key("shared"))).id();
        app.update();
        assert_eq!(
            app.world()
                .resource::<IdentityIndex<Key, Primary>>()
                .entity(&Key("shared")),
            Ok(Some(primary))
        );
        assert_eq!(
            app.world()
                .resource::<IdentityIndex<Key, Secondary>>()
                .entity(&Key("shared")),
            Ok(Some(secondary))
        );
        app.world_mut().entity_mut(primary).remove::<Primary>();
        app.update();
        assert_eq!(
            app.world()
                .resource::<IdentityIndex<Key, Primary>>()
                .entity(&Key("shared")),
            Ok(None)
        );
        assert_eq!(
            app.world()
                .resource::<IdentityIndex<Key, Secondary>>()
                .entity(&Key("shared")),
            Ok(Some(secondary))
        );
    }
}
