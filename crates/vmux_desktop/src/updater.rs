mod apply;
mod download;
mod github;
mod stage;

use bevy::prelude::*;
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use crate::settings::AppSettings;

/// Auto-updater for Vmux. Checks GitHub Releases for new versions,
/// downloads the signed .app bundle, and applies on next launch.
///
/// ```rust,ignore
/// let updater = VmuxUpdater::builder()
///     .repo("vmux-ai", "vmux")
///     .initial_delay(Duration::from_secs(5))
///     .poll_interval(Duration::from_secs(3600))
///     .build();
/// ```
#[derive(Clone, Debug)]
pub struct VmuxUpdater {
    repo_owner: String,
    repo_name: String,
    initial_delay: Duration,
    poll_interval: Duration,
}

impl VmuxUpdater {
    pub fn builder() -> VmuxUpdaterBuilder {
        VmuxUpdaterBuilder::default()
    }

    /// Check for a staged update and apply it if valid.
    /// Call this in main() before Bevy starts.
    /// Returns `true` if the process should re-exec.
    pub fn apply_staged_update(&self) -> bool {
        apply::apply_staged_update()
    }

    /// Re-exec the current binary (replaces the current process).
    pub fn re_exec(&self) -> ! {
        apply::re_exec()
    }

    /// Convert into a Bevy plugin.
    pub fn plugin(self) -> UpdatePlugin {
        UpdatePlugin { updater: self }
    }
}

#[derive(Clone, Debug)]
pub struct VmuxUpdaterBuilder {
    repo_owner: String,
    repo_name: String,
    initial_delay: Duration,
    poll_interval: Duration,
}

impl Default for VmuxUpdaterBuilder {
    fn default() -> Self {
        Self {
            repo_owner: "vmux-ai".to_string(),
            repo_name: "vmux".to_string(),
            initial_delay: Duration::from_secs(5),
            poll_interval: Duration::from_secs(3600),
        }
    }
}

impl VmuxUpdaterBuilder {
    pub fn repo(mut self, owner: &str, name: &str) -> Self {
        self.repo_owner = owner.to_string();
        self.repo_name = name.to_string();
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
            repo_owner: self.repo_owner,
            repo_name: self.repo_name,
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
            repo_owner: self.updater.repo_owner.clone(),
            repo_name: self.updater.repo_name.clone(),
            initial_delay: self.updater.initial_delay,
            poll_interval: self.updater.poll_interval,
        });
        app.add_systems(Startup, init_update_checker)
            .add_systems(Update, poll_update_result);
    }
}

#[derive(Resource)]
struct UpdateConfig {
    repo_owner: String,
    repo_name: String,
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
    Staged { version: String },
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
            UpdateResult::Staged { version } => {
                bevy::log::info!("update v{version} staged, will apply on next launch");
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

    if stage::has_staged_update() {
        checker.done = true;
        bevy::log::debug!("staged update already present, skipping check");
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
        checker.timer = Timer::from_seconds(config.poll_interval.as_secs_f32(), TimerMode::Repeating);
    }

    let tx = checker.tx.clone();
    let owner = config.repo_owner.clone();
    let name = config.repo_name.clone();
    checker.in_flight = true;

    std::thread::spawn(move || {
        let result = run_update_check(&owner, &name);
        let _ = tx.send(result);
    });
}

fn run_update_check(repo_owner: &str, repo_name: &str) -> UpdateResult {
    let current = match semver::Version::parse(env!("CARGO_PKG_VERSION")) {
        Ok(v) => v,
        Err(e) => return UpdateResult::Failed(format!("bad current version: {e}")),
    };

    let release = match github::check_for_update(&current, repo_owner, repo_name) {
        Ok(Some(r)) => r,
        Ok(None) => return UpdateResult::NoUpdate,
        Err(e) => return UpdateResult::Failed(format!("check failed: {e}")),
    };

    let download_dir = match stage::downloading_dir() {
        Some(d) => d,
        None => return UpdateResult::Failed("no cache dir".to_string()),
    };

    let (tarball, sha256) = match download::download_and_verify(
        &release.tarball_url,
        &release.sha256_url,
        &download_dir,
    ) {
        Ok(result) => result,
        Err(e) => return UpdateResult::Failed(format!("download failed: {e}")),
    };

    let version_str = release.version.to_string();
    if let Err(e) = stage::stage_update(&tarball, &version_str, &sha256) {
        return UpdateResult::Failed(format!("staging failed: {e}"));
    }

    UpdateResult::Staged {
        version: version_str,
    }
}
