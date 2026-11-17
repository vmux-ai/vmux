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

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component, Serialize, Deserialize, Stored, @Format::new("account_binding", 1))]
pub struct Binding {
    pub server: String,
    pub account_id: Option<String>,
    pub credential_generation: u64,
}

impl Binding {
    pub fn is_linked(&self) -> bool {
        self.account_id.is_some()
    }

    pub(crate) fn is_valid(&self) -> bool {
        match &self.account_id {
            Some(id) => {
                !self.server.is_empty()
                    && self.credential_generation > 0
                    && self.server.len() <= 512
                    && !self.server.chars().any(char::is_control)
                    && Uuid::parse_str(id).is_ok_and(|id| !id.is_nil())
            }
            None => self.server.is_empty() && self.credential_generation == 0,
        }
    }
}

#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Serialize, Deserialize, Stored, @Format::new("profile_sync", 1))]
pub struct SyncEnabled(pub bool);

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
            return Err(Failure::invalid_name());
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
pub struct Failure {
    pub(crate) kind: FailureKind,
    detail: Option<String>,
}

impl Failure {
    fn new(kind: FailureKind) -> Self {
        Self { kind, detail: None }
    }

    pub fn kind(&self) -> FailureKind {
        self.kind
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    pub fn storage(detail: String) -> Self {
        Self {
            kind: FailureKind::Storage,
            detail: Some(detail),
        }
    }

    pub fn invalid_name() -> Self {
        Self::new(FailureKind::InvalidName)
    }

    pub fn unknown_profile() -> Self {
        Self::new(FailureKind::UnknownProfile)
    }

    pub fn not_ready() -> Self {
        Self::new(FailureKind::NotReady)
    }

    pub fn conflict() -> Self {
        Self::new(FailureKind::Conflict)
    }

    pub fn invalid_store() -> Self {
        Self::new(FailureKind::InvalidStore)
    }

    pub fn account_mismatch() -> Self {
        Self::new(FailureKind::AccountMismatch)
    }

    pub fn not_linked() -> Self {
        Self::new(FailureKind::NotLinked)
    }

    pub fn not_authenticated() -> Self {
        Self::new(FailureKind::NotAuthenticated)
    }

    pub fn authorization_expired() -> Self {
        Self::new(FailureKind::AuthorizationExpired)
    }

    pub fn access_denied() -> Self {
        Self::new(FailureKind::AccessDenied)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    InvalidName,
    UnknownProfile,
    NotReady,
    Conflict,
    Storage,
    InvalidStore,
    AccountMismatch,
    NotLinked,
    NotAuthenticated,
    AuthorizationExpired,
    AccessDenied,
}
