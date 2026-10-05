use std::time::{Duration, Instant};

use bevy::prelude::*;
use vmux_ecs::ProcessId;
use vmux_ecs::page::PageReady;
use vmux_ecs::service::ServiceMessageSet;

use crate::Terminal;
use crate::event::TermLoadingEvent;

use super::plugin::ShellOutputSeen;

pub(crate) struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                arm_shell,
                arm_shell_on_restart,
                announce_slow_shell_boot.after(ServiceMessageSet),
                clear_shell.after(ServiceMessageSet),
                set_shell_icon,
            ),
        );
    }
}

const SHELL_LOADING_TIMEOUT: Duration = Duration::from_secs(10);
const SHELL_BOOT_GRACE: Duration = Duration::from_millis(250);

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct ShellLoading {
    pub(crate) since: Instant,
    pub(crate) announced: bool,
}

fn set_shell_icon(
    mut terminals: Query<
        (&crate::launch::TerminalLaunch, &mut vmux_ecs::PageMetadata),
        With<Terminal>,
    >,
) {
    for (launch, mut metadata) in &mut terminals {
        if !metadata.icon.is_none() {
            continue;
        }
        if let Some(icon) = vmux_api::BuiltinIcon::for_shell(&launch.command) {
            metadata.icon = vmux_api::PageIcon::Builtin(icon);
        }
    }
}

fn arm_shell(
    newly_ready: Query<(Entity, Has<ShellOutputSeen>), (With<Terminal>, Added<PageReady>)>,
    mut commands: Commands,
) {
    for (entity, output_seen) in &newly_ready {
        if output_seen {
            continue;
        }
        commands.entity(entity).insert(ShellLoading {
            since: Instant::now(),
            announced: false,
        });
    }
}

fn announce_slow_shell_boot(
    mut waiting: Query<(Entity, &mut ShellLoading), (With<Terminal>, Without<ShellOutputSeen>)>,
    mut commands: Commands,
) {
    for (entity, mut loading) in &mut waiting {
        if loading.announced || loading.since.elapsed() < SHELL_BOOT_GRACE {
            continue;
        }
        loading.announced = true;
        commands.trigger(
            vmux_ecs::UiStateWrite::<vmux_ecs::event::TerminalUiState>::from_event(
                entity,
                &TermLoadingEvent {
                    loading: true,
                    label: "Terminal".to_string(),
                    segment: "terminal".to_string(),
                },
            ),
        );
    }
}

fn arm_shell_on_restart(
    restarted: Query<
        Entity,
        (
            With<Terminal>,
            With<PageReady>,
            Without<ShellLoading>,
            Changed<ProcessId>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in &restarted {
        commands.entity(entity).insert(ShellLoading {
            since: Instant::now(),
            announced: false,
        });
    }
}

fn clear_shell(
    loading: Query<(Entity, &ShellLoading, Has<ShellOutputSeen>), With<Terminal>>,
    mut commands: Commands,
) {
    for (entity, state, output_seen) in &loading {
        if !output_seen && state.since.elapsed() < SHELL_LOADING_TIMEOUT {
            continue;
        }
        commands.entity(entity).remove::<ShellLoading>();
        if !state.announced {
            continue;
        }
        commands.trigger(
            vmux_ecs::UiStateWrite::<vmux_ecs::event::TerminalUiState>::from_event(
                entity,
                &TermLoadingEvent {
                    loading: false,
                    label: "Terminal".to_string(),
                    segment: "terminal".to_string(),
                },
            ),
        );
    }
}
