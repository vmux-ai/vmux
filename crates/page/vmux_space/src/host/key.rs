use bevy::prelude::*;
use bevy_cef::prelude::BinHostEmitEvent;
use vmux_api::space::SpaceKey;
use vmux_command::{
    CommandDefinition, CommandDispatch, CommandRuntimePlugin, RegisterCommandDefinitions,
};

pub(crate) struct SpaceKeyPlugin;

impl Plugin for SpaceKeyPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_systems(Startup, spawn_commands.in_set(RegisterCommandDefinitions))
            .add_observer(echo_key_command);
    }
}

#[derive(Component)]
struct SpaceKeyBinding(SpaceKey);

fn spawn_commands(mut commands: Commands) {
    for (definition, key) in [
        (
            CommandDefinition::new("space_next", "Next Space", "Layout > Space")
                .hidden()
                .direct_when("ArrowDown", Some("spaces"))
                .direct_when("Ctrl+n", Some("spaces"))
                .direct_when("Ctrl+j", Some("spaces")),
            SpaceKey::Next,
        ),
        (
            CommandDefinition::new("space_previous", "Previous Space", "Layout > Space")
                .hidden()
                .direct_when("ArrowUp", Some("spaces"))
                .direct_when("Ctrl+p", Some("spaces"))
                .direct_when("Ctrl+k", Some("spaces")),
            SpaceKey::Previous,
        ),
        (
            CommandDefinition::new("space_attach", "Open Selected Space", "Layout > Space")
                .hidden()
                .direct_when("Enter", Some("spaces")),
            SpaceKey::Attach,
        ),
        (
            CommandDefinition::new("space_delete", "Delete Selected Space", "Layout > Space")
                .hidden()
                .direct_when("Delete", Some("spaces"))
                .direct_when("Backspace", Some("spaces")),
            SpaceKey::Delete,
        ),
    ] {
        commands.spawn((definition, SpaceKeyBinding(key)));
    }
}

fn echo_key_command(
    trigger: On<CommandDispatch>,
    keys: Query<&SpaceKeyBinding>,
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
    use vmux_api::BinEvent;
    use vmux_command::CommandInvocation;

    #[derive(Resource, Default)]
    struct Echoed(Vec<(Entity, String)>);

    impl Echoed {
        fn record(trigger: On<BinHostEmitEvent>, mut echoed: ResMut<Self>) {
            let decoded = rkyv::from_bytes::<SpaceKey, rkyv::rancor::Error>(trigger.payload())
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
                .add_plugins(SpaceKeyPlugin)
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

        Echo::issue(&mut app, pressed, "space_delete");

        assert_eq!(
            app.world().resource::<Echoed>().0,
            vec![(pressed, format!("{}:Delete", SpaceKey::id()))]
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
