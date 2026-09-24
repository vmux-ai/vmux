use bevy::prelude::*;
use vmux_chat::event::ChatKey;
use vmux_command::{
    CommandDispatch, CommandManifest, CommandRuntimePlugin, RegisterCommandDefinitions,
};

pub(crate) struct ChatKeyPlugin;

impl Plugin for ChatKeyPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_systems(Startup, spawn_commands.in_set(RegisterCommandDefinitions))
            .add_observer(echo_key_command);
    }
}

#[derive(Component)]
struct ChatKeyBinding(ChatKey);

fn spawn_commands(mut commands: Commands) {
    let manifest = CommandManifest::<ChatKey>::from_ron(include_str!("key.ron"));
    for (definition, key) in manifest.into_commands() {
        commands.spawn((definition, ChatKeyBinding(key)));
    }
}

fn echo_key_command(
    trigger: On<CommandDispatch>,
    keys: Query<&ChatKeyBinding>,
    mut commands: Commands,
) {
    let Ok(key) = keys.get(trigger.event().command()) else {
        return;
    };
    commands.trigger(
        vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
            trigger.event().invocation().caller,
            &key.0,
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_chat::state::{ChatUiState, ChatUiStatePatch};
    use vmux_command::CommandInvocation;
    use vmux_core::host::UiStateWrite;

    #[derive(Resource, Default)]
    struct Echoed(Vec<(Entity, ChatKey)>);

    impl Echoed {
        fn record(trigger: On<UiStateWrite<ChatUiState>>, mut echoed: ResMut<Self>) {
            let ChatUiStatePatch::Key(key) = trigger.event().patch() else {
                return;
            };
            echoed.0.push((trigger.event().webview(), *key));
        }
    }

    struct Echo;

    impl Echo {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(ChatKeyPlugin)
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

        Echo::issue(&mut app, pressed, "chat_list_choose");

        assert_eq!(
            app.world().resource::<Echoed>().0,
            vec![(pressed, ChatKey::ListChoose)]
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
