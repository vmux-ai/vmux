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
