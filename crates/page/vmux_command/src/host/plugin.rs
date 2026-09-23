use bevy::prelude::*;

use crate::command::{AppCommand, ReadAppCommands, WriteAppCommands};
use crate::definition::{
    CommandDefinition, CommandInvocation, DispatchCommandInvocations, RegisterCommandDefinitions,
};
use crate::issued::CommandIssued;
use crate::page_key::KeyPlugin;
use crate::snapshot::{UiStatePlugin, WriteCommandBarSnapshots};
use crate::surface::SurfacePlugin;
use vmux_core::team::{Profile, User};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((KeyPlugin, UiStatePlugin, SurfacePlugin))
            .add_message::<AppCommand>()
            .add_message::<CommandInvocation>()
            .add_message::<CommandIssued>()
            .add_message::<crate::host::ExLineSubmitted>()
            .add_message::<crate::host::FileStatusPicked>()
            .add_systems(
                Startup,
                spawn_legacy_command_definitions.in_set(RegisterCommandDefinitions),
            )
            .configure_sets(
                Update,
                (
                    WriteAppCommands,
                    DispatchCommandInvocations,
                    WriteCommandBarSnapshots,
                    ReadAppCommands,
                )
                    .chain(),
            )
            .init_resource::<CommandSettle>()
            .add_systems(
                Update,
                (
                    dispatch_legacy_commands.in_set(DispatchCommandInvocations),
                    log_app_commands
                        .after(WriteAppCommands)
                        .before(ReadAppCommands),
                ),
            )
            .add_systems(Last, CommandSettle::keep_frames_coming);
    }
}

fn spawn_legacy_command_definitions(mut commands: Commands) {
    for (id, name, shortcut) in AppCommand::command_bar_entries() {
        let (group, label) = name
            .rsplit_once(" > ")
            .map(|(group, label)| (group.to_string(), label.to_string()))
            .unwrap_or_else(|| (String::new(), name));
        commands.spawn(CommandDefinition {
            id: id.to_string(),
            label,
            group,
            accelerator: None,
            hidden: false,
            native_menu: false,
            shortcut_label: (!shortcut.is_empty()).then(|| shortcut.to_string()),
            shortcuts: Vec::new(),
        });
    }
}

fn dispatch_legacy_commands(
    mut invocations: MessageReader<CommandInvocation>,
    mut commands: MessageWriter<AppCommand>,
    mut issued: MessageWriter<CommandIssued>,
) {
    for invocation in invocations.read() {
        let Some(command) = AppCommand::from_shortcut_id(&invocation.id) else {
            continue;
        };
        issued.write(CommandIssued {
            caller: invocation.caller,
            command: command.clone(),
        });
        commands.write(command);
    }
}

#[derive(Resource, Default)]
struct CommandSettle(Option<std::time::Instant>);

impl CommandSettle {
    const WINDOW: std::time::Duration = std::time::Duration::from_millis(150);

    fn keep_frames_coming(
        mut settle: ResMut<Self>,
        mut reader: MessageReader<CommandIssued>,
        proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    ) {
        if reader.read().count() > 0 {
            settle.0 = Some(std::time::Instant::now());
        }
        let Some(since) = settle.0 else {
            return;
        };
        if since.elapsed() >= Self::WINDOW {
            settle.0 = None;
            return;
        }
        let Some(proxy) = proxy else {
            return;
        };
        let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
    }
}

fn log_app_commands(
    mut reader: MessageReader<CommandIssued>,
    profiles: Query<(&Profile, Has<User>)>,
) {
    for ev in reader.read() {
        let who = profiles
            .get(ev.caller)
            .map(|(p, is_user)| format!("{} ({})", p.name, if is_user { "user" } else { "agent" }))
            .unwrap_or_else(|_| "unknown".to_string());
        info!(target: "vmux_command::app_command", caller = %who, cmd = ?ev.command, "AppCommand");
    }
}
