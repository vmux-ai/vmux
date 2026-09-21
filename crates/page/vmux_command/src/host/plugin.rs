use bevy::prelude::*;

use crate::command::{AppCommand, ReadAppCommands, WriteAppCommands};
use crate::issued::CommandIssued;
use crate::page_key::PageKeyPlugin;
use crate::snapshot::{CommandBarSnapshotPlugin, WriteCommandBarSnapshots};
use crate::surface::CommandBarSurfacePlugin;
use vmux_core::team::{Profile, User};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            PageKeyPlugin,
            CommandBarSnapshotPlugin,
            CommandBarSurfacePlugin,
        ))
        .add_message::<AppCommand>()
        .add_message::<CommandIssued>()
        .add_message::<crate::host::ExLineSubmitted>()
        .add_message::<crate::host::FileStatusPicked>()
        .configure_sets(
            Update,
            (WriteAppCommands, WriteCommandBarSnapshots, ReadAppCommands).chain(),
        )
        .init_resource::<CommandSettle>()
        .add_systems(
            Update,
            log_app_commands
                .after(WriteAppCommands)
                .before(ReadAppCommands),
        )
        .add_systems(Last, CommandSettle::keep_frames_coming);
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
