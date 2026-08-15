use std::path::Path;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_cef::prelude::{BinHostEmitEvent, BinReceive, Browsers};
use crossbeam_channel::{Receiver, Sender};
use vmux_core::page::PageReady;
use vmux_layout::LayoutCef;
use vmux_layout::event::{
    REMOTE_STATE_EVENT, RemoteCommandEvent, RemoteCopyEvent, RemotePhase, RemoteStateEvent,
};
use vmux_service::RemotePaths;

/// Turning phone pairing on and off: owns the desktop's remote-control state, drives the worker
/// thread that mints the pairing link, and pushes the result to the layout page.
pub(crate) struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RemoteState>()
            .add_observer(on_remote_command)
            .add_observer(on_remote_copy)
            .add_systems(Startup, reconcile_remote_on_startup)
            .add_systems(
                Update,
                (
                    poll_remote_worker,
                    poll_paired_marker,
                    push_remote_state_emit,
                )
                    .chain(),
            );
    }
}

fn on_remote_copy(_trigger: On<BinReceive<RemoteCopyEvent>>, state: Res<RemoteState>) {
    if state.phase == RemotePhase::Enabled && !state.pairing_url.is_empty() {
        vmux_terminal::clipboard::write(state.pairing_url.clone());
    }
}

#[derive(Clone, Debug)]
struct RemotePairingInfo {
    pairing_url: String,
    pairing_deep_link: String,
}

impl RemotePairingInfo {
    /// How long the daemon is given to register with the relay before enabling is called a
    /// failure. Matches what `vmux remote` waits, because it is the same registration.
    const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(20);

    /// Block until the daemon has registered, then build the link a phone can follow.
    ///
    /// Registration is asynchronous — the daemon dials out and the relay allocates it a port — and
    /// the port file is cleared on deregistration, so between a daemon starting and its first
    /// registration neither the port nor the identity is on disk. Reporting that as an error made
    /// a normal few seconds look like a broken setup.
    fn wait(relay: &vmux_service::pairing::Relay, token: &str) -> Result<Self, String> {
        let deadline = Instant::now() + Self::REGISTRATION_TIMEOUT;
        loop {
            if let Some(pairing) = Self::ready(relay, token)? {
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

    /// The link, or `None` while the relay has yet to register this desktop or the identity to be
    /// written.
    ///
    /// There is no loopback fallback: a desktop behind NAT is only reachable through the relay, so
    /// a link naming anything else would pair a phone that can never connect.
    fn ready(relay: &vmux_service::pairing::Relay, token: &str) -> Result<Option<Self>, String> {
        let (Some(base_url), Some(device), Some(fingerprint)) = (
            relay.base_url()?,
            relay.registered_device(),
            vmux_service::remote::quic::identity_fingerprint(),
        ) else {
            return Ok(None);
        };
        let pairing =
            vmux_service::pairing::PairingInfo::new(&base_url, token, &fingerprint, &device)?;
        Ok(Some(Self {
            pairing_url: pairing.url,
            pairing_deep_link: pairing.deep_link,
        }))
    }
}

struct RemoteWorkerResult {
    enabled: bool,
    result: Result<Option<RemotePairingInfo>, String>,
}

#[derive(Resource)]
struct RemoteState {
    enabled: bool,
    phase: RemotePhase,
    pairing_url: String,
    pairing_deep_link: String,
    paired: bool,
    error: String,
    command_tx: Sender<bool>,
    result_rx: Receiver<RemoteWorkerResult>,
    paired_checked_at: Instant,
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
        Self {
            enabled,
            phase: if reconcile_on_startup {
                RemotePhase::Starting
            } else {
                RemotePhase::Disabled
            },
            pairing_url: String::new(),
            pairing_deep_link: String::new(),
            paired: RemotePaths::current().paired().exists(),
            error: String::new(),
            command_tx,
            result_rx,
            paired_checked_at: Instant::now(),
            reconcile_on_startup,
        }
    }
}

fn reconcile_remote_on_startup(state: Res<RemoteState>) {
    if state.reconcile_on_startup {
        let _ = state.command_tx.send(state.enabled);
    }
}

fn on_remote_command(trigger: On<BinReceive<RemoteCommandEvent>>, mut state: ResMut<RemoteState>) {
    let enabled = trigger.event().payload.enabled;
    if enabled == state.enabled && state.phase != RemotePhase::Error {
        return;
    }
    state.enabled = enabled;
    state.phase = RemotePhase::Starting;
    state.error.clear();
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

fn poll_remote_worker(mut state: ResMut<RemoteState>) {
    while let Ok(message) = state.result_rx.try_recv() {
        if message.enabled != state.enabled {
            continue;
        }
        match message.result {
            Ok(Some(pairing)) => {
                state.phase = RemotePhase::Enabled;
                state.pairing_url = pairing.pairing_url;
                state.pairing_deep_link = pairing.pairing_deep_link;
                state.error.clear();
            }
            Ok(None) => {
                state.pairing_url.clear();
                state.pairing_deep_link.clear();
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
                state.phase = RemotePhase::Error;
                state.error = error;
            }
        }
    }
}

fn poll_paired_marker(mut state: ResMut<RemoteState>) {
    if state.paired_checked_at.elapsed() < Duration::from_secs(1) {
        return;
    }
    state.paired_checked_at = Instant::now();
    state.paired = RemotePaths::current().paired().exists();
}

fn push_remote_state_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    state: Res<RemoteState>,
    mut last: Local<Option<RemoteStateEvent>>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.can_emit_to(&cef_e) {
        return;
    }
    let payload = RemoteStateEvent {
        enabled: state.enabled,
        phase: state.phase,
        pairing_url: state.pairing_url.clone(),
        pairing_deep_link: state.pairing_deep_link.clone(),
        paired: state.paired,
        error: state.error.clone(),
    };
    if last.as_ref() == Some(&payload) && !page_ready.is_changed() {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        REMOTE_STATE_EVENT,
        &payload,
    ));
    *last = Some(payload);
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
    let token = wait_for_token().map_err(|error| error.to_string())?;
    let relay = configured_relay()?;
    RemotePairingInfo::wait(&relay, &token)
}

fn disable_remote() -> Result<(), String> {
    Ok(())
}

/// The relay this desktop pairs through, recorded so the daemon dials the same one.
fn configured_relay() -> Result<vmux_service::pairing::Relay, String> {
    let relay = vmux_service::pairing::Relay::from_env();
    relay.persist().map_err(|error| error.to_string())?;
    // Minted here as well as in the daemon so both agree before the first registration.
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

fn wait_for_token() -> std::io::Result<String> {
    let path = RemotePaths::current().token();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(token) = std::fs::read_to_string(&path) {
            let token = token.trim();
            if token.len() >= 32 {
                return Ok(token.to_string());
            }
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("Remote token was not created at {}", path.display()),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
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
