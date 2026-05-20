use bevy::prelude::*;

#[derive(Message, Debug, Clone)]
pub struct SettingsPageSpawnRequest {
    pub target_stack: Entity,
}

#[derive(Message, Debug, Clone)]
pub struct SpacesPageSpawnRequest {
    pub target_stack: Entity,
}
