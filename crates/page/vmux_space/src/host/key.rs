use bevy::prelude::*;
use bevy_cef::prelude::BinHostEmitEvent;
use vmux_command::{AppCommand, CommandIssued, LayoutCommand, ReadAppCommands};
use vmux_wire::space::SPACE_KEY_EVENT;

pub(crate) struct SpaceKeyPlugin;

impl Plugin for SpaceKeyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, echo_key_command.in_set(ReadAppCommands));
    }
}

fn echo_key_command(mut issued: MessageReader<CommandIssued>, mut commands: Commands) {
    for issue in issued.read() {
        let AppCommand::Layout(LayoutCommand::Space(command)) = issue.command else {
            continue;
        };
        let Some(key) = command.key() else {
            continue;
        };
        commands.trigger(BinHostEmitEvent::from_rkyv(
            issue.caller,
            SPACE_KEY_EVENT,
            &key,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_command::SpaceCommand;
    use vmux_wire::space::SpaceKey;

    #[derive(Resource, Default)]
    struct Echoed(Vec<(Entity, String)>);

    impl Echoed {
        fn record(trigger: On<BinHostEmitEvent>, mut echoed: ResMut<Self>) {
            let decoded = rkyv::from_bytes::<SpaceKey, rkyv::rancor::Error>(&trigger.payload)
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
                .add_plugins(SpaceKeyPlugin)
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

    #[test]
    fn a_resolved_key_reaches_only_the_page_that_sent_it() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn_empty().id();
        let other = app.world_mut().spawn_empty().id();

        Echo::issue(
            &mut app,
            pressed,
            AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Delete)),
        );

        assert_eq!(
            app.world().resource::<Echoed>().0,
            vec![(pressed, format!("{SPACE_KEY_EVENT}:Delete"))]
        );
        assert!(
            !app.world()
                .resource::<Echoed>()
                .0
                .iter()
                .any(|(entity, _)| *entity == other)
        );
    }

    #[test]
    fn opening_the_page_is_not_echoed_to_it() {
        let mut app = Echo::app();
        let caller = app.world_mut().spawn_empty().id();

        Echo::issue(
            &mut app,
            caller,
            AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open)),
        );

        assert!(app.world().resource::<Echoed>().0.is_empty());
    }

    #[test]
    fn a_command_that_is_not_a_space_key_is_left_alone() {
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
