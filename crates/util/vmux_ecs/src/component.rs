use bevy_ecs::prelude::*;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};
use moonshine_save::prelude::*;
use std::path::PathBuf;

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Reflect,
    serde::Deserialize,
    serde::Serialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[type_path = "vmux_ecs"]
pub struct UnixMillis(pub i64);

impl UnixMillis {
    pub fn now() -> Self {
        Self(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        )
    }
}

#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Reflect,
    serde::Deserialize,
    serde::Serialize,
)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct UpdatedAt(pub UnixMillis);

impl UpdatedAt {
    pub fn now() -> Self {
        Self(UnixMillis::now())
    }
}

#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Reflect,
    serde::Deserialize,
    serde::Serialize,
)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct DeletedAt(pub UnixMillis);

#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Reflect,
    serde::Deserialize,
    serde::Serialize,
)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct RemoteUpdatedAt(pub UnixMillis);

#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Reflect,
    serde::Deserialize,
    serde::Serialize,
)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct LastSyncedAt(pub UnixMillis);

#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    Reflect,
    serde::Deserialize,
    serde::Serialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct Revision(u64);

impl Revision {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyboardOwner;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PagePresented;

#[derive(Component, Reflect)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_ecs"]
#[vmux_api::contract(Copy, Default, Eq)]
pub enum CollectionView {
    #[default]
    List,
    Grid,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowFullscreen(pub bool);

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowFullscreenSet;

#[derive(Component, Clone, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_ecs"]
pub struct Cwd(pub PathBuf);

#[derive(Component, Clone, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_ecs"]
pub struct Description(pub String);

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Terminal;

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct JsonArguments(pub serde_json::Value);

impl JsonArguments {
    pub fn parse<T: serde::de::DeserializeOwned>(&self, name: &str) -> Result<T, String> {
        serde_json::from_value(self.0.clone())
            .map_err(|error| format!("{name}: invalid arguments: {error}"))
    }
}

impl TryFrom<&vmux_api::json::JsonValue> for JsonArguments {
    type Error = String;

    fn try_from(input: &vmux_api::json::JsonValue) -> Result<Self, Self::Error> {
        let value = serde_json::Value::try_from(input)
            .map_err(|error| format!("invalid JSON arguments: {error}"))?;
        if !value.is_object() {
            return Err("command arguments must be a JSON object".to_string());
        }
        Ok(Self(value))
    }
}

#[derive(Component, Debug, PartialEq, Eq)]
pub struct EntityTarget<T: Send + Sync + 'static> {
    entity: Entity,
    marker: std::marker::PhantomData<fn() -> T>,
}

impl<T: Send + Sync + 'static> EntityTarget<T> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            marker: std::marker::PhantomData,
        }
    }

    pub const fn entity(&self) -> Entity {
        self.entity
    }
}

impl<T: Send + Sync + 'static> Clone for EntityTarget<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for EntityTarget<T> {}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessAnchor(pub crate::ProcessId);

impl ProcessAnchor {
    pub fn required(value: Option<&Self>, name: &str) -> Result<crate::ProcessId, String> {
        value.map(|anchor| anchor.0).ok_or_else(|| {
            format!("{name} requires an agent anchor (not available to this client)")
        })
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct HostShell(pub String);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegistrationOrder(pub u32);

#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct CreatedAt(pub i64);

impl CreatedAt {
    pub fn now() -> Self {
        Self(UnixMillis::now().0)
    }
}

#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct LastActivatedAt(pub i64);

impl LastActivatedAt {
    pub fn now() -> Self {
        Self(UnixMillis::now().0)
    }

    pub fn latest(entities: impl Iterator<Item = (Entity, Self)>) -> Option<Entity> {
        entities
            .max_by_key(|(_, activated_at)| activated_at.0)
            .map(|(entity, _)| entity)
    }
}

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_ecs"]
pub struct ArchivedAt(pub i64);

#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct ActivateRequest {
    #[event_target]
    pub entity: Entity,
}

#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct Visit;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Ready;

#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct Url;

#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct VisitCount(pub u32);

#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct LastVisitedAt(pub i64);

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_ecs"]
pub struct Order(pub u32);

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Active;

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct BookmarkOrder(pub u32);

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Pin;

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Bookmark;

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Folder;

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Collapsed;

#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct VisitedUrl(pub Entity);

impl Default for VisitedUrl {
    fn default() -> Self {
        Self(Entity::PLACEHOLDER)
    }
}

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub enum TransitionType {
    #[default]
    Link,
    Typed,
    Reload,
    BackForward,
    Redirect,
    Other,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectiveStartupUrl(pub String);
