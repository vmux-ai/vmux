use std::path::Path;
use std::result::Result;
use std::time::{Duration, Instant};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::prelude::{App, Plugin};
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{Browsers, UiEventSystems, UiInput};
use vmux_ecs::page::PageReady;
use vmux_ecs::{CreatedAt, Order, UiStateWrite};
use vmux_layout::event::{
    RemoteCopyEvent, RemoteDevice, RemotePairingDismissRequest, RemotePairingShowRequest,
    RemotePhase, RemoteRequest, RemoteRevokeRequest, RemoteUiState,
};
use vmux_layout::{LayoutCef, state::LayoutUiState};
use vmux_service::{RelayToken, RemoteAuthorizationStore, RemotePaths};

use std::os::unix::fs::PermissionsExt;

pub struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EnableRequest>()
            .add_message::<DisableRequest>()
            .add_observer(reconcile_devices)
            .add_observer(request)
            .add_observer(show_pairing)
            .add_observer(dismiss_pairing)
            .add_observer(copy)
            .add_observer(revoke)
            .add_systems(Startup, (spawn_runtime, reconcile_on_startup).chain())
            .add_systems(
                Update,
                (
                    begin_enable,
                    disable,
                    poll_enable,
                    poll_registration,
                    poll_authorizations.after(UiEventSystems::Flush),
                    expire_pairing,
                    push_state_emit,
                )
                    .chain(),
            );
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
    devices: Query<(Entity, &Authorized, &ChildOf)>,
    mut commands: Commands,
) {
    let Ok((runtime, mut state, authorizations)) = states.single_mut() else {
        return;
    };
    let client_id = vmux_service::DeviceId::new(&trigger.event().payload.client_id);
    match authorizations.revoke(&client_id) {
        Ok(true) => {
            for (entity, device, owner) in &devices {
                if owner.parent() == runtime && device.0 == client_id {
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
struct Authorized(vmux_service::DeviceId);

#[derive(EntityEvent)]
struct AuthorizedSnapshot {
    entity: Entity,
    devices: Vec<vmux_service::AuthorizedDevice>,
}

#[derive(SystemParam)]
struct Devices<'w, 's> {
    devices: Query<'w, 's, (&'static Authorized, &'static ChildOf, &'static Order)>,
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
                devices.push((order.0, device.0.as_str()));
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
    devices: Query<(Entity, &Authorized, &ChildOf)>,
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
            .any(|incoming| incoming.id == device.0)
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
            if owner.parent() == snapshot.entity && device.0 == incoming.id {
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
                Authorized(incoming.id.clone()),
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

#[derive(Component)]
struct EnableOperation {
    target: Entity,
    generation: u64,
    task: Task<Result<RemotePairingRegistration, String>>,
}

#[derive(Message, Clone, Copy)]
struct DisableRequest {
    target: Entity,
    generation: u64,
}

#[derive(Component, Default)]
struct PairingVisibility(Option<Instant>);

impl PairingVisibility {
    const DURATION: Duration = Duration::from_secs(120);
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

fn reconcile_on_startup(
    state: Query<(Entity, &RemoteState)>,
    mut enable: MessageWriter<EnableRequest>,
    mut disable: MessageWriter<DisableRequest>,
) {
    let Ok((entity, state)) = state.single() else {
        return;
    };
    if !state.reconcile_on_startup {
        return;
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

fn request(
    trigger: On<UiInput<RemoteRequest>>,
    mut states: Query<(Entity, &mut RemoteState, &mut PairingVisibility)>,
    mut enable: MessageWriter<EnableRequest>,
    mut disable: MessageWriter<DisableRequest>,
) {
    let Ok((entity, mut state, mut visibility)) = states.single_mut() else {
        return;
    };
    let enabled = trigger.event().payload.enabled;
    if enabled == state.enabled && state.phase != RemotePhase::Error {
        return;
    }
    state.enabled = enabled;
    state.phase = RemotePhase::Starting;
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
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok((state, authorizations)) = states.get(request.target) else {
            continue;
        };
        if !state.enabled || request.generation != state.operation_generation {
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
    mut states: Query<(&mut RemoteState, &mut PairingVisibility)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok((mut state, mut visibility)) = states.get_mut(request.target) else {
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
            state.phase = RemotePhase::Error;
            state.error = format!("Remote is off, but its state could not be saved: {error}");
        } else {
            state.phase = RemotePhase::Disabled;
            state.error.clear();
        }
    }
}

fn poll_enable(
    mut operations: Query<(Entity, &mut EnableOperation)>,
    mut states: Query<&mut RemoteState>,
    mut commands: Commands,
) {
    for (operation_entity, mut operation) in &mut operations {
        let Some(result) = future::block_on(future::poll_once(&mut operation.task)) else {
            continue;
        };
        commands.entity(operation_entity).despawn();
        let Ok(mut state) = states.get_mut(operation.target) else {
            continue;
        };
        if operation.generation != state.operation_generation || !state.enabled {
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
                state.phase = RemotePhase::Error;
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
    )>,
    mut commands: Commands,
) {
    let now = Instant::now();
    for (entity, mut state, mut registration, mut visibility) in &mut states {
        if now < registration.next_check {
            continue;
        }
        registration.next_check = now + Duration::from_millis(100);
        match registration
            .relay
            .pairing(&registration.relay_token, &registration.pairing_token)
        {
            Ok(Some(pairing)) => {
                state.phase = RemotePhase::Enabled;
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
                state.phase = RemotePhase::Error;
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
                state.phase = RemotePhase::Error;
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

#[derive(Component)]
struct SubmittedRemote(RemoteUiState);

type RemotePages<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Ref<'static, PageReady>,
        Option<&'static SubmittedRemote>,
    ),
    With<LayoutCef>,
>;

fn push_state_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: RemotePages,
    states: Query<(
        Entity,
        &RemoteState,
        Option<&RemotePairingInfo>,
        &PairingVisibility,
    )>,
    devices: Devices,
) {
    let Ok((runtime, state, pairing, visibility)) = states.single() else {
        return;
    };
    let now = Instant::now();
    let payload = RemoteUiState {
        enabled: state.enabled,
        phase: state.phase,
        pairing_url: pairing
            .map(|pairing| pairing.pairing_url.clone())
            .unwrap_or_default(),
        pairing_deep_link: pairing
            .map(|pairing| pairing.pairing_deep_link.clone())
            .unwrap_or_default(),
        paired: devices.paired(runtime),
        pairing_visible: visibility.0.is_some_and(|deadline| deadline > now),
        devices: devices.rows(runtime),
        error: state.error.clone(),
    };
    for (cef_e, page_ready, published) in &cef_q {
        if !browsers.can_emit_to(&cef_e) {
            continue;
        }
        if published.is_some_and(|submitted| submitted.0 == payload) && !page_ready.is_changed() {
            continue;
        }
        commands.trigger(UiStateWrite::<LayoutUiState>::from_event(cef_e, &payload));
        commands
            .entity(cef_e)
            .insert(SubmittedRemote(payload.clone()));
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
    use vmux_ecs::UiState;

    #[derive(Resource, Default)]
    struct Published {
        last: Option<RemoteUiState>,
        writes: usize,
    }

    impl Published {
        fn capture(trigger: On<UiStateWrite<LayoutUiState>>, mut published: ResMut<Self>) {
            if let Some(remote) = &trigger.event().update().remote {
                published.last = Some(remote.clone());
                published.writes += 1;
            }
        }
    }

    struct RemoteFixture {
        app: App,
        runtime: Entity,
    }

    impl RemoteFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_non_send(Browsers::default())
                .add_plugins(vmux_ecs::UiStatePlugin::<LayoutUiState>::default())
                .add_plugins(RemotePlugin)
                .init_resource::<Published>()
                .add_observer(Published::capture);
            let runtime = app
                .world_mut()
                .spawn((
                    RemoteState {
                        enabled: false,
                        phase: RemotePhase::Disabled,
                        error: String::new(),
                        operation_generation: 1,
                        authorization_checked_at: Instant::now(),
                        reconcile_on_startup: false,
                    },
                    PairingVisibility::default(),
                ))
                .id();
            app.world_mut().flush();
            Self { app, runtime }
        }

        fn tick(&mut self) {
            self.app.world_mut().run_schedule(Update);
        }

        fn snapshot(&mut self, runtime: Entity, ids: &[&str]) {
            let mut devices = Vec::new();
            for &id in ids {
                devices.push(vmux_service::AuthorizedDevice {
                    id: vmux_service::DeviceId::new(id),
                    authorized_at_unix: 100,
                });
            }
            self.app.world_mut().trigger(AuthorizedSnapshot {
                entity: runtime,
                devices,
            });
            self.app.world_mut().flush();
        }

        fn devices(&mut self, runtime: Entity) -> Vec<(Entity, String)> {
            let mut query = self
                .app
                .world_mut()
                .query::<(Entity, &Authorized, &ChildOf, &Order)>();
            let mut devices = Vec::new();
            for (entity, device, owner, order) in query.iter(self.app.world()) {
                if owner.parent() == runtime {
                    devices.push((order.0, entity, device.0.as_str().to_string()));
                }
            }
            devices.sort_by_key(|(order, _, _)| *order);
            devices
                .into_iter()
                .map(|(_, entity, id)| (entity, id))
                .collect()
        }

        fn failure(&mut self, generation: u64) -> Entity {
            let task = IoTaskPool::get().spawn(async { Err("enable rejected".to_string()) });
            self.app
                .world_mut()
                .spawn(EnableOperation {
                    target: self.runtime,
                    generation,
                    task,
                })
                .id()
        }

        fn drain(&mut self, operation: Entity) {
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.app.world().get_entity(operation).is_ok() && Instant::now() < deadline {
                self.tick();
                std::thread::yield_now();
            }
            assert!(
                self.app.world().get_entity(operation).is_err(),
                "enable completion must drain"
            );
        }
    }

    #[test]
    fn pairing_visibility_expires_in_ecs() {
        let mut fixture = RemoteFixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.runtime)
            .insert(PairingVisibility(Some(
                Instant::now() - Duration::from_millis(1),
            )));
        fixture.tick();
        assert!(
            fixture
                .app
                .world()
                .get::<PairingVisibility>(fixture.runtime)
                .unwrap()
                .0
                .is_none()
        );
    }

    #[test]
    fn authorization_refresh_preserves_identity_order_and_runtime_scope() {
        let mut fixture = RemoteFixture::new();
        let other = fixture
            .app
            .world_mut()
            .spawn(RemoteState {
                enabled: false,
                phase: RemotePhase::Disabled,
                error: String::new(),
                operation_generation: 1,
                authorization_checked_at: Instant::now(),
                reconcile_on_startup: false,
            })
            .id();
        fixture.snapshot(fixture.runtime, &["phone-a", "phone-b"]);
        fixture.snapshot(other, &["phone-a"]);
        let before = fixture.devices(fixture.runtime);
        let other_before = fixture.devices(other);
        assert_eq!(
            before.iter().map(|(_, id)| id.as_str()).collect::<Vec<_>>(),
            ["phone-a", "phone-b"]
        );

        fixture.snapshot(fixture.runtime, &["phone-b", "phone-c", "phone-b"]);
        let after = fixture.devices(fixture.runtime);
        assert_eq!(
            after.iter().map(|(_, id)| id.as_str()).collect::<Vec<_>>(),
            ["phone-b", "phone-c"]
        );
        assert_eq!(after[0].0, before[1].0);
        assert!(fixture.app.world().get_entity(before[0].0).is_err());
        assert_eq!(fixture.devices(other), other_before);

        fixture.app.world_mut().despawn(fixture.runtime);
        fixture.snapshot(fixture.runtime, &["stale-phone"]);
        assert!(fixture.devices(fixture.runtime).is_empty());
        for (entity, _) in after {
            assert!(fixture.app.world().get_entity(entity).is_err());
        }
        assert_eq!(fixture.devices(other), other_before);
    }

    #[test]
    fn stale_enable_completion_drains_without_replacing_current_state() {
        let mut fixture = RemoteFixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.runtime)
            .insert(RemotePairingInfo {
                pairing_url: "retained".to_string(),
                pairing_deep_link: String::new(),
                relay_token: String::new(),
                pairing_token: String::new(),
            });
        let operation = fixture.failure(0);
        fixture.drain(operation);
        let state = fixture
            .app
            .world()
            .get::<RemoteState>(fixture.runtime)
            .unwrap();
        assert_eq!(state.phase, RemotePhase::Disabled);
        assert!(state.error.is_empty());
        assert_eq!(
            fixture
                .app
                .world()
                .get::<RemotePairingInfo>(fixture.runtime)
                .unwrap()
                .pairing_url,
            "retained"
        );

        fixture
            .app
            .world_mut()
            .get_mut::<RemoteState>(fixture.runtime)
            .unwrap()
            .enabled = true;
        let orphan = fixture.failure(1);
        fixture.app.world_mut().despawn(fixture.runtime);
        fixture.drain(orphan);
    }

    #[test]
    fn accepted_enable_failure_retires_pairing_and_reports_error() {
        let mut fixture = RemoteFixture::new();
        fixture
            .app
            .world_mut()
            .get_mut::<RemoteState>(fixture.runtime)
            .unwrap()
            .enabled = true;
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.runtime)
            .insert(RemotePairingInfo {
                pairing_url: "old".to_string(),
                pairing_deep_link: String::new(),
                relay_token: String::new(),
                pairing_token: String::new(),
            });
        let operation = fixture.failure(1);
        fixture.drain(operation);
        let state = fixture
            .app
            .world()
            .get::<RemoteState>(fixture.runtime)
            .unwrap();
        assert_eq!(state.phase, RemotePhase::Error);
        assert_eq!(state.error, "enable rejected");
        assert!(
            fixture
                .app
                .world()
                .get::<RemotePairingInfo>(fixture.runtime)
                .is_none()
        );
    }

    #[test]
    fn stale_disable_and_ui_refusal_preserve_state_until_delivery_recovers() {
        let mut fixture = RemoteFixture::new();
        fixture
            .app
            .world_mut()
            .get_mut::<RemoteState>(fixture.runtime)
            .unwrap()
            .enabled = true;
        fixture.app.world_mut().write_message(DisableRequest {
            target: fixture.runtime,
            generation: 0,
        });
        let page = fixture.app.world_mut().spawn((LayoutCef, PageReady)).id();
        fixture.tick();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<RemoteState>(fixture.runtime)
                .unwrap()
                .phase,
            RemotePhase::Disabled
        );
        assert!(
            fixture
                .app
                .world()
                .get::<UiState<LayoutUiState>>(page)
                .and_then(UiState::current)
                .is_none()
        );
        assert!(fixture.app.world().resource::<Published>().last.is_none());
        fixture.snapshot(fixture.runtime, &["phone"]);
        fixture
            .app
            .world_mut()
            .non_send_mut::<Browsers>()
            .set_externally_hosted(page);
        fixture.tick();
        let published = fixture
            .app
            .world()
            .resource::<Published>()
            .last
            .as_ref()
            .unwrap();
        assert!(published.paired);
        assert_eq!(
            published
                .devices
                .iter()
                .map(|device| device.id.as_str())
                .collect::<Vec<_>>(),
            ["phone"]
        );

        fixture.app.world_mut().despawn(page);
        let replacement = fixture.app.world_mut().spawn((LayoutCef, PageReady)).id();
        fixture.snapshot(fixture.runtime, &["replacement-phone"]);
        fixture
            .app
            .world_mut()
            .non_send_mut::<Browsers>()
            .set_externally_hosted(replacement);
        fixture.tick();
        let published = fixture
            .app
            .world()
            .resource::<Published>()
            .last
            .as_ref()
            .unwrap();
        assert_eq!(
            published
                .devices
                .iter()
                .map(|device| device.id.as_str())
                .collect::<Vec<_>>(),
            ["replacement-phone"]
        );
    }
    #[test]
    fn queued_remote_projection_is_deduplicated_and_expired_qr_supersedes_it() {
        let mut fixture = RemoteFixture::new();
        let page = fixture
            .app
            .world_mut()
            .spawn((LayoutCef, PageReady, bevy_cef::prelude::BinIpcBlocked))
            .id();
        fixture
            .app
            .world_mut()
            .non_send_mut::<Browsers>()
            .set_externally_hosted(page);
        fixture.tick();
        fixture.tick();
        assert_eq!(fixture.app.world().resource::<Published>().writes, 1);

        fixture
            .app
            .world_mut()
            .entity_mut(fixture.runtime)
            .insert(PairingVisibility(Some(
                Instant::now() + Duration::from_secs(60),
            )));
        fixture.tick();
        fixture.tick();
        assert_eq!(fixture.app.world().resource::<Published>().writes, 2);
        assert!(
            fixture
                .app
                .world()
                .resource::<Published>()
                .last
                .as_ref()
                .unwrap()
                .pairing_visible
        );

        fixture
            .app
            .world_mut()
            .entity_mut(fixture.runtime)
            .insert(PairingVisibility(Some(
                Instant::now() - Duration::from_millis(1),
            )));
        fixture.tick();
        fixture.tick();
        assert_eq!(fixture.app.world().resource::<Published>().writes, 3);
        assert!(
            !fixture
                .app
                .world()
                .resource::<Published>()
                .last
                .as_ref()
                .unwrap()
                .pairing_visible
        );
    }
}
