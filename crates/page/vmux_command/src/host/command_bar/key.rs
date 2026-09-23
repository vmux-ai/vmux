use crate::event::CommandBarKey;
use crate::{CommandDefinition, CommandDispatch, CommandRuntimePlugin, RegisterCommandDefinitions};
use bevy::prelude::*;
use bevy_cef::prelude::BinHostEmitEvent;

pub(crate) struct KeyPlugin;

impl Plugin for KeyPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_systems(Startup, spawn_commands.in_set(RegisterCommandDefinitions))
            .add_observer(echo_key_command);
    }
}

#[derive(Component)]
struct CommandBarKeyCommand(CommandBarKey);

fn spawn_commands(mut commands: Commands) {
    for (definition, key) in [
        (
            CommandDefinition::new("command_bar_next", "Next Result", "Command Bar")
                .hidden()
                .direct_when("ArrowDown", Some("command-bar"))
                .direct_when("Ctrl+n", Some("command-bar"))
                .direct_when("Ctrl+j", Some("command-bar")),
            CommandBarKey::Next,
        ),
        (
            CommandDefinition::new("command_bar_previous", "Previous Result", "Command Bar")
                .hidden()
                .direct_when("ArrowUp", Some("command-bar"))
                .direct_when("Ctrl+p", Some("command-bar"))
                .direct_when("Ctrl+k", Some("command-bar")),
            CommandBarKey::Previous,
        ),
        (
            CommandDefinition::new("command_bar_complete", "Accept Completion", "Command Bar")
                .hidden()
                .direct_when("Tab", Some("command-bar")),
            CommandBarKey::Complete,
        ),
        (
            CommandDefinition::new("command_bar_dismiss", "Dismiss Command Bar", "Command Bar")
                .hidden()
                .direct_when("Escape", Some("command-bar"))
                .direct_when("Ctrl+c", Some("command-bar")),
            CommandBarKey::Dismiss,
        ),
    ] {
        commands.spawn((definition, CommandBarKeyCommand(key)));
    }
}

fn echo_key_command(
    trigger: On<CommandDispatch>,
    keys: Query<&CommandBarKeyCommand>,
    mut commands: Commands,
) {
    let Ok(key) = keys.get(trigger.event().command()) else {
        return;
    };
    commands.trigger(BinHostEmitEvent::from_event(
        trigger.event().invocation().caller,
        &key.0,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandInvocation;
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

        fn issue(app: &mut App, caller: Entity, id: &str) {
            app.world_mut()
                .resource_mut::<bevy::ecs::message::Messages<CommandInvocation>>()
                .write(CommandInvocation::new(caller, id));
            app.update();
        }
    }

    #[test]
    fn a_resolved_key_reaches_only_the_page_that_sent_it() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn_empty().id();
        let other = app.world_mut().spawn_empty().id();

        Echo::issue(&mut app, pressed, "command_bar_next");

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
