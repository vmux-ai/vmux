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

pub(crate) struct RemoteDesktopPlugin;

impl Plugin for RemoteDesktopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RemoteDesktopState>()
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

fn on_remote_copy(_trigger: On<BinReceive<RemoteCopyEvent>>, state: Res<RemoteDesktopState>) {
    if state.phase == RemotePhase::Enabled && !state.pairing_url.is_empty() {
        vmux_terminal::clipboard::write(state.pairing_url.clone());
    }
}

#[derive(Clone, Debug)]
struct RemotePairingInfo {
    pairing_url: String,
    pairing_deep_link: String,
}

struct RemoteWorkerResult {
    enabled: bool,
    result: Result<Option<RemotePairingInfo>, String>,
}

#[derive(Resource)]
struct RemoteDesktopState {
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

impl Default for RemoteDesktopState {
    fn default() -> Self {
        let persisted = std::fs::read_to_string(vmux_service::remote_state_path()).ok();
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
            paired: vmux_service::remote_paired_path().exists(),
            error: String::new(),
            command_tx,
            result_rx,
            paired_checked_at: Instant::now(),
            reconcile_on_startup,
        }
    }
}

fn reconcile_remote_on_startup(state: Res<RemoteDesktopState>) {
    if state.reconcile_on_startup {
        let _ = state.command_tx.send(state.enabled);
    }
}

fn on_remote_command(
    trigger: On<BinReceive<RemoteCommandEvent>>,
    mut state: ResMut<RemoteDesktopState>,
) {
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

fn poll_remote_worker(mut state: ResMut<RemoteDesktopState>) {
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
                if let Err(error) = remove_if_exists(&vmux_service::remote_state_path()) {
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

fn poll_paired_marker(mut state: ResMut<RemoteDesktopState>) {
    if state.paired_checked_at.elapsed() < Duration::from_secs(1) {
        return;
    }
    state.paired_checked_at = Instant::now();
    state.paired = vmux_service::remote_paired_path().exists();
}

fn push_remote_state_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    state: Res<RemoteDesktopState>,
    mut last: Local<Option<RemoteStateEvent>>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
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
    let base_url = match relay_pairing_base_url()? {
        Some(base_url) => base_url,
        None => {
            let port = vmux_service::remote_port();
            format!("http://127.0.0.1:{port}")
        }
    };
    // Absent until the QUIC listener has written its certificate, which leaves the phone on
    // HTTP rather than blocking pairing.
    let fingerprint = vmux_service::remote::quic::identity_fingerprint().unwrap_or_default();
    pairing_info(&base_url, &token, &fingerprint)
}

fn disable_remote() -> Result<(), String> {
    Ok(())
}

/// Build the pairing URL and deep link.
///
/// `fingerprint` is passed in rather than read here so the result depends only on the arguments —
/// reading the certificate from disk would make this answer differ between a machine that has
/// started Remote and one that has not.
fn pairing_info(
    base_url: &str,
    token: &str,
    fingerprint: &str,
) -> Result<RemotePairingInfo, String> {
    let mut pairing_url = url::Url::parse(base_url).map_err(|error| error.to_string())?;
    pairing_url.set_fragment(Some(&if fingerprint.is_empty() {
        format!("token={token}")
    } else {
        format!("token={token}&fp={fingerprint}")
    }));
    let mut pairing_deep_link =
        url::Url::parse("vmuxremote://pair").map_err(|error| error.to_string())?;
    pairing_deep_link
        .query_pairs_mut()
        .append_pair("base", base_url)
        .append_pair("token", token);
    if !fingerprint.is_empty() {
        pairing_deep_link
            .query_pairs_mut()
            .append_pair("fp", fingerprint);
    }
    Ok(RemotePairingInfo {
        pairing_url: pairing_url.to_string(),
        pairing_deep_link: pairing_deep_link.to_string(),
    })
}

fn relay_pairing_base_url() -> Result<Option<String>, String> {
    // Removing the file matters: it is the daemon's only view of this setting, so a stale one
    // would keep it dialling a relay the user just switched off.
    let Some(relay_url) = vmux_service::relay_url_from_env() else {
        let _ = std::fs::remove_file(vmux_service::remote_relay_url_path());
        return Ok(None);
    };
    persist_relay_url(&relay_url).map_err(|error| error.to_string())?;
    let device_id = ensure_relay_device_id().map_err(|error| error.to_string())?;
    Ok(Some(format!("{relay_url}/r/{device_id}")))
}

fn persist_relay_url(relay_url: &str) -> std::io::Result<()> {
    let path = vmux_service::remote_relay_url_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, relay_url.trim().trim_end_matches('/'))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn ensure_relay_device_id() -> std::io::Result<String> {
    let path = vmux_service::remote_relay_device_path();
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
    let path = vmux_service::remote_token_path();
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
    let path = vmux_service::remote_state_path();
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
    fn builds_pairing_urls() {
        let pairing = pairing_info("http://127.0.0.1:54821", "secret", "").unwrap();
        assert_eq!(pairing.pairing_url, "http://127.0.0.1:54821/#token=secret");
        assert_eq!(
            pairing.pairing_deep_link,
            "vmuxremote://pair?base=http%3A%2F%2F127.0.0.1%3A54821&token=secret"
        );
    }

    /// The phone can only pin the desktop's certificate if the fingerprint survives into both
    /// pairing shapes — the QR-encoded URL and the deep link. Dropping it from either would
    /// downgrade that phone to an unpinned connection with nothing to show for it.
    #[test]
    fn a_fingerprint_reaches_both_pairing_shapes() {
        let pairing = pairing_info("http://127.0.0.1:54821", "secret", "abc123").unwrap();

        assert_eq!(
            pairing.pairing_url,
            "http://127.0.0.1:54821/#token=secret&fp=abc123"
        );
        assert_eq!(
            pairing.pairing_deep_link,
            "vmuxremote://pair?base=http%3A%2F%2F127.0.0.1%3A54821&token=secret&fp=abc123"
        );
    }
}
