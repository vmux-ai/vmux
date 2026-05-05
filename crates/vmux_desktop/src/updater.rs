use bevy::prelude::*;
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use crate::settings::AppSettings;

const DEFAULT_ENDPOINT: &str =
    "https://github.com/vmux-ai/vmux/releases/latest/download/update-manifest.json";

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
        });
        app.add_systems(Startup, init_update_checker)
            .add_systems(Update, poll_update_result);
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
    Installed { version: String },
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
) {
    if checker.done {
        return;
    }

    // Drain results from background thread
    let mut results = Vec::new();
    if let Ok(rx) = checker.rx.lock() {
        while let Ok(result) = rx.try_recv() {
            results.push(result);
        }
    }
    for result in results {
        checker.in_flight = false;
        match result {
            UpdateResult::NoUpdate => {
                bevy::log::debug!("no update available");
            }
            UpdateResult::Installed { version } => {
                bevy::log::info!("update v{version} installed, will take effect on next launch");
                checker.done = true;
                return;
            }
            UpdateResult::Failed(e) => {
                bevy::log::debug!("update check failed: {e}");
            }
        }
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
    checker.in_flight = true;

    std::thread::spawn(move || {
        let result = run_update_check(&endpoint, &pubkey);
        let _ = tx.send(result);
    });
}

fn run_update_check(endpoint: &str, pubkey: &str) -> UpdateResult {
    let current: semver::Version = match env!("CARGO_PKG_VERSION").parse() {
        Ok(v) => v,
        Err(e) => return UpdateResult::Failed(format!("bad current version: {e}")),
    };

    let url = match endpoint.parse() {
        Ok(u) => u,
        Err(e) => return UpdateResult::Failed(format!("bad endpoint URL: {e}")),
    };

    let config = cargo_packager_updater::Config {
        endpoints: vec![url],
        pubkey: pubkey.to_string(),
        ..Default::default()
    };

    let update = match cargo_packager_updater::check_update(current, config) {
        Ok(Some(u)) => u,
        Ok(None) => return UpdateResult::NoUpdate,
        Err(e) => return UpdateResult::Failed(format!("{e}")),
    };

    let version = update.version.clone();

    match update.download_and_install() {
        Ok(()) => UpdateResult::Installed { version },
        Err(e) => UpdateResult::Failed(format!("install failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
