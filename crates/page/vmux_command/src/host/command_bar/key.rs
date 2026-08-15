//! Handing a resolved command-bar key back to the page that pressed it.
//!
//! The command bar decides nothing about its own keyboard any more: it publishes the context key
//! `command-bar`, hands over whatever the core claimed, and waits to be told what the press meant.
//! This is the return leg — the only part of a command bar shortcut that lives on the host.
//!
//! It reads [`CommandIssued`] rather than [`crate::AppCommand`] because it has to answer
//! *that* page. A broadcast command names no webview, and with two palettes on screen the wrong
//! one would move its selection.

use crate::event::{COMMAND_BAR_KEY_EVENT, CommandBarKey};
use crate::{AppCommand, CommandIssued, ReadAppCommands};
use bevy::prelude::*;
use bevy_cef::prelude::BinHostEmitEvent;

pub(crate) struct CommandBarKeyPlugin;

impl Plugin for CommandBarKeyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, echo_key_command.in_set(ReadAppCommands));
    }
}

fn echo_key_command(mut issued: MessageReader<CommandIssued>, mut commands: Commands) {
    for issue in issued.read() {
        let AppCommand::CommandBar(key) = issue.command else {
            continue;
        };
        commands.trigger(BinHostEmitEvent::from_rkyv(
            issue.caller,
            COMMAND_BAR_KEY_EVENT,
            &CommandBarKey::from(key),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandBarKeyCommand;

    /// What the plugin pushed back to a page, which is its only observable.
    #[derive(Resource, Default)]
    struct Echoed(Vec<(Entity, String)>);

    impl Echoed {
        fn record(trigger: On<BinHostEmitEvent>, mut echoed: ResMut<Self>) {
            let decoded = rkyv::from_bytes::<CommandBarKey, rkyv::rancor::Error>(&trigger.payload)
                .map(|key| format!("{key:?}"))
                .unwrap_or_else(|_| "undecodable".to_string());
            echoed
                .0
                .push((trigger.webview, format!("{}:{decoded}", trigger.id)));
        }
    }

    struct Echo;

    impl Echo {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(CommandBarKeyPlugin)
                .add_message::<CommandIssued>()
                .init_resource::<Echoed>()
                .add_observer(Echoed::record);
            app
        }

        fn issue(app: &mut App, caller: Entity, command: AppCommand) {
            app.world_mut()
                .resource_mut::<bevy::ecs::message::Messages<CommandIssued>>()
                .write(CommandIssued { caller, command });
            app.update();
        }
    }

    /// A command-bar key goes back to the palette that pressed it and to no other, which is the
    /// whole reason this reads the caller-stamped bus instead of the broadcast one.
    #[test]
    fn a_resolved_key_reaches_only_the_page_that_sent_it() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn_empty().id();
        let other = app.world_mut().spawn_empty().id();

        Echo::issue(
            &mut app,
            pressed,
            AppCommand::CommandBar(CommandBarKeyCommand::Next),
        );

        assert_eq!(
            app.world().resource::<Echoed>().0,
            vec![(pressed, format!("{COMMAND_BAR_KEY_EVENT}:Next"))]
        );
        assert!(
            !app.world()
                .resource::<Echoed>()
                .0
                .iter()
                .any(|(entity, _)| *entity == other)
        );
    }

    /// Every other command on the bus belongs to someone else. Echoing one would push a payload no
    /// palette can decode.
    #[test]
    fn a_command_that_is_not_a_command_bar_key_is_left_alone() {
        let mut app = Echo::app();
        let caller = app.world_mut().spawn_empty().id();

        Echo::issue(
            &mut app,
            caller,
            AppCommand::Terminal(crate::TerminalCommand::Clear),
        );

        assert!(app.world().resource::<Echoed>().0.is_empty());
    }
}
