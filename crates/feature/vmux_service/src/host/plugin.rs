use std::collections::VecDeque;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};

use crate::DaemonBinary;
#[cfg(target_os = "macos")]
use crate::registry::RegistrationStep;
use crate::registry::{Backend, RegistrationError};
use vmux_api::protocol::ClientMessage;
use vmux_core::service::{ServiceConnected, ServiceInbound, ServiceRequest, ServiceUnavailable};

use super::client::{ServiceClient, ServiceHandle, ServiceWake};

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

impl ServiceConnectRetry {
    fn failed(&mut self) -> Option<&'static str> {
        self.remaining_attempts = self.remaining_attempts.saturating_sub(1);
        let message = if self.remaining_attempts == 0 && !self.reported_unavailable {
            self.reported_unavailable = true;
            Some("vmux service unavailable — run `vmux service logs` for details.")
        } else {
            None
        };
        self.next_delay_ms = (self.next_delay_ms * 2).min(1600);
        self.timer = Timer::new(
            std::time::Duration::from_millis(self.next_delay_ms),
            TimerMode::Once,
        );
        message
    }
}

#[derive(Component)]
struct ServiceWakeCallback(Option<ServiceWake>);

#[derive(Component)]
struct ServiceConnectTask(crossbeam_channel::Receiver<Option<ServiceHandle>>);

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
                    finish_service_connection,
                    start_service_connection,
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
        #[cfg(target_os = "macos")]
        let result = {
            let mut result = Ok(());
            for step in registration.0.registration_steps() {
                let step_result = match step {
                    RegistrationStep::CleanupLegacy => {
                        match crate::cleanup::LegacyRegistrations::current()
                            .and_then(crate::cleanup::LegacyRegistrations::cleanup)
                        {
                            Ok(0) => {}
                            Ok(count) => {
                                tracing::info!(removed = count, "removed legacy launchd plists")
                            }
                            Err(error) => tracing::warn!(
                                %error,
                                "legacy plist cleanup failed (continuing)"
                            ),
                        }
                        Ok(())
                    }
                    RegistrationStep::UnregisterMainApp => {
                        if let Err(error) = crate::sm_app_service::unregister_main_app() {
                            tracing::debug!(
                                %error,
                                "unregister main app login item (ignored)"
                            );
                        }
                        Ok(())
                    }
                    RegistrationStep::UnregisterEmbeddedAgent => {
                        if let Err(error) = crate::sm_app_service::unregister_agent(
                            crate::bundle::EMBEDDED_AGENT_PLIST,
                        ) {
                            tracing::debug!(%error, "unregister embedded agent (ignored)");
                        }
                        Ok(())
                    }
                    RegistrationStep::RegisterEmbeddedAgent => {
                        crate::sm_app_service::register_agent(crate::bundle::EMBEDDED_AGENT_PLIST)
                            .map_err(RegistrationError::from)
                    }
                    RegistrationStep::KickstartEmbeddedAgent => {
                        crate::launchd::kickstart(crate::bundle::EMBEDDED_AGENT_LABEL)
                            .map_err(RegistrationError::from)
                    }
                    RegistrationStep::EnsureLaunchAgent => {
                        crate::LaunchAgent::for_profile(crate::ServicePaths::build_profile())
                            .ensure_running(binary.path())
                            .map_err(RegistrationError::from)
                    }
                };
                if step_result.is_err() {
                    result = step_result;
                    break;
                }
            }
            result
        };
        #[cfg(not(target_os = "macos"))]
        let result: Result<(), RegistrationError> = {
            let _ = (binary, registration);
            Ok(())
        };
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

fn start_service_connection(
    mut runtimes: Query<
        (Entity, &mut ServiceConnectRetry, &ServiceWakeCallback),
        (Without<ServiceClient>, Without<ServiceConnectTask>),
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
        if socket.exists() {
            let (sender, receiver) = crossbeam_channel::bounded(1);
            let wake = wake.0.clone();
            let started = std::thread::Builder::new()
                .name("service-connect-worker".into())
                .spawn(move || {
                    let _ = sender.send(ServiceHandle::connect(wake));
                });
            if started.is_ok() {
                commands.entity(entity).insert(ServiceConnectTask(receiver));
                continue;
            }
        }
        if let Some(message) = retry.failed() {
            tracing::error!(message);
            commands
                .entity(entity)
                .insert(ServiceUnavailable(message.to_string()));
        }
    }
}

fn finish_service_connection(
    mut tasks: Query<(Entity, &mut ServiceConnectRetry, &ServiceConnectTask)>,
    mut commands: Commands,
) {
    for (entity, mut retry, task) in &mut tasks {
        match task.0.try_recv() {
            Ok(Some(handle)) => {
                commands
                    .entity(entity)
                    .remove::<ServiceConnectTask>()
                    .remove::<ServiceConnectRetry>()
                    .remove::<ServiceUnavailable>()
                    .insert((ServiceClient(handle), ServiceConnected));
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Ok(None) | Err(crossbeam_channel::TryRecvError::Disconnected) => {
                let mut service = commands.entity(entity);
                service.remove::<ServiceConnectTask>();
                if let Some(message) = retry.failed() {
                    tracing::error!(message);
                    service.insert(ServiceUnavailable(message.to_string()));
                }
            }
        }
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
