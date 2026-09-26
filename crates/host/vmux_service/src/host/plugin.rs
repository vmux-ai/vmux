use std::sync::Arc;

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};

use crate::client::{ServiceClient, ServiceHandle, ServiceInbound, ServiceRequest, ServiceWake};
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
pub struct ServiceConnected;

#[derive(Component)]
struct ServiceWakeCallback(Option<ServiceWake>);

pub struct ServicePlugin;

impl Plugin for ServicePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::ServicePage::plugin());
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        app.add_message::<ServiceRequest>()
            .add_message::<ServiceInbound>()
            .add_systems(Startup, start_service)
            .add_systems(Update, (connect_service, receive_service_messages).chain())
            .add_systems(Last, send_service_requests);
    }
}

fn start_service(mut commands: Commands, proxy: Option<Res<EventLoopProxyWrapper>>) {
    let wake = proxy.map(|wrapper| {
        let proxy = (**wrapper).clone();
        Arc::new(move || {
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
        }) as ServiceWake
    });
    commands.spawn((
        Name::new("vmux service"),
        ServiceConnectRetry {
            timer: Timer::from_seconds(0.05, TimerMode::Once),
            next_delay_ms: 50,
            remaining_attempts: 6,
        },
        ServiceWakeCallback(wake),
    ));
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
                .insert((ServiceClient(handle), ServiceConnected));
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

fn send_service_requests(
    mut requests: MessageReader<ServiceRequest>,
    client: Option<Single<&ServiceClient>>,
) {
    let Some(client) = client else {
        requests.clear();
        return;
    };
    for request in requests.read() {
        client.0.send(request.0.clone());
    }
}

fn receive_service_messages(
    client: Option<Single<(&ServiceClient, &ServiceWakeCallback)>>,
    mut inbound: MessageWriter<ServiceInbound>,
) {
    let Some(client) = client else {
        return;
    };
    let (messages, capped) = client.0.0.drain_with_status();
    for message in messages {
        inbound.write(ServiceInbound(message));
    }
    if capped && let Some(wake) = &client.1.0 {
        wake();
    }
}
