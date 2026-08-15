//! The shared component vocabulary: the markers and timestamps every vmux crate spawns onto
//! pages, panes and history entries.
//!
//! Each reflected type pins its `type_path`, so these names stay stable in saved sessions no
//! matter which module the declaration lives in.

use bevy::prelude::*;
use moonshine_save::prelude::*;

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Component, Clone, Debug)]
pub struct OscTitle(pub String);

/// The working directory of a non-terminal agent pane (e.g. an ACP session), so the command
/// bar's "current work" can list its cwd contents the same way it lists open terminals' cwds.
/// Terminals carry their cwd on `TerminalLaunch`; this covers agents that have no PTY.
#[derive(Component, Clone, Debug)]
pub struct AgentWorkingDir(pub String);

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

/// The URL a fresh stack opens when nothing else has been asked for.
///
/// Resolved once from settings and the active space, then read by everything that has to open
/// something without being told what.
#[derive(bevy::prelude::Resource, Clone, Debug, Default)]
pub struct EffectiveStartupUrl(pub String);
