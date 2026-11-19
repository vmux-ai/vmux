use std::path::Path;
use std::time::{Duration, Instant};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::UiInput;
use vmux_api::remote::{
    Live, RemoteCopyEvent, RemoteDevice, RemotePairingDismissRequest, RemotePairingShowRequest,
    RemotePhase, RemoteProjectionSet, RemoteRequest, RemoteRevokeRequest, UiRemote,
};
use vmux_ecs::service::{
    ServiceConnected, ServiceInbound, ServiceMessageIngressSet, ServiceRequest,
};
use vmux_ecs::{CreatedAt, Order};
use vmux_feature::flag::Decision;
use vmux_service::{RelayToken, RemoteAuthorizationStore, RemotePaths};

use std::os::unix::fs::PermissionsExt;

pub struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::RemoteDomainPlugin>() {
            app.add_plugins(crate::RemoteDomainPlugin);
        }
        app.add_message::<EnableRequest>()
            .add_message::<DisableRequest>()
            .add_message::<ServiceInbound>()
            .add_message::<ServiceRequest>()
            .add_observer(reconcile_devices)
            .add_observer(request)
            .add_observer(show_pairing)
            .add_observer(dismiss_pairing)
            .add_observer(copy)
            .add_observer(revoke)
            .add_systems(Startup, spawn_runtime)
            .add_systems(
                Update,
                (
                    subscribe,
                    observe,
                    reconcile,
                    begin_enable,
                    disable,
                    poll_enable,
                    poll_registration,
                    poll_authorizations,
                    expire_pairing,
                )
                    .chain()
                    .after(crate::RemoteDomainSet::Publish)
                    .after(ServiceMessageIngressSet),
            )
            .add_systems(PostUpdate, project.in_set(RemoteProjectionSet));
    }
}

fn spawn_runtime(mut commands: Commands) {
    let authorizations = RemoteAuthorizationStore::current();
    let devices = authorizations.devices().unwrap_or_default();
    let persisted = std::fs::read_to_string(RemotePaths::current().state()).ok();
    let enabled = persisted.as_deref().map(str::trim) == Some("enabled");
    let reconcile_on_startup = persisted.is_some();
    let entity = commands
        .spawn((
            Name::new("Remote runtime"),
            RemoteState {
                enabled,
                phase: if reconcile_on_startup {
                    RemotePhase::Starting
                } else {
                    RemotePhase::Disabled
                },
                error: String::new(),
                operation_generation: 0,
                authorization_checked_at: Instant::now(),
                reconcile_on_startup,
            },
            PairingVisibility::default(),
            ObservedAvailability(false),
            authorizations,
        ))
        .id();
    commands.trigger(AuthorizedSnapshot { entity, devices });
}

fn copy(_trigger: On<UiInput<RemoteCopyEvent>>, state: Query<(&RemoteState, &RemotePairingInfo)>) {
    let Ok((state, pairing)) = state.single() else {
        return;
    };
    if state.phase == RemotePhase::Enabled {
        vmux_clipboard::Clipboard::write(pairing.pairing_url.clone());
    }
}

fn revoke(
    trigger: On<UiInput<RemoteRevokeRequest>>,
    mut states: Query<(Entity, &mut RemoteState, &RemoteAuthorizationStore)>,
    devices: Query<(Entity, &Name, &ChildOf), With<Authorized>>,
    mut commands: Commands,
) {
    let Ok((runtime, mut state, authorizations)) = states.single_mut() else {
        return;
    };
    let client_id = vmux_service::DeviceId::new(&trigger.event().payload.client_id);
    match authorizations.revoke(&client_id) {
        Ok(true) => {
            for (entity, device, owner) in &devices {
                if owner.parent() == runtime && device.as_str() == client_id.as_str() {
                    commands.entity(entity).despawn();
                }
            }
            state.error.clear();
        }
        Ok(false) => {}
        Err(error) => {
            state.error = error.to_string();
        }
    }
}

#[derive(Component)]
struct Authorized;

#[derive(EntityEvent)]
struct AuthorizedSnapshot {
    entity: Entity,
    devices: Vec<vmux_service::AuthorizedDevice>,
}

#[derive(SystemParam)]
struct Devices<'w, 's> {
    devices: Query<'w, 's, (&'static Name, &'static ChildOf, &'static Order), With<Authorized>>,
}

impl Devices<'_, '_> {
    fn paired(&self, runtime: Entity) -> bool {
        self.devices
            .iter()
            .any(|(_, owner, _)| owner.parent() == runtime)
    }

    fn rows(&self, runtime: Entity) -> Vec<RemoteDevice> {
        let mut devices = Vec::new();
        for (device, owner, order) in &self.devices {
            if owner.parent() == runtime {
                devices.push((order.0, device.as_str()));
            }
        }
        devices.sort_by_key(|(order, _)| *order);
        let mut rows = Vec::with_capacity(devices.len());
        for (_, id) in devices {
            rows.push(RemoteDevice { id: id.to_string() });
        }
        rows
    }
}

fn reconcile_devices(
    trigger: On<AuthorizedSnapshot>,
    runtimes: Query<(), With<RemoteState>>,
    devices: Query<(Entity, &Name, &ChildOf), With<Authorized>>,
    mut commands: Commands,
) {
    let snapshot = trigger.event();
    if !runtimes.contains(snapshot.entity) {
        return;
    }
    for (entity, device, owner) in &devices {
        if owner.parent() != snapshot.entity {
            continue;
        }
        if !snapshot
            .devices
            .iter()
            .any(|incoming| incoming.id.as_str() == device.as_str())
        {
            commands.entity(entity).despawn();
        }
    }
    for (index, incoming) in snapshot.devices.iter().enumerate() {
        if snapshot.devices[..index]
            .iter()
            .any(|device| device.id == incoming.id)
        {
            continue;
        }
        let mut existing = None;
        for (entity, device, owner) in &devices {
            if owner.parent() == snapshot.entity && device.as_str() == incoming.id.as_str() {
                existing = Some(entity);
                break;
            }
        }
        let fields = (
            Order(index as u32),
            CreatedAt(incoming.authorized_at_unix.saturating_mul(1000)),
        );
        if let Some(entity) = existing {
            commands.entity(entity).insert(fields);
        } else {
            commands.spawn((
                Name::new(incoming.id.as_str().to_string()),
                Authorized,
                ChildOf(snapshot.entity),
                fields,
            ));
        }
    }
}

#[derive(Component, Clone, Debug)]
struct RemotePairingInfo {
    pairing_url: String,
    pairing_deep_link: String,
    relay_token: String,
    pairing_token: String,
}

#[derive(Component)]
struct RemotePairingRegistration {
    relay: vmux_service::pairing::Relay,
    relay_token: String,
    pairing_token: String,
    deadline: Instant,
    next_check: Instant,
}

#[derive(Message, Clone, Copy)]
struct EnableRequest {
    target: Entity,
    generation: u64,
}

#[derive(Message, Clone, Copy)]
struct DisableRequest {
    target: Entity,
    generation: u64,
}

#[derive(Component)]
struct EnableOperation {
    target: Entity,
    generation: u64,
    task: Task<Result<RemotePairingRegistration, String>>,
}

#[derive(Component, Default)]
struct PairingVisibility(Option<Instant>);

impl PairingVisibility {
    const DURATION: Duration = Duration::from_secs(120);
}

#[derive(Component)]
struct ObservedAvailability(bool);

#[derive(Component, Default)]
struct ServiceObservation {
    epoch: uuid::Uuid,
    revision: u64,
}

impl ServiceObservation {
    fn known(observation: Option<&Self>) -> bool {
        observation.is_some_and(|observation| !observation.epoch.is_nil())
    }
}

fn subscribe(
    connected: Query<(), Added<ServiceConnected>>,
    runtimes: Query<Entity, With<RemoteState>>,
    mut requests: MessageWriter<ServiceRequest>,
    mut commands: Commands,
) {
    if connected.is_empty() {
        return;
    }
    for runtime in &runtimes {
        commands
            .entity(runtime)
            .insert(ServiceObservation::default());
    }
    requests.write(ServiceRequest(
        vmux_api::protocol::ClientMessage::SubscribeRemoteObservations,
    ));
}

fn observe(
    mut incoming: MessageReader<ServiceInbound>,
    mut runtimes: Query<(Entity, &mut RemoteState, Option<&mut ServiceObservation>)>,
    mut commands: Commands,
) {
    for incoming in incoming.read() {
        let vmux_api::protocol::ServiceMessage::RemoteObserved(observed) = &incoming.0 else {
            continue;
        };
        if observed.epoch.is_nil() {
            continue;
        }
        let Ok((entity, mut state, history)) = runtimes.single_mut() else {
            continue;
        };
        if let Some(mut history) = history {
            if !history.epoch.is_nil() && history.epoch != observed.epoch
                || !history.epoch.is_nil() && history.revision >= observed.revision
            {
                continue;
            }
            history.epoch = observed.epoch;
            history.revision = observed.revision;
        } else {
            commands.entity(entity).insert(ServiceObservation {
                epoch: observed.epoch,
                revision: observed.revision,
            });
        }
        state.phase = observed.phase;
    }
}

#[derive(Component)]
struct RemoteState {
    enabled: bool,
    phase: RemotePhase,
    error: String,
    operation_generation: u64,
    authorization_checked_at: Instant,
    reconcile_on_startup: bool,
}

fn reconcile(
    mut states: Query<(
        Entity,
        &mut RemoteState,
        &mut ObservedAvailability,
        Option<&ServiceObservation>,
    )>,
    decision: Query<&Decision<bool>, With<Live>>,
    mut enable: MessageWriter<EnableRequest>,
    mut disable: MessageWriter<DisableRequest>,
    mut commands: Commands,
) {
    let available = decision.single().is_ok_and(|decision| decision.value);
    for (entity, mut state, mut observed, service) in &mut states {
        if observed.0 == available {
            continue;
        }
        observed.0 = available;
        state.operation_generation = state.operation_generation.wrapping_add(1);
        commands
            .entity(entity)
            .remove::<RemotePairingInfo>()
            .remove::<RemotePairingRegistration>();
        if !available {
            if !ServiceObservation::known(service) {
                state.phase = RemotePhase::Stopping;
            }
            continue;
        }
        if !state.reconcile_on_startup && !state.enabled {
            continue;
        }
        state.reconcile_on_startup = false;
        if !ServiceObservation::known(service) {
            state.phase = if state.enabled {
                RemotePhase::Starting
            } else {
                RemotePhase::Stopping
            };
        }
        if state.enabled {
            enable.write(EnableRequest {
                target: entity,
                generation: state.operation_generation,
            });
        } else {
            disable.write(DisableRequest {
                target: entity,
                generation: state.operation_generation,
            });
        }
    }
}

fn request(
    trigger: On<UiInput<RemoteRequest>>,
    mut states: Query<(
        Entity,
        &mut RemoteState,
        &mut PairingVisibility,
        Option<&ServiceObservation>,
    )>,
    mut enable: MessageWriter<EnableRequest>,
    mut disable: MessageWriter<DisableRequest>,
    decision: Query<&Decision<bool>, With<Live>>,
) {
    let Ok((entity, mut state, mut visibility, service)) = states.single_mut() else {
        return;
    };
    let enabled = trigger.event().payload.enabled;
    if enabled && !decision.single().is_ok_and(|decision| decision.value) {
        state.error = "Live is unavailable for this installation".into();
        return;
    }
    if enabled == state.enabled && state.phase != RemotePhase::Error {
        return;
    }
    state.enabled = enabled;
    if !ServiceObservation::known(service) {
        state.phase = if enabled {
            RemotePhase::Starting
        } else {
            RemotePhase::Stopping
        };
    }
    state.error.clear();
    if !enabled {
        visibility.0 = None;
    }
    if let Err(error) = persist_enabled(enabled) {
        state.error = error.to_string();
        if enabled {
            state.phase = RemotePhase::Error;
            return;
        }
    }
    state.operation_generation = state.operation_generation.wrapping_add(1);
    if enabled {
        enable.write(EnableRequest {
            target: entity,
            generation: state.operation_generation,
        });
    } else {
        disable.write(DisableRequest {
            target: entity,
            generation: state.operation_generation,
        });
    }
}

fn show_pairing(
    _trigger: On<UiInput<RemotePairingShowRequest>>,
    mut states: Query<(
        &RemoteState,
        Option<&RemotePairingInfo>,
        &mut PairingVisibility,
    )>,
) {
    let Ok((state, pairing, mut visibility)) = states.single_mut() else {
        return;
    };
    if state.phase == RemotePhase::Enabled && pairing.is_some() {
        visibility.0 = Some(Instant::now() + PairingVisibility::DURATION);
    }
}

fn dismiss_pairing(
    _trigger: On<UiInput<RemotePairingDismissRequest>>,
    mut visibility: Single<&mut PairingVisibility>,
) {
    visibility.0 = None;
}

fn begin_enable(
    mut requests: MessageReader<EnableRequest>,
    states: Query<(&RemoteState, &RemoteAuthorizationStore)>,
    decision: Query<&Decision<bool>, With<Live>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok((state, authorizations)) = states.get(request.target) else {
            continue;
        };
        if !state.enabled
            || request.generation != state.operation_generation
            || !decision.single().is_ok_and(|decision| decision.value)
        {
            continue;
        }
        let authorizations = authorizations.clone();
        let task = IoTaskPool::get().spawn(async move { prepare_remote_pairing(&authorizations) });
        commands.spawn((
            Name::new("Enable remote"),
            EnableOperation {
                target: request.target,
                generation: request.generation,
                task,
            },
        ));
    }
}

fn disable(
    mut requests: MessageReader<DisableRequest>,
    mut states: Query<(
        &mut RemoteState,
        &mut PairingVisibility,
        Option<&ServiceObservation>,
    )>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok((mut state, mut visibility, service)) = states.get_mut(request.target) else {
            continue;
        };
        if state.enabled || request.generation != state.operation_generation {
            continue;
        }
        commands
            .entity(request.target)
            .remove::<RemotePairingInfo>()
            .remove::<RemotePairingRegistration>();
        visibility.0 = None;
        if let Err(error) = remove_if_exists(&RemotePaths::current().state()) {
            if !ServiceObservation::known(service) {
                state.phase = RemotePhase::Error;
            }
            state.error = format!("Remote is off, but its state could not be saved: {error}");
        } else {
            if !ServiceObservation::known(service) {
                state.phase = RemotePhase::Disabled;
            }
            state.error.clear();
        }
    }
}

fn poll_enable(
    mut operations: Query<(Entity, &mut EnableOperation)>,
    mut states: Query<(&mut RemoteState, Option<&ServiceObservation>)>,
    decision: Query<&Decision<bool>, With<Live>>,
    mut commands: Commands,
) {
    for (operation_entity, mut operation) in &mut operations {
        let Some(result) = future::block_on(future::poll_once(&mut operation.task)) else {
            continue;
        };
        commands.entity(operation_entity).despawn();
        let Ok((mut state, service)) = states.get_mut(operation.target) else {
            continue;
        };
        if operation.generation != state.operation_generation
            || !state.enabled
            || !decision.single().is_ok_and(|decision| decision.value)
        {
            continue;
        }
        match result {
            Ok(registration) => {
                commands
                    .entity(operation.target)
                    .insert(registration)
                    .remove::<RemotePairingInfo>();
            }
            Err(error) => {
                commands
                    .entity(operation.target)
                    .remove::<RemotePairingInfo>()
                    .remove::<RemotePairingRegistration>();
                if !ServiceObservation::known(service) {
                    state.phase = RemotePhase::Error;
                }
                state.error = error;
            }
        }
    }
}

fn poll_registration(
    devices: Devices,
    mut states: Query<(
        Entity,
        &mut RemoteState,
        &mut RemotePairingRegistration,
        &mut PairingVisibility,
        Option<&ServiceObservation>,
    )>,
    mut commands: Commands,
) {
    let now = Instant::now();
    for (entity, mut state, mut registration, mut visibility, service) in &mut states {
        if now < registration.next_check {
            continue;
        }
        registration.next_check = now + Duration::from_millis(100);
        match registration
            .relay
            .pairing(&registration.relay_token, &registration.pairing_token)
        {
            Ok(Some(pairing)) => {
                if !ServiceObservation::known(service) {
                    state.phase = RemotePhase::Enabled;
                }
                state.error.clear();
                if !devices.paired(entity) {
                    visibility.0 = Some(now + PairingVisibility::DURATION);
                }
                commands
                    .entity(entity)
                    .insert(RemotePairingInfo {
                        pairing_url: pairing.url,
                        pairing_deep_link: pairing.deep_link,
                        relay_token: registration.relay_token.clone(),
                        pairing_token: registration.pairing_token.clone(),
                    })
                    .remove::<RemotePairingRegistration>();
            }
            Ok(None) if now >= registration.deadline => {
                if !ServiceObservation::known(service) {
                    state.phase = RemotePhase::Error;
                }
                state.error = format!(
                    "{} has not allocated a port for this desktop yet",
                    registration.relay.url()
                );
                commands
                    .entity(entity)
                    .remove::<RemotePairingRegistration>();
            }
            Ok(None) => {}
            Err(error) => {
                if !ServiceObservation::known(service) {
                    state.phase = RemotePhase::Error;
                }
                state.error = error;
                commands
                    .entity(entity)
                    .remove::<RemotePairingRegistration>();
            }
        }
    }
}

fn poll_authorizations(
    mut states: Query<(
        Entity,
        &mut RemoteState,
        &RemoteAuthorizationStore,
        Option<&RemotePairingInfo>,
        &mut PairingVisibility,
    )>,
    mut commands: Commands,
    devices_before: Devices,
) {
    let Ok((entity, mut state, authorizations, pairing, mut visibility)) = states.single_mut()
    else {
        return;
    };
    if state.authorization_checked_at.elapsed() < Duration::from_secs(1) {
        return;
    }
    state.authorization_checked_at = Instant::now();
    let Ok(devices) = authorizations.devices() else {
        return;
    };
    let paired = !devices.is_empty();
    let was_paired = devices_before.paired(entity);
    let Ok(pairing_token) = authorizations.pairing_token() else {
        return;
    };
    let became_paired = !was_paired && paired;
    commands.trigger(AuthorizedSnapshot { entity, devices });
    if became_paired {
        visibility.0 = None;
    }
    let Some(pairing) = pairing else {
        return;
    };
    if state.phase != RemotePhase::Enabled || pairing.pairing_token == pairing_token {
        return;
    }
    let relay = vmux_service::pairing::Relay::configured();
    let Ok(Some(updated)) = relay.pairing(&pairing.relay_token, &pairing_token) else {
        return;
    };
    commands.entity(entity).insert(RemotePairingInfo {
        pairing_url: updated.url,
        pairing_deep_link: updated.deep_link,
        relay_token: pairing.relay_token.clone(),
        pairing_token,
    });
    if !paired {
        visibility.0 = Some(Instant::now() + PairingVisibility::DURATION);
    }
}

fn expire_pairing(mut visibility: Single<&mut PairingVisibility>) {
    if visibility
        .0
        .is_some_and(|deadline| deadline <= Instant::now())
    {
        visibility.0 = None;
    }
}

type HostSnapshots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static RemoteState,
        Option<&'static RemotePairingInfo>,
        &'static PairingVisibility,
        Option<&'static UiRemote>,
    ),
>;

fn project(
    mut commands: Commands,
    states: HostSnapshots,
    devices: Devices,
    decision: Query<&Decision<bool>, With<Live>>,
) {
    let available = decision.single().is_ok_and(|decision| decision.value);
    for (entity, state, pairing, visibility, previous) in &states {
        let payload = UiRemote {
            available,
            requested: state.enabled,
            phase: state.phase,
            pairing_url: pairing
                .map(|pairing| pairing.pairing_url.clone())
                .unwrap_or_default(),
            pairing_deep_link: pairing
                .map(|pairing| pairing.pairing_deep_link.clone())
                .unwrap_or_default(),
            paired: devices.paired(entity),
            pairing_visible: visibility
                .0
                .is_some_and(|deadline| deadline > Instant::now()),
            devices: devices.rows(entity),
            error: state.error.clone(),
        };
        if previous != Some(&payload) {
            commands.entity(entity).insert(payload);
        }
    }
}

fn prepare_remote_pairing(
    authorizations: &RemoteAuthorizationStore,
) -> Result<RemotePairingRegistration, String> {
    let relay_token =
        RelayToken::wait(Duration::from_secs(5)).map_err(|error| error.to_string())?;
    let pairing_token = authorizations
        .pairing_token()
        .map_err(|error| error.to_string())?;
    let relay = configured_relay()?;
    let now = Instant::now();
    Ok(RemotePairingRegistration {
        relay,
        relay_token: relay_token.as_str().to_string(),
        pairing_token,
        deadline: now + Duration::from_secs(20),
        next_check: now,
    })
}

fn configured_relay() -> Result<vmux_service::pairing::Relay, String> {
    let relay = vmux_service::pairing::Relay::from_env();
    relay.persist().map_err(|error| error.to_string())?;
    let _ = ensure_relay_device_id().map_err(|error| error.to_string())?;
    Ok(relay)
}

fn ensure_relay_device_id() -> std::io::Result<String> {
    let path = RemotePaths::current().relay_device();
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim();
        if !existing.is_empty() {
            return Ok(existing.to_string());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let device_id = uuid::Uuid::new_v4().simple().to_string();
    std::fs::write(&path, &device_id)?;
    #[cfg(unix)]
    {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(device_id)
}

fn persist_enabled(enabled: bool) -> std::io::Result<()> {
    let path = RemotePaths::current().state();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, if enabled { "enabled\n" } else { "disabled\n" })
}

fn remove_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        app: App,
        runtime: Entity,
    }

    impl Fixture {
        fn new(requested: bool) -> Self {
            let mut app = App::new();
            app.add_plugins(RemotePlugin);
            let runtime = app
                .world_mut()
                .spawn((
                    Name::new("Remote host"),
                    RemoteState {
                        enabled: requested,
                        phase: RemotePhase::Enabled,
                        error: String::new(),
                        operation_generation: 1,
                        authorization_checked_at: Instant::now(),
                        reconcile_on_startup: false,
                    },
                    ObservedAvailability(false),
                    PairingVisibility(Some(Instant::now() - Duration::from_millis(1))),
                ))
                .id();
            Self { app, runtime }
        }

        fn project(&mut self) -> UiRemote {
            self.app.world_mut().run_schedule(Update);
            self.app.world_mut().run_schedule(PostUpdate);
            self.app
                .world()
                .get::<UiRemote>(self.runtime)
                .unwrap()
                .clone()
        }
    }

    #[test]
    fn unavailable_enable_intent_preserves_the_preference_and_queues_no_native_work() {
        let mut fixture = Fixture::new(false);
        let webview = fixture.app.world_mut().spawn_empty().id();
        fixture.app.world_mut().trigger(UiInput {
            webview,
            payload: RemoteRequest { enabled: true },
        });
        let view = fixture.project();
        assert!(!view.available);
        assert!(!view.requested);
        assert!(!view.error.is_empty());
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<EnableRequest>>()
                .is_empty()
        );
    }

    #[test]
    fn hiding_expired_pairing_details_does_not_change_the_requested_preference() {
        let mut fixture = Fixture::new(true);
        let view = fixture.project();
        assert!(view.requested);
        assert!(!view.pairing_visible);
    }

    #[test]
    fn observed_shutdown_preserves_preference_and_refuses_stale_or_foreign_runtime_phases() {
        let mut fixture = Fixture::new(true);
        let epoch = uuid::Uuid::new_v4();
        fixture.app.world_mut().write_message(ServiceInbound(
            vmux_api::protocol::ServiceMessage::RemoteObserved(vmux_api::remote::Observation {
                epoch,
                revision: 1,
                phase: RemotePhase::Stopping,
            }),
        ));
        assert_eq!(fixture.project().phase, RemotePhase::Stopping);
        fixture.app.world_mut().write_message(ServiceInbound(
            vmux_api::protocol::ServiceMessage::RemoteObserved(vmux_api::remote::Observation {
                epoch,
                revision: 2,
                phase: RemotePhase::Disabled,
            }),
        ));
        let stopped = fixture.project();
        assert_eq!(stopped.phase, RemotePhase::Disabled);
        assert!(stopped.requested);
        for (epoch, revision) in [(epoch, 1), (epoch, 2), (uuid::Uuid::new_v4(), 100)] {
            fixture.app.world_mut().write_message(ServiceInbound(
                vmux_api::protocol::ServiceMessage::RemoteObserved(vmux_api::remote::Observation {
                    epoch,
                    revision,
                    phase: RemotePhase::Enabled,
                }),
            ));
        }
        assert_eq!(fixture.project().phase, RemotePhase::Disabled);
    }
}
