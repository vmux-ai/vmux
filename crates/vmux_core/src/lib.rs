//! Shared component types and reflection registration used across all vmux crates.

pub mod agent_setup;
pub mod dom_snapshot;
pub mod editor;
pub mod event;
pub mod icon;
pub mod knowledge;
pub mod media;
pub mod process_id;
pub mod scroll;
pub mod tools;
pub mod vault;
pub use editor::{CursorPos, EditMode, KeymapKind, SelSpan};
pub use icon::{BuiltinIcon, PageIcon};
pub use process_id::ProcessId;

#[cfg(not(web))]
pub mod host_spawn;
#[cfg(not(web))]
pub mod page;
#[cfg(not(web))]
pub mod page_open;
#[cfg(not(web))]
pub mod profile;
#[cfg(not(web))]
pub mod terminal;

#[cfg(not(web))]
pub mod agent;
#[cfg(not(web))]
pub mod archive;
#[cfg(not(web))]
pub mod browser;
#[cfg(not(web))]
pub mod extension;
#[cfg(not(web))]
pub mod notify;
#[cfg(not(web))]
pub mod team;
#[cfg(not(web))]
pub use archive::{
    ArchivedPage, ArchivedPagePosition, ArchivedTabPage, PageArchiveRequest, PaneStep, SplitAxis,
};
#[cfg(not(web))]
pub use host_spawn::{HostSpawnRegistry, register_host_spawn};
#[cfg(not(web))]
pub use notify::{AgentAttention, AgentDoneUnseen, BellReceived, OsNotify};
#[cfg(not(web))]
pub use page_open::{
    CefPageAttachRequest, PageOpenError, PageOpenHandled, PageOpenId, PageOpenRequest, PageOpenSet,
    PageOpenTarget, PageOpenTask,
};

#[cfg(not(web))]
use bevy::prelude::*;
#[cfg(not(web))]
use moonshine_save::prelude::*;

#[cfg(not(web))]
/// Registers reflection for the shared component types so they can be saved and loaded.
pub struct CorePlugin;

#[cfg(not(web))]
impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PageMetadata>()
            .register_type::<crate::icon::PageIcon>()
            .register_type::<crate::icon::BuiltinIcon>()
            .register_type::<ArchivedPage>()
            .register_type::<crate::archive::ArchivedPagePosition>()
            .register_type::<crate::archive::ArchivedTabPage>()
            .register_type::<crate::archive::PaneStep>()
            .register_type::<crate::archive::SplitAxis>()
            .register_type::<Vec<crate::archive::PaneStep>>()
            .register_type::<CreatedAt>()
            .register_type::<LastActivatedAt>()
            .register_type::<Visit>()
            .register_type::<Url>()
            .register_type::<VisitCount>()
            .register_type::<LastVisitedAt>()
            .register_type::<VisitedUrl>()
            .register_type::<TransitionType>()
            .register_type::<Order>()
            .register_type::<Active>()
            .register_type::<BookmarkOrder>()
            .register_type::<Pin>()
            .register_type::<Bookmark>()
            .register_type::<Folder>()
            .register_type::<Collapsed>()
            .register_type::<Uuid>()
            .register_type::<Children>()
            .register_type::<ChildOf>();
    }
}

// ── Time helpers ─────────────────────────────────────────────────────

#[cfg(not(web))]
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ── Shared components ────────────────────────────────────────────────

#[cfg_attr(not(web), derive(Component, Reflect))]
#[cfg_attr(not(web), reflect(Component, Default))]
#[cfg_attr(not(web), type_path = "vmux_header::system")]
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PageMetadata {
    pub title: String,
    pub url: String,
    pub icon: crate::icon::PageIcon,
    pub bg_color: Option<String>,
}

#[cfg(not(web))]
#[derive(Component, Clone, Debug)]
pub struct OscTitle(pub String);

/// The working directory of a non-terminal agent pane (e.g. an ACP session), so the command
/// bar's "current work" can list its cwd contents the same way it lists open terminals' cwds.
/// Terminals carry their cwd on `TerminalLaunch`; this covers agents that have no PTY.
#[cfg(not(web))]
#[derive(Component, Clone, Debug)]
pub struct AgentWorkingDir(pub String);

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct CreatedAt(pub i64);

#[cfg(not(web))]
impl CreatedAt {
    pub fn now() -> Self {
        Self(now_millis())
    }
}

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct LastActivatedAt(pub i64);

#[cfg(not(web))]
impl LastActivatedAt {
    pub fn now() -> Self {
        Self(now_millis())
    }
}

#[cfg(not(web))]
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

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct Visit;

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Ready;

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct Url;

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct VisitCount(pub u32);

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct LastVisitedAt(pub i64);

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_core"]
pub struct Order(pub u32);

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Active;

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct BookmarkOrder(pub u32);

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Pin;

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Bookmark;

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Folder;

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Collapsed;

#[cfg(not(web))]
#[derive(Component, Clone, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Uuid(pub String);

#[cfg(not(web))]
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct VisitedUrl(pub Entity);

#[cfg(not(web))]
impl Default for VisitedUrl {
    fn default() -> Self {
        Self(Entity::PLACEHOLDER)
    }
}

#[cfg(not(web))]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_new_history_components() {
        let mut app = App::new();
        app.add_plugins(CorePlugin);

        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(registry.get(std::any::TypeId::of::<Url>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<VisitCount>()).is_some());
        assert!(
            registry
                .get(std::any::TypeId::of::<LastVisitedAt>())
                .is_some()
        );
        assert!(registry.get(std::any::TypeId::of::<VisitedUrl>()).is_some());
        assert!(
            registry
                .get(std::any::TypeId::of::<TransitionType>())
                .is_some()
        );
    }

    #[test]
    fn registers_bookmark_components() {
        let mut app = App::new();
        app.add_plugins(CorePlugin);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(
            registry
                .get(std::any::TypeId::of::<BookmarkOrder>())
                .is_some()
        );
        assert!(registry.get(std::any::TypeId::of::<Pin>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Bookmark>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Folder>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Collapsed>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Uuid>()).is_some());
    }

    #[test]
    fn active_marker_is_registered_and_reflectable() {
        let mut app = App::new();
        app.add_plugins(CorePlugin);

        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(registry.get(std::any::TypeId::of::<Active>()).is_some());
    }
}
