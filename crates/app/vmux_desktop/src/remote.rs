use std::path::Path;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{Browsers, UiInput};
use vmux_core::host::UiStateWrite;
use vmux_core::page::PageReady;
use vmux_layout::event::{
    RemoteCopyEvent, RemoteDevice, RemotePairingDismissRequest, RemotePairingShowRequest,
    RemotePhase, RemoteRequest, RemoteRevokeRequest, RemoteUiState,
};
use vmux_layout::{LayoutCef, state::LayoutUiState};
use vmux_service::{RelayToken, RemoteAuthorizationStore, RemotePaths};

pub(crate) struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn((
            Name::new("Remote runtime"),
            RemoteState::default(),
            PairingVisibility::default(),
        ));
        app.add_message::<RemoteOperationRequest>()
            .add_observer(on_remote_request)
            .add_observer(show_remote_pairing)
            .add_observer(dismiss_remote_pairing)
            .add_observer(on_remote_copy)
            .add_observer(on_remote_revoke)
            .add_systems(Startup, reconcile_remote_on_startup)
            .add_systems(
                Update,
                (
                    begin_remote_operations,
                    poll_remote_operations,
                    poll_remote_registration,
                    poll_remote_authorizations,
                    expire_remote_pairing,
                    push_remote_state_emit,
                )
                    .chain(),
            );
    }
}

fn on_remote_copy(
    _trigger: On<UiInput<RemoteCopyEvent>>,
    state: Query<(&RemoteState, &RemotePairingInfo)>,
) {
    let Ok((state, pairing)) = state.single() else {
        return;
    };
    if state.phase == RemotePhase::Enabled {
        vmux_clipboard::write(pairing.pairing_url.clone());
    }
}

fn on_remote_revoke(
    trigger: On<UiInput<RemoteRevokeRequest>>,
    mut states: Query<&mut RemoteState>,
) {
    let Ok(mut state) = states.single_mut() else {
        return;
    };
    let client_id = vmux_service::DeviceId::new(&trigger.event().payload.client_id);
    match RemoteAuthorizationStore::current().revoke(&client_id) {
        Ok(true) => {
            state.devices.retain(|device| device.id != client_id);
            state.paired = !state.devices.is_empty();
            state.error.clear();
        }
        Ok(false) => {}
        Err(error) => {
            state.error = error.to_string();
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
struct RemoteOperationRequest {
    target: Entity,
    enabled: bool,
    generation: u64,
}

#[derive(Component)]
struct RemoteOperation {
    target: Entity,
    enabled: bool,
    generation: u64,
    task: Task<Result<Option<RemotePairingRegistration>, String>>,
}

#[derive(Component, Default)]
struct PairingVisibility(Option<Instant>);

const PAIRING_VISIBILITY_DURATION: Duration = Duration::from_secs(120);

#[derive(Component)]
struct RemoteState {
    enabled: bool,
    phase: RemotePhase,
    paired: bool,
    devices: Vec<vmux_service::AuthorizedDevice>,
    error: String,
    operation_generation: u64,
    authorization_checked_at: Instant,
    reconcile_on_startup: bool,
}

impl Default for RemoteState {
    fn default() -> Self {
        let persisted = std::fs::read_to_string(RemotePaths::current().state()).ok();
        let enabled = persisted.as_deref().map(str::trim) == Some("enabled");
        let reconcile_on_startup = persisted.is_some();
        let devices = RemoteAuthorizationStore::current()
            .devices()
            .unwrap_or_default();
        Self {
            enabled,
            phase: if reconcile_on_startup {
                RemotePhase::Starting
            } else {
                RemotePhase::Disabled
            },
            paired: !devices.is_empty(),
            devices,
            error: String::new(),
            operation_generation: 0,
            authorization_checked_at: Instant::now(),
            reconcile_on_startup,
        }
    }
}

fn reconcile_remote_on_startup(
    state: Query<(Entity, &RemoteState)>,
    mut operations: MessageWriter<RemoteOperationRequest>,
) {
    let Ok((entity, state)) = state.single() else {
        return;
    };
    if state.reconcile_on_startup {
        operations.write(RemoteOperationRequest {
            target: entity,
            enabled: state.enabled,
            generation: state.operation_generation,
        });
    }
}

fn on_remote_request(
    trigger: On<UiInput<RemoteRequest>>,
    mut states: Query<(Entity, &mut RemoteState, &mut PairingVisibility)>,
    mut operations: MessageWriter<RemoteOperationRequest>,
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
    operations.write(RemoteOperationRequest {
        target: entity,
        enabled,
        generation: state.operation_generation,
    });
}

fn show_remote_pairing(
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
        visibility.0 = Some(Instant::now() + PAIRING_VISIBILITY_DURATION);
    }
}

fn dismiss_remote_pairing(
    _trigger: On<UiInput<RemotePairingDismissRequest>>,
    mut visibility: Single<&mut PairingVisibility>,
) {
    visibility.0 = None;
}

fn begin_remote_operations(
    mut requests: MessageReader<RemoteOperationRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let enabled = request.enabled;
        let task = IoTaskPool::get().spawn(async move {
            if enabled {
                prepare_remote_pairing().map(Some)
            } else {
                Ok(None)
            }
        });
        commands.spawn((
            Name::new(if enabled {
                "Enable remote"
            } else {
                "Disable remote"
            }),
            RemoteOperation {
                target: request.target,
                enabled,
                generation: request.generation,
                task,
            },
        ));
    }
}

fn poll_remote_operations(
    mut operations: Query<(Entity, &mut RemoteOperation)>,
    mut states: Query<(&mut RemoteState, &mut PairingVisibility)>,
    mut commands: Commands,
) {
    for (operation_entity, mut operation) in &mut operations {
        let Some(result) = future::block_on(future::poll_once(&mut operation.task)) else {
            continue;
        };
        commands.entity(operation_entity).despawn();
        let Ok((mut state, mut visibility)) = states.get_mut(operation.target) else {
            continue;
        };
        if operation.generation != state.operation_generation || operation.enabled != state.enabled
        {
            continue;
        }
        match result {
            Ok(Some(registration)) => {
                commands
                    .entity(operation.target)
                    .insert(registration)
                    .remove::<RemotePairingInfo>();
            }
            Ok(None) => {
                commands
                    .entity(operation.target)
                    .remove::<RemotePairingInfo>()
                    .remove::<RemotePairingRegistration>();
                visibility.0 = None;
                if let Err(error) = remove_if_exists(&RemotePaths::current().state()) {
                    state.phase = RemotePhase::Error;
                    state.error =
                        format!("Remote is off, but its state could not be saved: {error}");
                } else {
                    state.phase = RemotePhase::Disabled;
                    state.error.clear();
                }
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

fn poll_remote_registration(
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
                if !state.paired {
                    visibility.0 = Some(now + PAIRING_VISIBILITY_DURATION);
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

fn poll_remote_authorizations(
    mut states: Query<(
        Entity,
        &mut RemoteState,
        Option<&RemotePairingInfo>,
        &mut PairingVisibility,
    )>,
    mut commands: Commands,
) {
    let Ok((entity, mut state, pairing, mut visibility)) = states.single_mut() else {
        return;
    };
    if state.authorization_checked_at.elapsed() < Duration::from_secs(1) {
        return;
    }
    state.authorization_checked_at = Instant::now();
    let store = RemoteAuthorizationStore::current();
    let Ok(devices) = store.devices() else {
        return;
    };
    let paired = !devices.is_empty();
    let Ok(pairing_token) = store.pairing_token() else {
        return;
    };
    let became_paired = !state.paired && paired;
    if state.paired != paired {
        state.paired = paired;
    }
    if became_paired {
        visibility.0 = None;
    }
    if state.devices != devices {
        state.devices = devices;
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
    if !state.paired {
        visibility.0 = Some(Instant::now() + PAIRING_VISIBILITY_DURATION);
    }
}

fn expire_remote_pairing(mut visibility: Single<&mut PairingVisibility>) {
    if visibility
        .0
        .is_some_and(|deadline| deadline <= Instant::now())
    {
        visibility.0 = None;
    }
}

fn push_remote_state_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    states: Query<(&RemoteState, Option<&RemotePairingInfo>, &PairingVisibility)>,
    mut last: Local<std::collections::HashMap<Entity, RemoteUiState>>,
) {
    let Ok((state, pairing, visibility)) = states.single() else {
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
        paired: state.paired,
        pairing_visible: visibility.0.is_some_and(|deadline| deadline > now),
        devices: state
            .devices
            .iter()
            .map(|device| RemoteDevice {
                id: device.id.as_str().to_string(),
            })
            .collect(),
        error: state.error.clone(),
    };
    for (cef_e, page_ready) in &cef_q {
        if !browsers.can_emit_to(&cef_e) {
            continue;
        }
        if last.get(&cef_e) == Some(&payload) && !page_ready.is_changed() {
            continue;
        }
        commands.trigger(UiStateWrite::<LayoutUiState>::from_event(cef_e, &payload));
        last.insert(cef_e, payload.clone());
    }
}

fn prepare_remote_pairing() -> Result<RemotePairingRegistration, String> {
    let relay_token =
        RelayToken::wait(Duration::from_secs(5)).map_err(|error| error.to_string())?;
    let pairing_token = RemoteAuthorizationStore::current()
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
        use std::os::unix::fs::PermissionsExt;
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

    #[test]
    fn pairing_visibility_expires_in_ecs() {
        let mut app = App::new();
        app.add_systems(Update, expire_remote_pairing);
        let entity = app
            .world_mut()
            .spawn(PairingVisibility(Some(
                Instant::now() - Duration::from_millis(1),
            )))
            .id();

        app.update();

        assert!(
            app.world()
                .get::<PairingVisibility>(entity)
                .unwrap()
                .0
                .is_none()
        );
    }
}
