use bevy::prelude::*;

#[derive(Message, Clone)]
pub struct ExLineSubmitted {
    pub stack: Option<Entity>,
    pub line: String,
}

#[derive(Message, Clone)]
pub struct FileStatusPicked {
    pub stack: Option<Entity>,
    pub pick: vmux_api::command_bar::CommandBarPick,
}
