use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PageOpenId(pub [u8; 16]);

impl Default for PageOpenId {
    fn default() -> Self {
        Self::new()
    }
}

impl PageOpenId {
    pub fn new() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }
}

#[derive(Clone, Debug)]
pub enum PageOpenTarget {
    ActiveStack,
    Stack(Entity),
    ActiveStackInPane(Entity),
    NewStackInPane(Entity),
}

#[derive(Message, Clone, Debug)]
pub struct PageOpenRequest {
    pub target: PageOpenTarget,
    pub url: String,
    pub request_id: Option<[u8; 16]>,
}

#[derive(Component, Clone, Debug)]
pub struct PageOpenTask {
    pub id: PageOpenId,
    pub stack: Entity,
    pub url: String,
    pub request_id: Option<[u8; 16]>,
}

#[derive(Component, Clone, Debug)]
pub struct PageOpenHandled;

#[derive(Component, Clone, Debug)]
pub struct PageOpenError {
    pub message: String,
}

/// Host-driven request to navigate an already-open `file://` page (the
/// `target` entity) to a different file and scroll to `line`. Consumed by the
/// editor; lets an agent's follow-pane swap content in place without a new tab.
#[derive(Message, Clone, Debug)]
pub struct FileFollowRequest {
    pub target: Entity,
    pub path: String,
    pub line: Option<u32>,
}

#[derive(Message, Clone, Debug)]
pub struct CefPageAttachRequest {
    pub stack: Entity,
    pub url: String,
    pub title: String,
    pub bg_color: Option<String>,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PageOpenSet {
    ResolveTarget,
    HandleKnownPages,
    Fallback,
    Respond,
}
