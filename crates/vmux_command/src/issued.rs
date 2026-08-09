use bevy::prelude::*;

use crate::command::AppCommand;

#[derive(Message, Clone)]
pub struct CommandIssued {
    pub caller: Entity,
    pub command: AppCommand,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct CommandIssuer<'w> {
    pub app: MessageWriter<'w, AppCommand>,
    pub issued: MessageWriter<'w, CommandIssued>,
}

impl CommandIssuer<'_> {
    pub fn issue(&mut self, caller: Entity, command: AppCommand) {
        self.issued.write(CommandIssued {
            caller,
            command: command.clone(),
        });
        self.app.write(command);
    }
}

#[cfg(test)]
#[path = "issued.test.rs"]
mod tests;
