use std::path::Path;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_cef::prelude::{BinReceive, Browsers};
use crossbeam_channel::{Receiver, Sender};
use vmux_core::page::PageReady;
use vmux_layout::event::{
    RemoteCopyEvent, RemoteDevice, RemotePairingDismissRequest, RemotePairingShowRequest,
    RemotePhase, RemoteRequest, RemoteRevokeRequest, RemoteUiState,
};
use vmux_layout::{LayoutCef, LayoutUiStateUpdates};
use vmux_service::{RelayToken, RemoteAuthorizationStore, RemotePaths};

pub(crate) struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .spawn((Name::new("Remote runtime"), RemoteState::default()));
        app.add_observer(on_remote_request)
            .add_observer(show_remote_pairing)
            .add_observer(dismiss_remote_pairing)
            .add_observer(on_remote_copy)
            .add_observer(on_remote_revoke)
            .add_systems(Startup, reconcile_remote_on_startup)
            .add_systems(
                Update,
                (
                    poll_remote_worker,
                    poll_remote_authorizations,
                    expire_remote_pairing,
                    push_remote_state_emit,
                )
                    .chain(),
            );
    }
}

fn on_remote_copy(
    _trigger: On<BinReceive<RemoteCopyEvent>>,
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
    trigger: On<BinReceive<RemoteRevokeRequest>>,
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

impl RemotePairingInfo {
    const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(20);

    fn wait(
        relay: &vmux_service::pairing::Relay,
        relay_token: &str,
        pairing_token: &str,
    ) -> Result<Self, String> {
        let deadline = Instant::now() + Self::REGISTRATION_TIMEOUT;
        loop {
            if let Some(pairing) = Self::ready(relay, relay_token, pairing_token)? {
                return Ok(pairing);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "{} has not allocated a port for this desktop yet",
                    relay.url()
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn ready(
        relay: &vmux_service::pairing::Relay,
        relay_token: &str,
        pairing_token: &str,
    ) -> Result<Option<Self>, String> {
        let (Some(base_url), Some(device), Some(fingerprint)) = (
            relay.base_url()?,
            relay.registered_device(),
            vmux_service::remote::quic::identity_fingerprint(),
        ) else {
            return Ok(None);
        };
        let pairing = vmux_service::pairing::PairingInfo::new(
            &base_url,
            relay_token,
            pairing_token,
            &fingerprint,
            &device,
        )?;
        Ok(Some(Self {
            pairing_url: pairing.url,
            pairing_deep_link: pairing.deep_link,
            relay_token: relay_token.to_string(),
            pairing_token: pairing_token.to_string(),
        }))
    }
}

struct RemoteWorkerResult {
    enabled: bool,
    result: Result<Option<RemotePairingInfo>, String>,
}

#[derive(Default)]
struct PairingVisibility(Option<Instant>);

impl PairingVisibility {
    const DURATION: Duration = Duration::from_secs(120);

    fn show(&mut self, now: Instant) {
        self.0 = Some(now + Self::DURATION);
    }

    fn dismiss(&mut self) {
        self.0 = None;
    }

    fn visible(&self, now: Instant) -> bool {
        self.0.is_some_and(|deadline| deadline > now)
    }

    fn expire(&mut self, now: Instant) {
        if !self.visible(now) {
            self.dismiss();
        }
    }
}

#[derive(Component)]
struct RemoteState {
    enabled: bool,
    phase: RemotePhase,
    paired: bool,
    pairing_visibility: PairingVisibility,
    devices: Vec<vmux_service::AuthorizedDevice>,
    error: String,
    command_tx: Sender<bool>,
    result_rx: Receiver<RemoteWorkerResult>,
    authorization_checked_at: Instant,
    reconcile_on_startup: bool,
}

impl Default for RemoteState {
    fn default() -> Self {
        let persisted = std::fs::read_to_string(RemotePaths::current().state()).ok();
        let enabled = persisted.as_deref().map(str::trim) == Some("enabled");
        let reconcile_on_startup = persisted.is_some();
        let (command_tx, command_rx) = crossbeam_channel::unbounded();
        let (result_tx, result_rx) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("vmux-remote-control".to_string())
            .spawn(move || remote_worker(command_rx, result_tx))
            .expect("spawn remote control worker");
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
            pairing_visibility: PairingVisibility::default(),
            devices,
            error: String::new(),
            command_tx,
            result_rx,
            authorization_checked_at: Instant::now(),
            reconcile_on_startup,
        }
    }
}

impl RemoteState {
    fn show_pairing(&mut self, now: Instant) {
        if self.phase == RemotePhase::Enabled {
            self.pairing_visibility.show(now);
        }
    }

    fn dismiss_pairing(&mut self) {
        self.pairing_visibility.dismiss();
    }

    fn pairing_visible(&self, now: Instant) -> bool {
        self.pairing_visibility.visible(now)
    }

    fn expire_pairing(&mut self, now: Instant) {
        self.pairing_visibility.expire(now);
    }
}

fn reconcile_remote_on_startup(state: Query<&RemoteState>) {
    let Ok(state) = state.single() else {
        return;
    };
    if state.reconcile_on_startup {
        let _ = state.command_tx.send(state.enabled);
    }
}

fn on_remote_request(trigger: On<BinReceive<RemoteRequest>>, mut states: Query<&mut RemoteState>) {
    let Ok(mut state) = states.single_mut() else {
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
        state.dismiss_pairing();
    }
    if let Err(error) = persist_enabled(enabled) {
        state.error = error.to_string();
        if enabled {
            state.phase = RemotePhase::Error;
            return;
        }
    }
    if let Err(error) = state.command_tx.send(enabled) {
        state.phase = RemotePhase::Error;
        state.error = error.to_string();
    }
}

fn show_remote_pairing(
    _trigger: On<BinReceive<RemotePairingShowRequest>>,
    mut states: Query<(&mut RemoteState, Option<&RemotePairingInfo>)>,
) {
    let Ok((mut state, pairing)) = states.single_mut() else {
        return;
    };
    if pairing.is_some() {
        state.show_pairing(Instant::now());
    }
}

fn dismiss_remote_pairing(
    _trigger: On<BinReceive<RemotePairingDismissRequest>>,
    mut states: Query<&mut RemoteState>,
) {
    let Ok(mut state) = states.single_mut() else {
        return;
    };
    state.dismiss_pairing();
}

fn poll_remote_worker(mut states: Query<(Entity, &mut RemoteState)>, mut commands: Commands) {
    let Ok((entity, mut state)) = states.single_mut() else {
        return;
    };
    while let Ok(message) = state.result_rx.try_recv() {
        if message.enabled != state.enabled {
            continue;
        }
        match message.result {
            Ok(Some(pairing)) => {
                state.phase = RemotePhase::Enabled;
                commands.entity(entity).insert(pairing);
                state.error.clear();
                if !state.paired {
                    state.show_pairing(Instant::now());
                }
            }
            Ok(None) => {
                commands.entity(entity).remove::<RemotePairingInfo>();
                state.dismiss_pairing();
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
                commands.entity(entity).remove::<RemotePairingInfo>();
                state.phase = RemotePhase::Error;
                state.error = error;
            }
        }
    }
}

fn poll_remote_authorizations(
    mut states: Query<(Entity, &mut RemoteState, Option<&RemotePairingInfo>)>,
    mut commands: Commands,
) {
    let Ok((entity, mut state, pairing)) = states.single_mut() else {
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
        state.dismiss_pairing();
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
    let Ok(Some(pairing)) = RemotePairingInfo::ready(&relay, &pairing.relay_token, &pairing_token)
    else {
        return;
    };
    commands.entity(entity).insert(pairing);
    if !state.paired {
        state.show_pairing(Instant::now());
    }
}

fn expire_remote_pairing(mut states: Query<&mut RemoteState>) {
    let Ok(mut state) = states.single_mut() else {
        return;
    };
    state.expire_pairing(Instant::now());
}

fn push_remote_state_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    states: Query<(&RemoteState, Option<&RemotePairingInfo>)>,
    mut last: Local<std::collections::HashMap<Entity, RemoteUiState>>,
) {
    let Ok((state, pairing)) = states.single() else {
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
        pairing_visible: state.pairing_visible(now),
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
        LayoutUiStateUpdates::write(&mut commands, cef_e, &payload);
        last.insert(cef_e, payload.clone());
    }
}

fn remote_worker(command_rx: Receiver<bool>, result_tx: Sender<RemoteWorkerResult>) {
    while let Ok(enabled) = command_rx.recv() {
        let result = if enabled {
            enable_remote().map(Some)
        } else {
            disable_remote().map(|_| None)
        };
        if result_tx
            .send(RemoteWorkerResult { enabled, result })
            .is_err()
        {
            return;
        }
    }
}

fn enable_remote() -> Result<RemotePairingInfo, String> {
    let relay_token =
        RelayToken::wait(Duration::from_secs(5)).map_err(|error| error.to_string())?;
    let pairing_token = RemoteAuthorizationStore::current()
        .pairing_token()
        .map_err(|error| error.to_string())?;
    let relay = configured_relay()?;
    RemotePairingInfo::wait(&relay, relay_token.as_str(), &pairing_token)
}

fn disable_remote() -> Result<(), String> {
    Ok(())
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
    fn pairing_visibility_expires_and_can_be_dismissed() {
        let now = Instant::now();
        let mut visibility = PairingVisibility::default();

        visibility.show(now);
        assert!(visibility.visible(now + Duration::from_secs(119)));

        visibility.expire(now + Duration::from_secs(120));
        assert!(!visibility.visible(now + Duration::from_secs(120)));

        visibility.show(now);
        visibility.dismiss();
        assert!(!visibility.visible(now));
    }
}
