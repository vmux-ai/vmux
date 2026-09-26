use bevy::prelude::*;
use moonshine_save::prelude::*;

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyboardOwner;

#[derive(Component, Clone, Debug)]
pub struct AgentWorkingDir(pub String);

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
        Self(now_millis())
    }
}

#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct LastActivatedAt(pub i64);

impl LastActivatedAt {
    pub fn now() -> Self {
        Self(now_millis())
    }
}

pub fn focus_pane_entity(entity: Entity, commands: &mut Commands, child_of_q: &Query<&ChildOf>) {
    use bevy::ecs::relationship::Relationship;
    commands.entity(entity).insert(LastActivatedAt::now());
    let mut current = entity;
    while let Ok(parent_rel) = child_of_q.get(current) {
        let parent = parent_rel.get();
        commands.entity(parent).insert(LastActivatedAt::now());
        current = parent;
    }
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
#[type_path = "vmux_core"]
pub struct Order(pub u32);

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Active;

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct BookmarkOrder(pub u32);

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Pin;

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Bookmark;

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Folder;

#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Collapsed;

#[derive(Component, Clone, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Uuid(pub String);

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

#[derive(bevy::prelude::Resource, Clone, Debug, Default)]
pub struct EffectiveStartupUrl(pub String);

impl EffectiveStartupUrl {
    pub const START_PAGE: &'static str = "vmux://start/";

    pub fn resolve(resolved: Option<&Self>) -> String {
        match resolved {
            Some(url) if !url.0.is_empty() => url.0.clone(),
            _ => Self::START_PAGE.to_string(),
        }
    }
}
