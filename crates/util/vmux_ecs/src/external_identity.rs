use std::any::TypeId;
use std::collections::HashSet;
use std::hash::Hash;
use std::marker::PhantomData;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::prelude::*;

use crate::IdentityLookupError;
use crate::identity::IdentityMap;

pub struct ExternalIdentityIndexPlugin<P: Component, I: Component, M: Component>(Scope<P, I, M>);

type Scope<P, I, M> = PhantomData<fn() -> (P, I, M)>;

impl<P: Component, I: Component, M: Component> Default for ExternalIdentityIndexPlugin<P, I, M> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<P: Component + Clone + Eq + Hash, I: Component + Clone + Eq + Hash, M: Component> Plugin
    for ExternalIdentityIndexPlugin<P, I, M>
{
    fn build(&self, app: &mut App) {
        let set = ExternalIdentityIndexSet::of::<P, I, M>();
        app.init_resource::<ExternalIdentityIndex<P, I, M>>()
            .init_resource::<Indexed<P, I, M>>()
            .add_systems(Update, (ApplyDeferred, sync::<P, I, M>).chain().in_set(set))
            .add_systems(
                PostUpdate,
                (ApplyDeferred, sync::<P, I, M>).chain().in_set(set),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExternalIdentityIndexSet(TypeId, TypeId, TypeId);

impl ExternalIdentityIndexSet {
    pub fn of<P: Component, I: Component, M: Component>() -> Self {
        Self(TypeId::of::<P>(), TypeId::of::<I>(), TypeId::of::<M>())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExternalIdentity<P, I> {
    pub provider: P,
    pub id: I,
}

#[derive(Resource)]
pub struct ExternalIdentityIndex<P: Component, I: Component, M: Component> {
    identities: IdentityMap<ExternalIdentity<P, I>>,
    marker: PhantomData<fn() -> M>,
}

impl<P: Component, I: Component, M: Component> Default for ExternalIdentityIndex<P, I, M> {
    fn default() -> Self {
        Self {
            identities: IdentityMap::default(),
            marker: PhantomData,
        }
    }
}

impl<P: Component + Clone + Eq + Hash, I: Component + Clone + Eq + Hash, M: Component>
    ExternalIdentityIndex<P, I, M>
{
    pub fn entity(
        &self,
        provider: &P,
        id: &I,
    ) -> Result<Option<Entity>, IdentityLookupError<ExternalIdentity<P, I>>> {
        let key = ExternalIdentity {
            provider: provider.clone(),
            id: id.clone(),
        };
        self.identities.entity(&key)
    }

    fn include(&mut self, provider: P, id: I, entity: Entity) {
        let key = ExternalIdentity { provider, id };
        self.identities.include(key, entity);
    }
}

#[derive(Resource)]
struct Indexed<P: Component, I: Component, M: Component> {
    entities: HashSet<Entity>,
    marker: Scope<P, I, M>,
}

impl<P: Component, I: Component, M: Component> Default for Indexed<P, I, M> {
    fn default() -> Self {
        Self {
            entities: HashSet::new(),
            marker: PhantomData,
        }
    }
}

type Changes<'w, 's, P, I, M> =
    Query<'w, 's, (), (With<M>, Or<(Added<M>, Changed<P>, Changed<I>)>)>;

fn sync<P: Component + Clone + Eq + Hash, I: Component + Clone + Eq + Hash, M: Component>(
    entities: Query<(Entity, &P, &I), With<M>>,
    changes: Changes<P, I, M>,
    mut providers: RemovedComponents<P>,
    mut ids: RemovedComponents<I>,
    mut markers: RemovedComponents<M>,
    mut current: ResMut<ExternalIdentityIndex<P, I, M>>,
    mut indexed: ResMut<Indexed<P, I, M>>,
) {
    let mut dirty = !changes.is_empty();
    for entity in providers.read().chain(ids.read()).chain(markers.read()) {
        if indexed.entities.contains(&entity) {
            dirty = true;
        }
    }
    if !dirty {
        return;
    }
    let mut next = ExternalIdentityIndex::default();
    let mut known = HashSet::new();
    for (entity, provider, id) in &entities {
        next.include(provider.clone(), id.clone(), entity);
        known.insert(entity);
    }
    *current = next;
    indexed.entities = known;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
    struct Provider(&'static str);

    #[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
    struct Key(&'static str);

    #[derive(Component)]
    struct Primary;

    #[derive(Component)]
    struct Secondary;

    #[test]
    fn providers_and_marker_scopes_keep_the_same_external_id_independent() {
        let mut app = App::new();
        app.add_plugins((
            ExternalIdentityIndexPlugin::<Provider, Key, Primary>::default(),
            ExternalIdentityIndexPlugin::<Provider, Key, Secondary>::default(),
        ));
        let first = app
            .world_mut()
            .spawn((Primary, Provider("one"), Key("shared")))
            .id();
        let other = app
            .world_mut()
            .spawn((Primary, Provider("two"), Key("shared")))
            .id();
        let secondary = app
            .world_mut()
            .spawn((Secondary, Provider("one"), Key("shared")))
            .id();
        app.update();
        let primary = app
            .world()
            .resource::<ExternalIdentityIndex<Provider, Key, Primary>>();
        assert_eq!(
            primary.entity(&Provider("one"), &Key("shared")),
            Ok(Some(first))
        );
        assert_eq!(
            primary.entity(&Provider("two"), &Key("shared")),
            Ok(Some(other))
        );
        let secondary_index = app
            .world()
            .resource::<ExternalIdentityIndex<Provider, Key, Secondary>>();
        assert_eq!(
            secondary_index.entity(&Provider("one"), &Key("shared")),
            Ok(Some(secondary))
        );

        let duplicate = app
            .world_mut()
            .spawn((Primary, Provider("one"), Key("shared")))
            .id();
        app.update();
        assert!(
            app.world()
                .resource::<ExternalIdentityIndex<Provider, Key, Primary>>()
                .entity(&Provider("one"), &Key("shared"))
                .is_err()
        );
        app.world_mut().entity_mut(duplicate).despawn();
        app.world_mut().entity_mut(first).insert(Provider("three"));
        app.update();
        let primary = app
            .world()
            .resource::<ExternalIdentityIndex<Provider, Key, Primary>>();
        assert_eq!(primary.entity(&Provider("one"), &Key("shared")), Ok(None));
        assert_eq!(
            primary.entity(&Provider("three"), &Key("shared")),
            Ok(Some(first))
        );
        app.world_mut().entity_mut(first).remove::<Key>();
        app.world_mut().entity_mut(other).remove::<Primary>();
        app.update();
        let primary = app
            .world()
            .resource::<ExternalIdentityIndex<Provider, Key, Primary>>();
        assert_eq!(primary.entity(&Provider("three"), &Key("shared")), Ok(None));
        assert_eq!(primary.entity(&Provider("two"), &Key("shared")), Ok(None));
    }
}
