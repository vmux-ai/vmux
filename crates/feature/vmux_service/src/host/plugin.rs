use std::collections::VecDeque;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};

use crate::DaemonBinary;
use crate::client::{ServiceClient, ServiceHandle, ServiceInbound, ServiceRequest, ServiceWake};
use crate::registry::Backend;
use vmux_api::protocol::ClientMessage;

#[derive(Component)]
struct ServiceConnectRetry {
    timer: Timer,
    next_delay_ms: u64,
    remaining_attempts: u32,
    reported_unavailable: bool,
}

impl Default for ServiceConnectRetry {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.05, TimerMode::Once),
            next_delay_ms: 50,
            remaining_attempts: 6,
            reported_unavailable: false,
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct ServiceUnavailable(pub String);

#[derive(Component)]
pub struct ServiceConnected;

#[derive(Component)]
struct ServiceWakeCallback(Option<ServiceWake>);

#[derive(Component, Default)]
struct PendingServiceRequests(VecDeque<ClientMessage>);

#[derive(Component)]
struct ServiceDisconnected;

#[derive(Component)]
struct ServiceRegistration(Backend);

#[derive(Component)]
struct DetachedServiceLaunch;

#[derive(Component)]
struct ServiceLaunchCompleted;

pub struct ServicePlugin;

impl Plugin for ServicePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::ServicePage::plugin());
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        app.add_message::<ServiceRequest>()
            .add_message::<ServiceInbound>()
            .add_systems(
                Startup,
                (
                    start_service,
                    ApplyDeferred,
                    register_service,
                    launch_detached_service,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    receive_service_messages,
                    reconnect_disconnected_service,
                    connect_service,
                )
                    .chain(),
            )
            .add_systems(
                Last,
                (queue_service_requests, send_service_requests).chain(),
            );
    }
}

fn start_service(mut commands: Commands, proxy: Option<Res<EventLoopProxyWrapper>>) {
    let wake = proxy.map(|wrapper| {
        let proxy = (**wrapper).clone();
        Arc::new(move || {
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
        }) as ServiceWake
    });
    let mut service = commands.spawn((
        Name::new("vmux service"),
        ServiceConnectRetry::default(),
        ServiceWakeCallback(wake),
        PendingServiceRequests::default(),
    ));
    if ServiceHandle::service_running() {
        tracing::info!("service already running");
        return;
    }
    let binary = match DaemonBinary::current() {
        Ok(binary) => binary,
        Err(error) => {
            tracing::error!(%error, "could not locate vmux_service binary");
            service.insert(ServiceUnavailable(error.to_string()));
            return;
        }
    };
    if binary.requires_registration(crate::ServicePaths::build_profile()) {
        let backend = Backend::for_binary(&binary);
        service.insert((binary, ServiceRegistration(backend)));
    } else {
        service.insert((binary, DetachedServiceLaunch));
    }
}

fn register_service(
    registrations: Query<(Entity, &DaemonBinary, &ServiceRegistration)>,
    mut commands: Commands,
) {
    for (entity, binary, registration) in &registrations {
        let result = registration
            .0
            .ensure_running(crate::ServicePaths::build_profile(), binary);
        let mut service = commands.entity(entity);
        service.remove::<ServiceRegistration>();
        match result {
            Ok(()) => {
                service.insert(ServiceLaunchCompleted);
            }
            Err(error) => {
                tracing::error!(?error, "service registration failed");
                service.insert(ServiceUnavailable(format!(
                    "service registration failed: {error:?}"
                )));
            }
        }
    }
}

#[cfg(unix)]
fn launch_detached_service(
    launches: Query<(Entity, &DaemonBinary), With<DetachedServiceLaunch>>,
    mut commands: Commands,
) {
    for (entity, binary) in &launches {
        binary.prepare_detached_spawn();
        let result = binary.spawn_detached();
        let mut service = commands.entity(entity);
        service.remove::<DetachedServiceLaunch>();
        match result {
            Ok(()) => {
                service.insert(ServiceLaunchCompleted);
            }
            Err(error) => {
                tracing::error!(%error, "failed to spawn vmux_service");
                service.insert(ServiceUnavailable(format!(
                    "failed to spawn vmux_service: {error}"
                )));
            }
        }
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
        let socket = crate::ServicePaths::current().socket();
        if socket.exists()
            && let Some(handle) = ServiceHandle::connect_with_wake(wake.0.clone())
            && handle.send(ClientMessage::SubscribeAgentCommands)
        {
            commands
                .entity(entity)
                .remove::<ServiceConnectRetry>()
                .remove::<ServiceUnavailable>()
                .insert((ServiceClient(handle), ServiceConnected));
            continue;
        }
        retry.remaining_attempts = retry.remaining_attempts.saturating_sub(1);
        if retry.remaining_attempts == 0 && !retry.reported_unavailable {
            let message = "vmux service unavailable — run `vmux service logs` for details.";
            tracing::error!(message);
            retry.reported_unavailable = true;
            commands
                .entity(entity)
                .insert(ServiceUnavailable(message.to_string()));
        }
        retry.next_delay_ms = (retry.next_delay_ms * 2).min(1600);
        retry.timer = Timer::new(
            std::time::Duration::from_millis(retry.next_delay_ms),
            TimerMode::Once,
        );
    }
}

fn queue_service_requests(
    mut requests: MessageReader<ServiceRequest>,
    mut runtimes: Query<&mut PendingServiceRequests>,
) {
    let Ok(mut pending) = runtimes.single_mut() else {
        requests.clear();
        return;
    };
    for request in requests.read() {
        pending.0.push_back(request.0.clone());
    }
}

fn send_service_requests(
    mut runtimes: Query<
        (Entity, &ServiceClient, &mut PendingServiceRequests),
        Without<ServiceDisconnected>,
    >,
    mut commands: Commands,
) {
    let Ok((entity, client, mut pending)) = runtimes.single_mut() else {
        return;
    };
    while let Some(message) = pending.0.front().cloned() {
        if !client.0.send(message) {
            commands.entity(entity).insert(ServiceDisconnected);
            return;
        }
        pending.0.pop_front();
    }
}

fn receive_service_messages(
    clients: Query<(Entity, &ServiceClient, &ServiceWakeCallback), Without<ServiceDisconnected>>,
    mut inbound: MessageWriter<ServiceInbound>,
    mut commands: Commands,
) {
    let Ok((entity, client, wake)) = clients.single() else {
        return;
    };
    let drained = client.0.drain_with_status();
    for message in drained.messages {
        inbound.write(ServiceInbound(message));
    }
    if drained.disconnected {
        commands.entity(entity).insert(ServiceDisconnected);
    }
    if drained.capped
        && let Some(wake) = &wake.0
    {
        wake();
    }
}

fn reconnect_disconnected_service(
    disconnected: Query<Entity, Added<ServiceDisconnected>>,
    mut commands: Commands,
) {
    for entity in &disconnected {
        commands
            .entity(entity)
            .remove::<ServiceClient>()
            .remove::<ServiceConnected>()
            .remove::<ServiceUnavailable>()
            .remove::<ServiceDisconnected>()
            .insert(ServiceConnectRetry::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnected_service_retries_without_dropping_requests() {
        let mut app = App::new();
        app.add_message::<ServiceRequest>()
            .add_message::<ServiceInbound>()
            .add_systems(
                Update,
                (receive_service_messages, reconnect_disconnected_service).chain(),
            )
            .add_systems(
                Last,
                (queue_service_requests, send_service_requests).chain(),
            );
        let runtime = app
            .world_mut()
            .spawn((
                ServiceClient(ServiceHandle::disconnected()),
                ServiceConnected,
                ServiceWakeCallback(None),
                PendingServiceRequests::default(),
            ))
            .id();
        app.world_mut()
            .write_message(ServiceRequest(ClientMessage::Shutdown));

        app.update();

        assert!(app.world().get::<ServiceClient>(runtime).is_none());
        assert!(app.world().get::<ServiceConnected>(runtime).is_none());
        assert!(app.world().get::<ServiceConnectRetry>(runtime).is_some());
        assert_eq!(
            app.world()
                .get::<PendingServiceRequests>(runtime)
                .expect("request queue should remain")
                .0
                .len(),
            1
        );
    }
}
