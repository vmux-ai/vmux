use bevy::prelude::*;

use crate::ProcessId;

#[derive(Message, Clone)]
pub struct BellReceived {
    pub process_id: ProcessId,
}

#[derive(Message, Clone)]
pub struct AgentAttention {
    pub entity: Entity,
    pub title: Option<String>,
    pub body: Option<String>,
}

#[derive(Message, Clone)]
pub struct OsNotify {
    pub title: String,
    pub body: String,
}

#[derive(Component)]
pub struct AgentDoneUnseen;

#[cfg(test)]
#[path = "notify.test.rs"]
mod tests;
