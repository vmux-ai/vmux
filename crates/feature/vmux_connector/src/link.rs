use std::collections::{HashMap, HashSet};

use bevy_app::{App, Plugin, PreUpdate};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_api::Id;
#[cfg(host)]
use vmux_ecs::persistence::PersistenceAppExt;

use crate::{CanonicalField, ExternalId, ExternalScope, ProviderKey, ResourceKind};

pub(crate) struct LinkPlugin;

impl Plugin for LinkPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(host)]
        app.register_persisted::<ExternalLink>()
            .register_persisted::<ExternalScopeRef>()
            .register_persisted::<ExternalId>()
            .register_persisted::<ResourceKind>()
            .register_persisted::<SyncPolicy>()
            .register_persisted::<SynchronizedFields>()
            .register_persisted::<PrimaryWorkItem>()
            .register_persisted::<ReviewArtifact>();
        app.init_resource::<ExternalLinkIndex>()
            .init_resource::<IndexedExternalLinks>()
            .add_systems(PreUpdate, rebuild);
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_connector"]
pub struct ExternalLink;

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_connector"]
pub struct ExternalScopeRef(Id<ExternalScope>);

impl ExternalScopeRef {
    pub fn new(scope: Id<ExternalScope>) -> Self {
        Self(scope)
    }

    pub fn id(&self) -> &Id<ExternalScope> {
        &self.0
    }
}

#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub enum SyncPolicy {
    #[default]
    ReferenceOnly,
    ImportOnce,
    Bidirectional,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct SynchronizedFields(Vec<CanonicalField>);

impl SynchronizedFields {
    pub fn new(mut fields: Vec<CanonicalField>) -> Self {
        fields.sort();
        fields.dedup();
        Self(fields)
    }

    pub fn fields(&self) -> &[CanonicalField] {
        &self.0
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct PrimaryWorkItem;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct ReviewArtifact;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExternalLinkIdentity {
    provider: ProviderKey,
    scope: Id<ExternalScope>,
    kind: ResourceKind,
    id: ExternalId,
}

impl ExternalLinkIdentity {
    pub fn new(
        provider: ProviderKey,
        scope: Id<ExternalScope>,
        kind: ResourceKind,
        id: ExternalId,
    ) -> Self {
        Self {
            provider,
            scope,
            kind,
            id,
        }
    }

    pub fn provider(&self) -> &ProviderKey {
        &self.provider
    }

    pub fn scope(&self) -> &Id<ExternalScope> {
        &self.scope
    }

    pub fn kind(&self) -> &ResourceKind {
        &self.kind
    }

    pub fn external_id(&self) -> &ExternalId {
        &self.id
    }
}

pub struct ExternalLinkSpec<'a> {
    pub identity: ExternalLinkIdentity,
    pub owner: Entity,
    pub primary: bool,
    pub sync_policy: SyncPolicy,
    pub fields: &'a [CanonicalField],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalLinkConflict {
    Identity,
    PrimaryWorkItem,
    FieldAuthority,
}

impl std::fmt::Display for ExternalLinkConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Identity => formatter.write_str("external resource is already linked"),
            Self::PrimaryWorkItem => formatter.write_str("owner already has a primary work item"),
            Self::FieldAuthority => {
                formatter.write_str("another external link already synchronizes these fields")
            }
        }
    }
}

#[derive(Resource, Default)]
pub struct ExternalLinkIndex {
    identities: HashMap<ExternalLinkIdentity, Entity>,
    primary: HashSet<Entity>,
    authorities: HashMap<Entity, HashSet<CanonicalField>>,
    conflicts: HashMap<Entity, ExternalLinkConflict>,
}

impl ExternalLinkIndex {
    pub fn entity(&self, identity: &ExternalLinkIdentity) -> Option<Entity> {
        self.identities.get(identity).copied()
    }

    pub fn conflict(&self, entity: Entity) -> Option<ExternalLinkConflict> {
        self.conflicts.get(&entity).copied()
    }

    pub fn validate(&self, spec: &ExternalLinkSpec<'_>) -> Result<(), ExternalLinkConflict> {
        if self.identities.contains_key(&spec.identity) {
            return Err(ExternalLinkConflict::Identity);
        }
        if spec.primary && self.primary.contains(&spec.owner) {
            return Err(ExternalLinkConflict::PrimaryWorkItem);
        }
        if spec.sync_policy == SyncPolicy::Bidirectional
            && self
                .authorities
                .get(&spec.owner)
                .is_some_and(|held| spec.fields.iter().any(|field| held.contains(field)))
        {
            return Err(ExternalLinkConflict::FieldAuthority);
        }
        Ok(())
    }

    fn include(&mut self, entity: Entity, spec: ExternalLinkSpec<'_>) {
        if let Err(conflict) = self.validate(&spec) {
            self.conflicts.insert(entity, conflict);
            return;
        }
        self.identities.insert(spec.identity, entity);
        if spec.primary {
            self.primary.insert(spec.owner);
        }
        if spec.sync_policy == SyncPolicy::Bidirectional {
            self.authorities
                .entry(spec.owner)
                .or_default()
                .extend(spec.fields.iter().cloned());
        }
    }
}

#[derive(Resource, Default)]
struct IndexedExternalLinks(HashSet<Entity>);

type ExternalLinks<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ProviderKey,
        &'static ExternalScopeRef,
        &'static ResourceKind,
        &'static ExternalId,
        &'static ChildOf,
        Option<&'static PrimaryWorkItem>,
        Option<&'static SyncPolicy>,
        Option<&'static SynchronizedFields>,
    ),
    With<ExternalLink>,
>;

fn rebuild(
    links: ExternalLinks,
    mut changes: ExternalLinkChanges,
    mut current: ResMut<ExternalLinkIndex>,
    mut indexed: ResMut<IndexedExternalLinks>,
) {
    if !changes.dirty(&indexed) {
        return;
    }

    let mut next = ExternalLinkIndex::default();
    let mut next_indexed = HashSet::new();
    for (entity, provider, scope, kind, id, owner, primary, policy, fields) in &links {
        next_indexed.insert(entity);
        next.include(
            entity,
            ExternalLinkSpec {
                identity: ExternalLinkIdentity::new(
                    provider.clone(),
                    scope.id().clone(),
                    kind.clone(),
                    id.clone(),
                ),
                owner: owner.parent(),
                primary: primary.is_some(),
                sync_policy: policy.copied().unwrap_or_default(),
                fields: fields.map_or(&[], SynchronizedFields::fields),
            },
        );
    }
    *current = next;
    indexed.0 = next_indexed;
}

type ChangedExternalLinks<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<ExternalLink>,
        Or<(
            Added<ExternalLink>,
            Added<ProviderKey>,
            Added<ExternalScopeRef>,
            Added<ResourceKind>,
            Added<ExternalId>,
            Changed<ChildOf>,
            Changed<PrimaryWorkItem>,
            Changed<SyncPolicy>,
            Changed<SynchronizedFields>,
        )>,
    ),
>;

#[derive(SystemParam)]
struct ExternalLinkChanges<'w, 's> {
    changed: ChangedExternalLinks<'w, 's>,
    links: RemovedComponents<'w, 's, ExternalLink>,
    providers: RemovedComponents<'w, 's, ProviderKey>,
    scopes: RemovedComponents<'w, 's, ExternalScopeRef>,
    kinds: RemovedComponents<'w, 's, ResourceKind>,
    ids: RemovedComponents<'w, 's, ExternalId>,
    owners: RemovedComponents<'w, 's, ChildOf>,
    primary: RemovedComponents<'w, 's, PrimaryWorkItem>,
    policies: RemovedComponents<'w, 's, SyncPolicy>,
    fields: RemovedComponents<'w, 's, SynchronizedFields>,
}

impl ExternalLinkChanges<'_, '_> {
    fn dirty(&mut self, indexed: &IndexedExternalLinks) -> bool {
        if !self.changed.is_empty() {
            return true;
        }
        for entity in self.links.read() {
            if indexed.0.contains(&entity) {
                return true;
            }
        }
        for entity in self.providers.read() {
            if indexed.0.contains(&entity) {
                return true;
            }
        }
        for entity in self.scopes.read() {
            if indexed.0.contains(&entity) {
                return true;
            }
        }
        for entity in self.kinds.read() {
            if indexed.0.contains(&entity) {
                return true;
            }
        }
        for entity in self.ids.read() {
            if indexed.0.contains(&entity) {
                return true;
            }
        }
        for entity in self.owners.read() {
            if indexed.0.contains(&entity) {
                return true;
            }
        }
        for entity in self.primary.read() {
            if indexed.0.contains(&entity) {
                return true;
            }
        }
        for entity in self.policies.read() {
            if indexed.0.contains(&entity) {
                return true;
            }
        }
        for entity in self.fields.read() {
            if indexed.0.contains(&entity) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        app: App,
        owner: Entity,
        scope: Id<ExternalScope>,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(LinkPlugin);
            let owner = app.world_mut().spawn_empty().id();
            Self {
                app,
                owner,
                scope: Id::new("github.com/accounts/1"),
            }
        }

        fn spawn(&mut self, id: &str, primary: bool) -> Entity {
            let mut entity = self.app.world_mut().spawn((
                ExternalLink,
                ProviderKey::new("github").unwrap(),
                ExternalScopeRef::new(self.scope.clone()),
                ResourceKind::issue(),
                ExternalId::new(id).unwrap(),
                ChildOf(self.owner),
            ));
            if primary {
                entity.insert(PrimaryWorkItem);
            }
            entity.id()
        }
    }

    #[test]
    fn shared_identity_does_not_include_personal_connection_identity() {
        let mut fixture = Fixture::new();
        let first = fixture.spawn("issue-1", true);
        let duplicate = fixture.spawn("issue-1", false);

        fixture.app.update();

        let index = fixture.app.world().resource::<ExternalLinkIndex>();
        let conflicts = [first, duplicate]
            .into_iter()
            .filter(|entity| index.conflict(*entity) == Some(ExternalLinkConflict::Identity))
            .count();
        assert_eq!(conflicts, 1);
    }

    #[test]
    fn removing_a_link_repairs_the_index() {
        let mut fixture = Fixture::new();
        let link = fixture.spawn("issue-1", true);
        fixture.app.update();
        fixture.app.world_mut().entity_mut(link).despawn();

        fixture.app.update();

        let identity = ExternalLinkIdentity::new(
            ProviderKey::new("github").unwrap(),
            fixture.scope,
            ResourceKind::issue(),
            ExternalId::new("issue-1").unwrap(),
        );
        assert_eq!(
            fixture
                .app
                .world()
                .resource::<ExternalLinkIndex>()
                .entity(&identity),
            None
        );
    }
}
