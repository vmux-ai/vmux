use bevy_ecs::prelude::*;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use vmux_api::Id;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_connector"]
pub struct ExternalLink;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_connector"]
pub struct ExternalScope;

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct ProviderKey(pub String);

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct ExternalHost(pub String);

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct ExternalNamespace(pub String);

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[type_path = "vmux_connector"]
pub struct ExternalScopeRef(pub Id<ExternalScope>);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct CanAccess(pub Vec<Id<ExternalScope>>);

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct AccountExternalId(pub String);

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct ExternalId(pub String);

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct ExternalKey(pub String);

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct ExternalUrl(pub String);

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct ResourceKind(pub String);

impl ResourceKind {
    pub fn repository() -> Self {
        Self("repository".into())
    }

    pub fn issue() -> Self {
        Self("issue".into())
    }

    pub fn pull_request() -> Self {
        Self("pull-request".into())
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

#[derive(Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[type_path = "vmux_connector"]
pub struct CanonicalField(pub String);

impl CanonicalField {
    pub fn title() -> Self {
        Self("title".into())
    }

    pub fn description() -> Self {
        Self("description".into())
    }

    pub fn workflow_state() -> Self {
        Self("workflow-state".into())
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct SynchronizedFields(pub Vec<CanonicalField>);

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
    pub provider: ProviderKey,
    pub scope: Id<ExternalScope>,
    pub kind: ResourceKind,
    pub id: ExternalId,
}

pub struct ExternalLinkSpec<'a> {
    pub identity: ExternalLinkIdentity,
    pub parent: Entity,
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
            Self::PrimaryWorkItem => formatter.write_str("parent already has a primary work item"),
            Self::FieldAuthority => {
                formatter.write_str("another external link already synchronizes these fields")
            }
        }
    }
}

#[derive(Resource, Default, PartialEq, Eq)]
pub struct ExternalLinkIndex {
    identities: HashSet<ExternalLinkIdentity>,
    primary: HashSet<Entity>,
    authorities: HashMap<Entity, HashSet<CanonicalField>>,
}

impl ExternalLinkIndex {
    pub fn include(&mut self, spec: ExternalLinkSpec<'_>) {
        self.identities.insert(spec.identity);
        if spec.primary {
            self.primary.insert(spec.parent);
        }
        if spec.sync_policy == SyncPolicy::Bidirectional {
            self.authorities
                .entry(spec.parent)
                .or_default()
                .extend(spec.fields.iter().cloned());
        }
    }

    pub fn reserve(&mut self, spec: ExternalLinkSpec<'_>) -> Result<(), ExternalLinkConflict> {
        if self.identities.contains(&spec.identity) {
            return Err(ExternalLinkConflict::Identity);
        }
        if spec.sync_policy == SyncPolicy::Bidirectional
            && self
                .authorities
                .get(&spec.parent)
                .is_some_and(|held| spec.fields.iter().any(|field| held.contains(field)))
        {
            return Err(ExternalLinkConflict::FieldAuthority);
        }
        if spec.primary && self.primary.contains(&spec.parent) {
            return Err(ExternalLinkConflict::PrimaryWorkItem);
        }
        self.include(spec);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_resource_identity_deduplicates_without_connection_identity() {
        let parent = Entity::from_raw_u32(1).unwrap();
        let identity = ExternalLinkIdentity {
            provider: ProviderKey("github".into()),
            scope: Id::new("github.com/accounts/1"),
            kind: ResourceKind::issue(),
            id: ExternalId("I_kwDO".into()),
        };
        let mut index = ExternalLinkIndex::default();

        index
            .reserve(ExternalLinkSpec {
                identity: identity.clone(),
                parent,
                primary: true,
                sync_policy: SyncPolicy::ReferenceOnly,
                fields: &[],
            })
            .unwrap();

        assert_eq!(
            index.reserve(ExternalLinkSpec {
                identity,
                parent: Entity::from_raw_u32(2).unwrap(),
                primary: false,
                sync_policy: SyncPolicy::ReferenceOnly,
                fields: &[],
            }),
            Err(ExternalLinkConflict::Identity)
        );
    }
}
