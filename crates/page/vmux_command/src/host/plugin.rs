use bevy::prelude::*;

use crate::definition::{
    CommandInvocation, CommandRuntimePlugin, DispatchCommandInvocations, WriteCommandRequests,
};
use crate::page_key::KeyPlugin;
use crate::snapshot::UiStatePlugin;
use crate::surface::SurfacePlugin;
use vmux_core::team::{Profile, User};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_plugins((
            KeyPlugin,
            UiStatePlugin,
            SurfacePlugin,
            crate::CommandToolPlugin,
        ))
        .add_message::<crate::host::ExLineSubmitted>()
        .add_message::<crate::host::FileStatusPicked>()
        .add_systems(
            Update,
            log_command_invocations
                .after(WriteCommandRequests)
                .before(DispatchCommandInvocations),
        )
        .add_systems(Last, keep_frames_coming);
    }
}

const COMMAND_SETTLE_WINDOW: std::time::Duration = std::time::Duration::from_millis(150);

fn keep_frames_coming(
    mut settle: Local<Option<std::time::Instant>>,
    mut reader: MessageReader<CommandInvocation>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    if reader.read().count() > 0 {
        *settle = Some(std::time::Instant::now());
    }
    let Some(since) = *settle else {
        return;
    };
    if since.elapsed() >= COMMAND_SETTLE_WINDOW {
        *settle = None;
        return;
    }
    let Some(proxy) = proxy else {
        return;
    };
    let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
}

fn log_command_invocations(
    mut reader: MessageReader<CommandInvocation>,
    profiles: Query<(&Profile, Has<User>)>,
) {
    for invocation in reader.read() {
        let who = profiles
            .get(invocation.caller)
            .map(|(p, is_user)| format!("{} ({})", p.name, if is_user { "user" } else { "agent" }))
            .unwrap_or_else(|_| "unknown".to_string());
        info!(
            target: "vmux_command::invocation",
            caller = %who,
            id = %invocation.id,
            "CommandInvocation"
        );
    }
}
