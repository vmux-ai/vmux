use std::collections::VecDeque;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};

use crate::DaemonBinary;
#[cfg(target_os = "macos")]
use crate::registry::RegistrationStep;
use crate::registry::{Backend, RegistrationError};
use vmux_api::protocol::ClientMessage;
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::service::{
    Restarting, ServiceConnected, ServiceInbound, ServiceMessageIngressSet, ServiceRequest,
    ServiceUnavailable, ShutdownSet,
};

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

#[derive(Resource)]
struct Lifetime {
    socket: std::path::PathBuf,
    lock: std::path::PathBuf,
    child: Option<std::process::Child>,
    launched: bool,
    #[cfg(target_os = "macos")]
    managed: Option<crate::LaunchAgent>,
}

impl Default for Lifetime {
    fn default() -> Self {
        let paths = crate::ServicePaths::current();
        Self {
            socket: paths.socket(),
            lock: paths.daemon_lock(),
            child: None,
            launched: false,
            #[cfg(target_os = "macos")]
            managed: if crate::ServicePaths::build_profile() == "release"
                && !vmux_profile::SessionEnvironment::requires_inherited_service_environment()
            {
                Some(crate::LaunchAgent::for_profile("release"))
            } else {
                None
            },
        }
    }
}

impl Lifetime {
    fn stop(&mut self) -> std::io::Result<()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while self.launched && (!self.socket.exists() || !self.lock.exists()) {
            if let Some(child) = &mut self.child
                && matches!(child.try_wait(), Ok(Some(_)))
            {
                self.child = None;
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let ready = self.socket.exists() && self.lock.exists();
        let result = if self.launched && !ready {
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Vmux service did not finish startup before shutdown",
            ))
        } else {
            crate::RuntimeHandoff::shutdown_runtime(
                &self.socket,
                &self.lock,
                deadline.saturating_duration_since(std::time::Instant::now()),
            )
        };
        #[cfg(target_os = "macos")]
        let result = if self.child.is_none()
            && let Some(agent) = &self.managed
            && let Err(error) = &result
            && matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::BrokenPipe
            ) {
            agent.bootout()?;
            crate::RuntimeHandoff::shutdown_runtime(
                &self.socket,
                &self.lock,
                std::time::Duration::from_secs(3),
            )
        } else {
            result
        };
        if let Some(mut child) = self.child.take() {
            while matches!(child.try_wait(), Ok(None)) && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            if !matches!(child.try_wait(), Ok(Some(_))) {
                let _ = child.kill();
            }
            if let Err(error) = child.wait()
                && result.is_ok()
            {
                return Err(error);
            }
        }
        result
    }
}

impl Drop for Lifetime {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            let _ = vmux_spawn::Reaper::start(child);
        }
    }
}

impl Plugin for ServicePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .init_resource::<Lifetime>()
            .add_message::<ServiceRequest>()
            .add_message::<ServiceInbound>()
            .add_systems(
                Startup,
                (start, ApplyDeferred, register, launch_detached).chain(),
            )
            .add_systems(
                Update,
                (
                    receive_messages.in_set(ServiceMessageIngressSet),
                    reconnect_disconnected,
                    finish_connection,
                    start_connection,
                )
                    .chain(),
            )
            .add_systems(
                Last,
                (queue_requests, send_requests).chain().before(ShutdownSet),
            )
            .add_systems(Last, shutdown.in_set(ShutdownSet));
    }
}

fn shutdown(
    mut exits: MessageReader<AppExit>,
    restarting: Option<Res<Restarting>>,
    mut lifetime: ResMut<Lifetime>,
) {
    if exits.read().count() == 0 || restarting.is_some() {
        return;
    }
    if let Err(error) = lifetime.stop() {
        tracing::error!(%error, "application service shutdown failed");
    }
}

fn start(mut commands: Commands, proxy: Option<Res<EventLoopProxyWrapper>>) {
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

fn register(
    registrations: Query<(Entity, &DaemonBinary, &ServiceRegistration)>,
    mut commands: Commands,
    mut lifetime: ResMut<Lifetime>,
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
                        if let Err(error) = crate::sm_app_service::MainAppService::unregister() {
                            tracing::debug!(
                                %error,
                                "unregister main app login item (ignored)"
                            );
                        }
                        Ok(())
                    }
                    RegistrationStep::UnregisterEmbeddedAgent => {
                        if let Err(error) = crate::sm_app_service::AgentService::new(
                            crate::bundle::EMBEDDED_AGENT_PLIST,
                        )
                        .unregister()
                        {
                            tracing::debug!(%error, "unregister embedded agent (ignored)");
                        }
                        Ok(())
                    }
                    RegistrationStep::RegisterEmbeddedAgent => {
                        crate::sm_app_service::AgentService::new(
                            crate::bundle::EMBEDDED_AGENT_PLIST,
                        )
                        .register()
                        .map_err(RegistrationError::from)
                    }
                    RegistrationStep::KickstartEmbeddedAgent => {
                        crate::LaunchAgent::for_profile("release")
                            .kickstart()
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
            let _ = (binary, &registration.0);
            Ok(())
        };
        let mut service = commands.entity(entity);
        service.remove::<ServiceRegistration>();
        match result {
            Ok(()) => {
                lifetime.launched = true;
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
fn launch_detached(
    launches: Query<(Entity, &DaemonBinary), With<DetachedServiceLaunch>>,
    mut commands: Commands,
    mut lifetime: ResMut<Lifetime>,
) {
    for (entity, binary) in &launches {
        binary.prepare_detached_spawn();
        let result = binary.spawn_detached();
        let mut service = commands.entity(entity);
        service.remove::<DetachedServiceLaunch>();
        match result {
            Ok(child) => {
                lifetime.child = Some(child);
                lifetime.launched = true;
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

type ConnectRuntime = (
    Entity,
    &'static mut ServiceConnectRetry,
    &'static ServiceWakeCallback,
);
type DisconnectedService = (Without<ServiceClient>, Without<ServiceConnectTask>);

fn start_connection(
    mut runtimes: Query<ConnectRuntime, DisconnectedService>,
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

fn finish_connection(
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

fn queue_requests(
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

fn send_requests(
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

fn receive_messages(
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

fn reconnect_disconnected(
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
    fn quitting_during_daemon_startup_waits_for_and_stops_the_owned_child() {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("own.sock");
        let lock = directory.path().join("own.lock");
        let child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut app = App::new();
        app.add_plugins(ServicePlugin).add_message::<AppExit>();
        app.insert_resource(Lifetime {
            socket: socket.clone(),
            lock: lock.clone(),
            child: Some(child),
            launched: true,
            #[cfg(target_os = "macos")]
            managed: None,
        });
        let server = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let lease = std::fs::File::create(lock).unwrap();
            lease.lock().unwrap();
            let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            let payload = vmux_transport::service::ServiceCodec::read_raw_blocking(&mut stream)
                .unwrap()
                .unwrap();
            assert!(matches!(
                rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&payload).unwrap(),
                ClientMessage::Shutdown
            ));
            assert_eq!(unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) }, 0);
        });
        app.world_mut().write_message(AppExit::Success);
        app.world_mut().run_schedule(Last);
        server.join().unwrap();
        assert!(app.world().resource::<Lifetime>().child.is_none());
        assert_eq!(unsafe { libc::kill(pid as libc::pid_t, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }

    #[test]
    fn final_application_frame_stops_its_service_while_restart_preserves_it() {
        for restarting in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let socket = directory.path().join("own.sock");
            let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
            if restarting {
                listener.set_nonblocking(true).unwrap();
            }
            let server = std::thread::spawn(move || {
                if restarting {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    assert!(
                        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
                    );
                } else {
                    let (mut stream, _) = listener.accept().unwrap();
                    let payload =
                        vmux_transport::service::ServiceCodec::read_raw_blocking(&mut stream)
                            .unwrap()
                            .unwrap();
                    let message =
                        rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&payload).unwrap();
                    assert!(matches!(message, ClientMessage::Shutdown));
                }
            });
            let mut app = App::new();
            app.add_plugins(ServicePlugin).add_message::<AppExit>();
            app.insert_resource(Lifetime {
                socket,
                lock: directory.path().join("own.lock"),
                child: None,
                launched: false,
                #[cfg(target_os = "macos")]
                managed: None,
            });
            if restarting {
                app.insert_resource(Restarting);
            }
            app.world_mut().write_message(AppExit::Success);
            app.world_mut().run_schedule(Last);
            server.join().unwrap();
        }
    }

    #[test]
    fn disconnected_service_retries_without_dropping_requests() {
        let mut app = App::new();
        app.add_message::<ServiceRequest>()
            .add_message::<ServiceInbound>()
            .add_systems(Update, (receive_messages, reconnect_disconnected).chain())
            .add_systems(Last, (queue_requests, send_requests).chain());
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
