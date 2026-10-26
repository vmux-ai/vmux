use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use moonshine_save::prelude::Save;
use vmux_api::Id;
use vmux_ecs::persistence::PersistenceAppExt;

use crate::{AccountExternalId, ExternalHost, ExternalNamespace, ProviderKey};

pub(crate) struct ScopePlugin;

impl Plugin for ScopePlugin {
    fn build(&self, app: &mut App) {
        app.register_persisted::<ExternalScope>()
            .register_persisted::<Id<ExternalScope>>()
            .register_persisted::<ProviderKey>()
            .register_persisted::<ExternalHost>()
            .register_persisted::<ExternalNamespace>()
            .register_type::<AccountExternalId>()
            .register_type::<CanAccess>()
            .init_resource::<ExternalScopeIndex>()
            .init_resource::<IndexedScopes>()
            .add_systems(PreUpdate, rebuild);
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_connector"]
pub struct ExternalScope;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct CanAccess(Vec<Id<ExternalScope>>);

impl CanAccess {
    pub fn new(mut scopes: Vec<Id<ExternalScope>>) -> Self {
        scopes.sort();
        scopes.dedup();
        Self(scopes)
    }

    pub fn scopes(&self) -> &[Id<ExternalScope>] {
        &self.0
    }

    pub fn grants(&self, scope: &Id<ExternalScope>) -> bool {
        self.0.binary_search(scope).is_ok()
    }
}

#[derive(Resource, Default)]
pub struct ExternalScopeIndex {
    entities: HashMap<Id<ExternalScope>, Entity>,
    duplicates: HashSet<Id<ExternalScope>>,
}

impl ExternalScopeIndex {
    pub fn entity(
        &self,
        id: &Id<ExternalScope>,
    ) -> Result<Option<Entity>, ExternalScopeLookupError> {
        if self.duplicates.contains(id) {
            return Err(ExternalScopeLookupError::Duplicate(id.clone()));
        }
        Ok(self.entities.get(id).copied())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalScopeLookupError {
    Duplicate(Id<ExternalScope>),
}

impl std::fmt::Display for ExternalScopeLookupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate(id) => write!(formatter, "duplicate external scope ID `{id}`"),
        }
    }
}

#[derive(Resource, Default)]
struct IndexedScopes(HashSet<Entity>);

fn rebuild(
    scopes: Query<(Entity, &Id<ExternalScope>), With<ExternalScope>>,
    changed: Query<(), (With<ExternalScope>, Added<Id<ExternalScope>>)>,
    mut removed_scopes: RemovedComponents<ExternalScope>,
    mut removed_ids: RemovedComponents<Id<ExternalScope>>,
    mut current: ResMut<ExternalScopeIndex>,
    mut indexed: ResMut<IndexedScopes>,
) {
    let removed_scope = removed_scopes
        .read()
        .any(|entity| indexed.0.contains(&entity));
    let removed_id = removed_ids.read().any(|entity| indexed.0.contains(&entity));
    if changed.is_empty() && !removed_scope && !removed_id {
        return;
    }

    let mut next = ExternalScopeIndex::default();
    let mut next_indexed = HashSet::new();
    for (entity, id) in &scopes {
        next_indexed.insert(entity);
        if next.entities.insert(id.clone(), entity).is_some() {
            next.duplicates.insert(id.clone());
        }
    }
    *current = next;
    indexed.0 = next_indexed;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_scope_ids_are_explicit() {
        let mut app = App::new();
        app.add_plugins(ScopePlugin);
        let id = Id::<ExternalScope>::new("github.com/accounts/1");
        app.world_mut().spawn((ExternalScope, id.clone()));
        app.world_mut().spawn((ExternalScope, id.clone()));

        app.update();

        assert_eq!(
            app.world().resource::<ExternalScopeIndex>().entity(&id),
            Err(ExternalScopeLookupError::Duplicate(id))
        );
    }
}
