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

/// What a page currently calls itself, as opposed to the name the host gave it when it opened.
///
/// Deliberately not a field of [`crate::PageMetadata`]: that is reflected into saved sessions by
/// field name, so growing it is a save-format migration, and a title reported by a live terminal
/// or conversation has no business outliving the process that reported it. Dropping this
/// component reverts the page to its host-given name, which is how a page survives its reporter
/// going away.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PageIdentity {
    pub title: Option<String>,
    pub icon: Option<crate::PageIcon>,
}

impl PageIdentity {
    pub fn of_title(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            icon: None,
        }
    }
}

impl crate::PageMetadata {
    /// The title to show: what the page calls itself, else the name the host gave it.
    ///
    /// An empty reported title counts as unset — a page that blanks its own title has nothing to
    /// say, rather than wanting a blank tab.
    pub fn title_with<'a>(&'a self, identity: Option<&'a PageIdentity>) -> &'a str {
        match identity.and_then(|identity| identity.title.as_deref()) {
            Some(title) if !title.is_empty() => title,
            _ => &self.title,
        }
    }

    /// The icon to show, on the same terms as [`Self::title_with`].
    pub fn icon_with<'a>(&'a self, identity: Option<&'a PageIdentity>) -> &'a crate::PageIcon {
        match identity.and_then(|identity| identity.icon.as_ref()) {
            Some(icon) if !icon.is_none() => icon,
            _ => &self.icon,
        }
    }
}

/// The pane the keyboard belongs to, as the host understands it.
///
/// A cached projection of the focused stack rather than a second opinion about it: the arbiter in
/// `vmux_browser` derives this every frame and keeps it on exactly one entity. What it buys over
/// asking the stack directly is that it can be watched — `Added<KeyboardOwner>` is how a pane
/// learns it has just come to the front, which a `Res<FocusedStack>` read cannot tell you.
///
/// This was `CefKeyboardTarget`, declared in the CEF fork, where it named the webviews an
/// offscreen browser's forwarded keys should go to. Nothing forwards keys any more — AppKit hands
/// them to whichever surface holds first responder — so the CEF half is deleted and what remains
/// never had anything to do with CEF.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyboardOwner;

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
