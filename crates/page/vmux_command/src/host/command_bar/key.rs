use crate::event::CommandBarKey;
use crate::{CommandDefinition, CommandInvocation, ReadCommandRequests};
use bevy::prelude::*;
use bevy_cef::prelude::BinHostEmitEvent;

pub(crate) struct KeyPlugin;

impl Plugin for KeyPlugin {
    fn build(&self, app: &mut App) {
        CommandBarKeyRequest::register(app);
        app.add_systems(Update, echo_key_command.in_set(ReadCommandRequests));
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct CommandBarKeyRequest {
    caller: Entity,
    key: CommandBarKey,
}

impl CommandBarKeyRequest {
    pub fn register(app: &mut App) {
        CommandDefinition::register(app, Self::definitions, Self::from_invocation);
    }

    pub fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("command_bar_next", "Next Result", "Command Bar")
                .hidden()
                .direct_when("ArrowDown", Some("command-bar"))
                .direct_when("Ctrl+n", Some("command-bar"))
                .direct_when("Ctrl+j", Some("command-bar")),
            CommandDefinition::new("command_bar_previous", "Previous Result", "Command Bar")
                .hidden()
                .direct_when("ArrowUp", Some("command-bar"))
                .direct_when("Ctrl+p", Some("command-bar"))
                .direct_when("Ctrl+k", Some("command-bar")),
            CommandDefinition::new("command_bar_complete", "Accept Completion", "Command Bar")
                .hidden()
                .direct_when("Tab", Some("command-bar")),
            CommandDefinition::new("command_bar_dismiss", "Dismiss Command Bar", "Command Bar")
                .hidden()
                .direct_when("Escape", Some("command-bar"))
                .direct_when("Ctrl+c", Some("command-bar")),
        ]
    }

    pub fn from_invocation(invocation: &CommandInvocation) -> Option<Self> {
        let key = match invocation.id.as_str() {
            "command_bar_next" => CommandBarKey::Next,
            "command_bar_previous" => CommandBarKey::Previous,
            "command_bar_complete" => CommandBarKey::Complete,
            "command_bar_dismiss" => CommandBarKey::Dismiss,
            _ => return None,
        };
        Some(Self {
            caller: invocation.caller,
            key,
        })
    }
}

fn echo_key_command(mut requests: MessageReader<CommandBarKeyRequest>, mut commands: Commands) {
    for request in requests.read() {
        commands.trigger(BinHostEmitEvent::from_event(request.caller, &request.key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::BinEvent;

    #[derive(Resource, Default)]
    struct Echoed(Vec<(Entity, String)>);

    impl Echoed {
        fn record(trigger: On<BinHostEmitEvent>, mut echoed: ResMut<Self>) {
            let decoded = rkyv::from_bytes::<CommandBarKey, rkyv::rancor::Error>(trigger.payload())
                .map(|key| format!("{key:?}"))
                .unwrap_or_else(|_| "undecodable".to_string());
            echoed
                .0
                .push((trigger.webview(), format!("{}:{decoded}", trigger.id())));
        }
    }

    struct Echo;

    impl Echo {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(KeyPlugin)
                .init_resource::<Echoed>()
                .add_observer(Echoed::record);
            app
        }

        fn issue(app: &mut App, caller: Entity, key: CommandBarKey) {
            app.world_mut()
                .resource_mut::<bevy::ecs::message::Messages<CommandBarKeyRequest>>()
                .write(CommandBarKeyRequest { caller, key });
            app.update();
        }
    }

    #[test]
    fn a_resolved_key_reaches_only_the_page_that_sent_it() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn_empty().id();
        let other = app.world_mut().spawn_empty().id();

        Echo::issue(&mut app, pressed, CommandBarKey::Next);

        assert_eq!(
            app.world().resource::<Echoed>().0,
            vec![(pressed, format!("{}:Next", CommandBarKey::id()))]
        );
        assert!(
            !app.world()
                .resource::<Echoed>()
                .0
                .iter()
                .any(|(entity, _)| *entity == other)
        );
    }
}
