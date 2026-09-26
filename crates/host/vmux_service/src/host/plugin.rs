use std::sync::Arc;

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};

use crate::client::{ServiceClient, ServiceHandle, ServiceWake};
use crate::protocol::ClientMessage;

#[derive(Component)]
struct ServiceConnectRetry {
    timer: Timer,
    next_delay_ms: u64,
    remaining_attempts: u32,
}

#[derive(Component, Clone, Debug)]
pub struct ServiceUnavailable(pub String);

#[derive(Component)]
struct ServiceWakeCallback(Option<ServiceWake>);

pub struct ServicePlugin;

impl Plugin for ServicePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::ServicePage::plugin());
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        let wake = app
            .world()
            .get_resource::<EventLoopProxyWrapper>()
            .map(|wrapper| {
                let proxy = (**wrapper).clone();
                Arc::new(move || {
                    let _ = proxy.send_event(WinitUserEvent::WakeUp);
                }) as ServiceWake
            });
        app.world_mut().spawn((
            Name::new("vmux service"),
            ServiceConnectRetry {
                timer: Timer::from_seconds(0.05, TimerMode::Once),
                next_delay_ms: 50,
                remaining_attempts: 6,
            },
            ServiceWakeCallback(wake),
        ));
        app.add_systems(Startup, ensure_service_started)
            .add_systems(Update, connect_service);
    }
}

fn ensure_service_started() {
    if ServiceHandle::service_running() {
        tracing::info!("service already running");
        return;
    }
    let binary = match crate::DaemonBinary::current() {
        Ok(binary) => binary.into_path(),
        Err(error) => {
            tracing::error!(%error, "could not locate vmux_service binary");
            return;
        }
    };
    match crate::registry::start_mode_for(&binary) {
        crate::registry::StartMode::Register => {
            let profile = crate::ServicePaths::build_profile();
            if let Err(error) = crate::registry::ensure_running(profile, &binary) {
                tracing::error!(?error, "service registration failed");
            }
        }
        crate::registry::StartMode::SpawnDetached => {
            crate::registry::prepare_spawn_detached(&binary);
            spawn_detached_service(&binary);
        }
    }
}

#[cfg(unix)]
fn spawn_detached_service(binary: &std::path::Path) {
    use std::os::unix::process::CommandExt;

    let log_dir = crate::ServicePaths::log_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let stderr = match std::fs::File::create(crate::ServicePaths::current().log()) {
        Ok(file) => std::process::Stdio::from(file),
        Err(error) => {
            tracing::warn!(%error, "could not create service log; stderr will be discarded");
            std::process::Stdio::null()
        }
    };
    let result = unsafe {
        std::process::Command::new(binary)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(stderr)
            .pre_exec(|| {
                libc::setsid();
                Ok(())
            })
            .spawn()
    };
    if let Err(error) = result {
        tracing::error!(%error, "failed to spawn vmux_service");
    }
}

fn connect_service(
    mut runtimes: Query<
        (Entity, &mut ServiceConnectRetry, &ServiceWakeCallback),
        Without<ServiceClient>,
    >,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (entity, mut retry, wake) in &mut runtimes {
        retry.timer.tick(time.delta());
        if !retry.timer.just_finished() {
            continue;
        }
        retry.remaining_attempts = retry.remaining_attempts.saturating_sub(1);
        let socket = crate::ServicePaths::current().socket();
        if socket.exists()
            && let Some(handle) = ServiceHandle::connect_with_wake(wake.0.clone())
        {
            handle.send(ClientMessage::SubscribeAgentCommands);
            commands
                .entity(entity)
                .remove::<ServiceConnectRetry>()
                .remove::<ServiceUnavailable>()
                .insert(ServiceClient(handle));
            continue;
        }
        if retry.remaining_attempts == 0 {
            let message = "vmux service unavailable — run `vmux service logs` for details.";
            tracing::error!(message);
            commands
                .entity(entity)
                .remove::<ServiceConnectRetry>()
                .insert(ServiceUnavailable(message.to_string()));
            continue;
        }
        retry.next_delay_ms = (retry.next_delay_ms * 2).min(1600);
        retry.timer = Timer::new(
            std::time::Duration::from_millis(retry.next_delay_ms),
            TimerMode::Once,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_owns_connection_lifecycle_entity() {
        let mut app = App::new();
        app.add_plugins(ServicePlugin);

        let entity = app
            .world_mut()
            .query_filtered::<Entity, With<ServiceConnectRetry>>()
            .single(app.world())
            .unwrap();

        assert!(app.world().get::<ServiceWakeCallback>(entity).is_some());
        assert_eq!(
            app.world().get::<Name>(entity).unwrap().as_str(),
            "vmux service"
        );
    }
}
