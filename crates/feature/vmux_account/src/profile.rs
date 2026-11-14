use bevy_ecs::prelude::*;
use bevy_ecs::reflect::ReflectComponent;
use bevy_reflect::{Reflect, ReflectDeserialize, ReflectSerialize};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vmux_ecs::record::{Format, Persisted, ReflectRelationship, ReflectStored, Scope};
use vmux_ecs::{ProfileId, Revision, Uuid as Id};

#[derive(Component, Reflect, Serialize, Deserialize)]
#[reflect(Component, Serialize, Deserialize, Stored, @Format::new("account", 1))]
pub struct Account;

impl Account {
    pub(crate) fn bundle(id: Uuid) -> impl Bundle {
        (
            Self,
            Id(id.to_string()),
            Name::new("Local account"),
            Scope(id),
            Revision(1),
            Persisted(0),
        )
    }
}

#[derive(Component, Reflect, Serialize, Deserialize)]
#[reflect(Component, Serialize, Deserialize, Stored, @Format::new("profile", 1))]
pub struct Profile;

impl Profile {
    pub(crate) fn bundle(
        scope: Uuid,
        id: Uuid,
        directory: &str,
        name: &str,
        owner: Entity,
    ) -> impl Bundle {
        (
            Self,
            Id(id.to_string()),
            ProfileId(directory.to_string()),
            Name::new(name.to_string()),
            Scope(scope),
            Revision(1),
            Persisted(0),
            Owner(owner),
        )
    }

    pub(crate) fn name(raw: &str) -> Result<String, Failure> {
        let name = raw.trim();
        if name.is_empty() || name.chars().count() > 256 || name.chars().any(char::is_control) {
            return Err(Failure::InvalidName);
        }
        Ok(name.to_string())
    }

    pub(crate) fn label(directory: &str) -> String {
        let mut characters = directory.chars();
        match characters.next() {
            Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
            None => "Personal".into(),
        }
    }
}

#[derive(Component, Reflect)]
#[reflect(Component, Relationship, @Format::new("account", 1))]
#[relationship(relationship_target = Profiles)]
pub struct Owner(pub Entity);

#[derive(Component, Default)]
#[relationship_target(relationship = Owner, linked_spawn)]
pub struct Profiles(Vec<Entity>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    InvalidName,
    UnknownProfile,
    NotReady,
    Conflict,
    Storage(String),
    InvalidStore,
}
