use bevy::prelude::*;
use bevy_cef::prelude::BinHostEmitEvent;
use vmux_command::{AppCommand, CommandIssued, ReadAppCommands};
use vmux_core::event::{FILE_KEY_EVENT, FileKey};

pub(crate) struct FileKeyPlugin;

impl Plugin for FileKeyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, echo_key_command.in_set(ReadAppCommands));
    }
}

fn echo_key_command(mut issued: MessageReader<CommandIssued>, mut commands: Commands) {
    for issue in issued.read() {
        let AppCommand::File(key) = issue.command else {
            continue;
        };
        commands.trigger(BinHostEmitEvent::from_rkyv(
            issue.caller,
            FILE_KEY_EVENT,
            &FileKey::from(key),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_command::FileKeyCommand;

    #[derive(Resource, Default)]
    struct Echoed(Vec<(Entity, String)>);

    impl Echoed {
        fn record(trigger: On<BinHostEmitEvent>, mut echoed: ResMut<Self>) {
            let decoded = rkyv::from_bytes::<FileKey, rkyv::rancor::Error>(&trigger.payload)
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
                .add_plugins(FileKeyPlugin)
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
            AppCommand::File(FileKeyCommand::PanelChoose),
        );

        assert_eq!(
            app.world().resource::<Echoed>().0,
            vec![(pressed, format!("{FILE_KEY_EVENT}:PanelChoose"))]
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
    fn a_command_that_is_not_a_file_key_is_left_alone() {
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
