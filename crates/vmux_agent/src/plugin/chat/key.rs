//! Handing a resolved chat key back to the page that pressed it.
//!
//! The chat page decides nothing about its own keyboard any more: it publishes `chat`, and
//! `chat.list` or `chat.selector` when one is showing, hands over whatever the core claimed, and
//! waits to be told what the press meant. This is the return leg — the only part of a chat
//! shortcut that lives on the host.
//!
//! It reads [`CommandIssued`] rather than [`AppCommand`] because it has to answer *that* page. A
//! broadcast command names no webview, and two chat panes can be open at once, so the wrong one
//! would answer the approval.

use bevy::prelude::*;
use bevy_cef::prelude::BinHostEmitEvent;
use vmux_chat::event::{CHAT_KEY_EVENT, ChatKey};
use vmux_command::{AppCommand, CommandIssued, ReadAppCommands};

pub(crate) struct ChatKeyPlugin;

impl Plugin for ChatKeyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, echo_key_command.in_set(ReadAppCommands));
    }
}

fn echo_key_command(mut issued: MessageReader<CommandIssued>, mut commands: Commands) {
    for issue in issued.read() {
        let AppCommand::Chat(key) = issue.command else {
            continue;
        };
        commands.trigger(BinHostEmitEvent::from_rkyv(
            issue.caller,
            CHAT_KEY_EVENT,
            &ChatKey::from(key),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_command::ChatKeyCommand;

    /// What the plugin pushed back to a page, which is its only observable.
    #[derive(Resource, Default)]
    struct Echoed(Vec<(Entity, String)>);

    impl Echoed {
        fn record(trigger: On<BinHostEmitEvent>, mut echoed: ResMut<Self>) {
            let decoded = rkyv::from_bytes::<ChatKey, rkyv::rancor::Error>(&trigger.payload)
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
                .add_plugins(ChatKeyPlugin)
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

    /// A chat key goes back to the pane that pressed it and to no other, which is the whole reason
    /// this reads the caller-stamped bus instead of the broadcast one. Two panes can hold two
    /// different approvals, and answering both would decide one of them for the user.
    #[test]
    fn a_resolved_key_reaches_only_the_page_that_sent_it() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn_empty().id();
        let other = app.world_mut().spawn_empty().id();

        Echo::issue(
            &mut app,
            pressed,
            AppCommand::Chat(ChatKeyCommand::ListChoose),
        );

        assert_eq!(
            app.world().resource::<Echoed>().0,
            vec![(pressed, format!("{CHAT_KEY_EVENT}:ListChoose"))]
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
    /// chat page can decode.
    #[test]
    fn a_command_that_is_not_a_chat_key_is_left_alone() {
        let mut app = Echo::app();
        let caller = app.world_mut().spawn_empty().id();

        Echo::issue(
            &mut app,
            caller,
            AppCommand::Terminal(vmux_command::TerminalCommand::Clear),
        );

        assert!(app.world().resource::<Echoed>().0.is_empty());
    }
}
