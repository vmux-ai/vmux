use bevy::prelude::*;

use crate::definition::CommandInvocation;

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

#[derive(bevy::ecs::system::SystemParam)]
pub struct CommandIssuer<'w> {
    pub invocations: MessageWriter<'w, CommandInvocation>,
}

impl CommandIssuer<'_> {
    pub fn issue_id(&mut self, caller: Entity, id: impl Into<String>) {
        self.invocations
            .write(CommandInvocation::new(caller, id.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::SystemState;

    #[test]
    fn issue_writes_an_invocation() {
        let mut app = App::new();
        app.add_message::<CommandInvocation>();
        let caller = app.world_mut().spawn_empty().id();
        let mut state = SystemState::<CommandIssuer>::new(app.world_mut());
        {
            let mut issuer = state.get_mut(app.world_mut()).expect("system params valid");
            issuer.issue_id(caller, "terminal_clear");
        }
        state.apply(app.world_mut());
        let invocations = app
            .world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(
            invocations,
            [CommandInvocation::new(caller, "terminal_clear")]
        );
    }
}
