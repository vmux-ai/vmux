use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive, JsEmitEventPlugin, Receive};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use vmux_layout::event::RestartRequestEvent;
use vmux_setting::AppSettings;

const DEFAULT_ENDPOINT: &str = "https://vmux.ai/updates.json";

fn default_pubkey() -> String {
    default_pubkey_from_env(
        std::env::var("VMUX_UPDATE_PUBLIC_KEY").ok(),
        option_env!("VMUX_UPDATE_PUBLIC_KEY"),
    )
}

fn default_pubkey_from_env(runtime: Option<String>, build_time: Option<&'static str>) -> String {
    runtime
        .or_else(|| build_time.map(ToString::to_string))
        .unwrap_or_default()
}

/// Auto-updater for Vmux. Checks a remote endpoint for new versions,
/// downloads the signed `.app` bundle, and replaces the current one in-place.
/// The update takes effect on the next launch.
///
/// ```rust,ignore
/// let updater = VmuxUpdater::builder()
///     .endpoint("https://example.com/updates.json")
///     .pubkey("<minisign-public-key>")
///     .initial_delay(Duration::from_secs(5))
///     .poll_interval(Duration::from_secs(3600))
///     .build();
/// ```
#[derive(Clone, Debug)]
pub struct VmuxUpdater {
    endpoint: String,
    pubkey: String,
    initial_delay: Duration,
    poll_interval: Duration,
}

impl VmuxUpdater {
    pub fn builder() -> VmuxUpdaterBuilder {
        VmuxUpdaterBuilder::default()
    }

    /// Convert into a Bevy plugin.
    pub fn plugin(self) -> UpdatePlugin {
        UpdatePlugin { updater: self }
    }
}

#[derive(Clone, Debug)]
pub struct VmuxUpdaterBuilder {
    endpoint: String,
    pubkey: String,
    initial_delay: Duration,
    poll_interval: Duration,
}

impl Default for VmuxUpdaterBuilder {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_string(),
            pubkey: default_pubkey(),
            initial_delay: Duration::from_secs(5),
            poll_interval: Duration::from_secs(3600),
        }
    }
}

impl VmuxUpdaterBuilder {
    pub fn endpoint(mut self, url: &str) -> Self {
        self.endpoint = url.to_string();
        self
    }

    pub fn pubkey(mut self, key: &str) -> Self {
        self.pubkey = key.to_string();
        self
    }

    pub fn initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    pub fn poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn build(self) -> VmuxUpdater {
        VmuxUpdater {
            endpoint: self.endpoint,
            pubkey: self.pubkey,
            initial_delay: self.initial_delay,
            poll_interval: self.poll_interval,
        }
    }
}

// --- Bevy Plugin ---

pub struct UpdatePlugin {
    updater: VmuxUpdater,
}

impl Plugin for UpdatePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(UpdateConfig {
            endpoint: self.updater.endpoint.clone(),
            pubkey: self.updater.pubkey.clone(),
            initial_delay: self.updater.initial_delay,
            poll_interval: self.updater.poll_interval,
        })
        .add_systems(Startup, init_update_checker)
        .add_systems(Update, poll_update_result)
        .add_plugins(BinEventEmitterPlugin::<(RestartRequestEvent,)>::for_hosts(
            &["debug", "extensions", "layout"],
        ))
        .add_plugins(JsEmitEventPlugin::<PageRelaunchRequest>::default())
        .add_observer(on_restart_request)
        .add_observer(on_page_relaunch)
        .add_observer(on_debug_simulate_download);
    }
}

#[derive(serde::Deserialize)]
struct PageRelaunchRequest {
    channel: String,
}

fn relaunch_plan(exe: &std::path::Path, pid: u32, dyld_library_path: Option<&str>) -> Vec<String> {
    let app_bundle = exe
        .ancestors()
        .nth(3)
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .and_then(|p| p.to_str());
    let launch = match app_bundle {
        Some(app) => format!("open \"{app}\""),
        // A bare dev binary is dynamically linked; /bin/sh strips DYLD_* (SIP),
        // so re-inject the search path the running process is already using.
        None => match dyld_library_path {
            Some(dyld) if !dyld.is_empty() => {
                format!("DYLD_LIBRARY_PATH=\"{dyld}\" \"{}\"", exe.display())
            }
            _ => format!("\"{}\"", exe.display()),
        },
    };
    vec![
        "-c".to_string(),
        format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; {launch}"),
    ]
}

fn relaunch_now(exit: &mut MessageWriter<AppExit>) {
    let Ok(exe) = std::env::current_exe() else {
        bevy::log::error!("restart requested but current_exe() is unavailable");
        return;
    };
    let dyld = std::env::var("DYLD_LIBRARY_PATH").ok();
    let args = relaunch_plan(&exe, std::process::id(), dyld.as_deref());
    if let Err(e) = std::process::Command::new("sh").args(&args).spawn() {
        bevy::log::error!("failed to spawn relauncher: {e}");
        return;
    }
    bevy::log::info!("relaunching to apply update");
    exit.write(AppExit::Success);
}

fn on_restart_request(
    _trigger: On<BinReceive<RestartRequestEvent>>,
    mut exit: MessageWriter<AppExit>,
) {
    relaunch_now(&mut exit);
}

fn on_page_relaunch(trigger: On<Receive<PageRelaunchRequest>>, mut exit: MessageWriter<AppExit>) {
    if trigger.payload.channel == "vmux-relaunch" {
        relaunch_now(&mut exit);
    }
}

#[derive(Resource)]
struct UpdateConfig {
    endpoint: String,
    pubkey: String,
    initial_delay: Duration,
    poll_interval: Duration,
}

#[derive(Resource)]
struct UpdateChecker {
    rx: Mutex<mpsc::Receiver<UpdateResult>>,
    tx: mpsc::Sender<UpdateResult>,
    timer: Timer,
    started: bool,
    done: bool,
    in_flight: bool,
}

enum UpdateResult {
    NoUpdate,
    Downloading {
        version: String,
        downloaded: u64,
        total: u64,
    },
    Installing {
        version: String,
    },
    Installed {
        version: String,
    },
    Failed(String),
}

fn init_update_checker(mut commands: Commands, config: Res<UpdateConfig>) {
    let (tx, rx) = mpsc::channel();

    commands.insert_resource(UpdateChecker {
        rx: Mutex::new(rx),
        tx,
        timer: Timer::from_seconds(config.initial_delay.as_secs_f32(), TimerMode::Once),
        started: false,
        done: false,
        in_flight: false,
    });
}

fn poll_update_result(
    mut checker: ResMut<UpdateChecker>,
    config: Res<UpdateConfig>,
    settings: Res<AppSettings>,
    time: Res<Time>,
    mut state: ResMut<vmux_layout::UpdateState>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let mut results = Vec::new();
    if let Ok(rx) = checker.rx.lock() {
        while let Ok(result) = rx.try_recv() {
            results.push(result);
        }
    }
    for result in results {
        match result {
            UpdateResult::NoUpdate => {
                checker.in_flight = false;
                bevy::log::debug!("no update available");
            }
            UpdateResult::Downloading {
                version,
                downloaded,
                total,
            } => {
                *state = vmux_layout::UpdateState::Downloading {
                    version,
                    downloaded,
                    total,
                };
            }
            UpdateResult::Installing { version } => {
                *state = vmux_layout::UpdateState::Installing { version };
            }
            UpdateResult::Installed { version } => {
                checker.in_flight = false;
                bevy::log::info!("update v{version} installed, will take effect on next launch");
                *state = vmux_layout::UpdateState::Ready { version };
                checker.done = true;
            }
            UpdateResult::Failed(e) => {
                checker.in_flight = false;
                bevy::log::debug!("update check failed: {e}");
                if !matches!(
                    *state,
                    vmux_layout::UpdateState::Idle | vmux_layout::UpdateState::Ready { .. }
                ) {
                    *state = vmux_layout::UpdateState::Idle;
                }
            }
        }
    }

    if checker.done {
        return;
    }

    if !settings.auto_update {
        return;
    }

    if checker.in_flight {
        return;
    }

    checker.timer.tick(time.delta());

    if !checker.timer.just_finished() {
        return;
    }

    if !checker.started {
        checker.started = true;
        checker.timer.set_duration(config.poll_interval);
        checker.timer.set_mode(TimerMode::Repeating);
        checker.timer.reset();
    }

    let tx = checker.tx.clone();
    let endpoint = config.endpoint.clone();
    let pubkey = config.pubkey.clone();
    let wake = make_wake(proxy.as_deref());
    checker.in_flight = true;

    std::thread::spawn(move || {
        run_update_check(&endpoint, &pubkey, &tx, &*wake);
    });
}

fn make_wake(proxy: Option<&EventLoopProxyWrapper>) -> Box<dyn Fn() + Send> {
    match proxy {
        Some(p) => {
            let proxy = (**p).clone();
            Box::new(move || {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            })
        }
        None => Box::new(|| {}),
    }
}

fn on_debug_simulate_download(
    _trigger: On<BinReceive<vmux_layout::event::DebugSimulateDownload>>,
    checker: Res<UpdateChecker>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let tx = checker.tx.clone();
    let wake = make_wake(proxy.as_deref());
    std::thread::spawn(move || {
        simulate_download(&tx, &*wake);
    });
}

fn simulate_download(tx: &mpsc::Sender<UpdateResult>, wake: &(dyn Fn() + Send)) {
    let version = "0.0.0-sim".to_string();
    let total: u64 = 24 * 1024 * 1024;
    let step = total / 50;
    let mut downloaded = 0u64;
    while downloaded < total {
        downloaded = downloaded.saturating_add(step).min(total);
        let _ = tx.send(UpdateResult::Downloading {
            version: version.clone(),
            downloaded,
            total,
        });
        wake();
        std::thread::sleep(Duration::from_millis(60));
    }
    let _ = tx.send(UpdateResult::Installing {
        version: version.clone(),
    });
    wake();
    std::thread::sleep(Duration::from_millis(1200));
    let _ = tx.send(UpdateResult::Installed { version });
    wake();
}

fn progress_step(downloaded: u64, total: u64, last_marker: u64) -> Option<u64> {
    let marker = match downloaded.saturating_mul(100).checked_div(total) {
        Some(pct) => pct.min(100),
        None => downloaded / (512 * 1024),
    };
    (marker > last_marker).then_some(marker)
}

fn run_update_check(
    endpoint: &str,
    pubkey: &str,
    tx: &mpsc::Sender<UpdateResult>,
    wake: &(dyn Fn() + Send),
) {
    let current: semver::Version = match env!("CARGO_PKG_VERSION").parse() {
        Ok(v) => v,
        Err(e) => {
            let _ = tx.send(UpdateResult::Failed(format!("bad current version: {e}")));
            return;
        }
    };

    let url = match endpoint.parse() {
        Ok(u) => u,
        Err(e) => {
            let _ = tx.send(UpdateResult::Failed(format!("bad endpoint URL: {e}")));
            return;
        }
    };

    let config = cargo_packager_updater::Config {
        endpoints: vec![url],
        pubkey: pubkey.to_string(),
        ..Default::default()
    };

    let update = match cargo_packager_updater::check_update(current, config) {
        Ok(Some(u)) => u,
        Ok(None) => {
            let _ = tx.send(UpdateResult::NoUpdate);
            return;
        }
        Err(e) => {
            let _ = tx.send(UpdateResult::Failed(format!("{e}")));
            return;
        }
    };

    let version = update.version.clone();

    let downloaded = std::cell::Cell::new(0u64);
    let total = std::cell::Cell::new(0u64);
    let marker = std::cell::Cell::new(0u64);

    let on_chunk = |chunk_len: usize, content_len: Option<u64>| {
        if total.get() == 0
            && let Some(t) = content_len
        {
            total.set(t);
        }
        downloaded.set(downloaded.get().saturating_add(chunk_len as u64));
        if let Some(m) = progress_step(downloaded.get(), total.get(), marker.get()) {
            marker.set(m);
            let _ = tx.send(UpdateResult::Downloading {
                version: version.clone(),
                downloaded: downloaded.get(),
                total: total.get(),
            });
            wake();
        }
    };
    let on_finish = || {
        let _ = tx.send(UpdateResult::Installing {
            version: version.clone(),
        });
        wake();
    };

    match update.download_and_install_extended(on_chunk, on_finish) {
        Ok(()) => {
            let _ = tx.send(UpdateResult::Installed { version });
            wake();
        }
        Err(e) => {
            let _ = tx.send(UpdateResult::Failed(format!("install failed: {e}")));
            wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_step_emits_on_percent_increase() {
        assert_eq!(progress_step(50, 100, 0), Some(50));
        assert_eq!(progress_step(50, 100, 50), None);
        assert_eq!(progress_step(100, 100, 50), Some(100));
    }

    #[test]
    fn progress_step_caps_at_100() {
        assert_eq!(progress_step(250, 100, 0), Some(100));
    }

    #[test]
    fn progress_step_unknown_total_buckets_by_512k() {
        let bucket = 512 * 1024;
        assert_eq!(progress_step(0, 0, 0), None);
        assert_eq!(progress_step(bucket + 1, 0, 0), Some(1));
        assert_eq!(progress_step(bucket + 1, 0, 1), None);
    }

    #[test]
    fn relaunch_plan_opens_app_bundle() {
        let exe = std::path::Path::new("/Applications/Vmux.app/Contents/MacOS/vmux_desktop");
        let args = relaunch_plan(exe, 4242, None);
        assert_eq!(args[0], "-c");
        assert!(args[1].contains("kill -0 4242"));
        assert!(args[1].contains("open \"/Applications/Vmux.app\""));
    }

    #[test]
    fn relaunch_plan_reexecs_bare_binary_in_dev_with_dyld() {
        let exe = std::path::Path::new("/tmp/target/debug/vmux_desktop");
        let args = relaunch_plan(exe, 7, Some("/rust/lib:/tmp/target/debug/deps"));
        assert!(args[1].contains("kill -0 7"));
        assert!(
            args[1].contains("DYLD_LIBRARY_PATH=\"/rust/lib:/tmp/target/debug/deps\" \"/tmp/target/debug/vmux_desktop\"")
        );
        assert!(!args[1].contains("open \""));
    }

    #[test]
    fn default_endpoint_is_vmux_ai_updates_json() {
        assert_eq!(DEFAULT_ENDPOINT, "https://vmux.ai/updates.json");
    }

    #[test]
    fn default_pubkey_uses_runtime_env_first() {
        let pubkey = default_pubkey_from_env(Some("runtime".to_string()), Some("build"));

        assert_eq!(pubkey, "runtime");
    }

    #[test]
    fn default_pubkey_falls_back_to_build_env() {
        let pubkey = default_pubkey_from_env(None, Some("build"));

        assert_eq!(pubkey, "build");
    }

    #[test]
    fn default_pubkey_is_empty_when_env_is_missing() {
        let pubkey = default_pubkey_from_env(None, None);

        assert_eq!(pubkey, "");
    }
}
