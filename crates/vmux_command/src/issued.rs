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
mod tests {
    use super::*;
    use crate::command::TerminalCommand;
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::SystemState;

    #[test]
    fn issue_writes_both_buses() {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<CommandIssued>();
        let caller = app.world_mut().spawn_empty().id();
        let mut state = SystemState::<CommandIssuer>::new(app.world_mut());
        {
            let mut issuer = state
                .get_mut(app.world_mut())
                .expect("system params valid");
            issuer.issue(caller, AppCommand::Terminal(TerminalCommand::Clear));
        }
        state.apply(app.world_mut());
        let app_count = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .count();
        let issued_count = app
            .world_mut()
            .resource_mut::<Messages<CommandIssued>>()
            .drain()
            .count();
        assert_eq!(app_count, 1);
        assert_eq!(issued_count, 1);
    }
}
