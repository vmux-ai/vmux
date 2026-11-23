use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use std::time::Duration;

use vmux_git::GitCheckForUpdatesRequest;
use vmux_layout::UpdateState;
use vmux_setting::{
    AppSettings, UpdateChannel,
    event::{CheckForUpdatesRequest, CurrentUpdateCheckStatus, UpdateCheckStatus},
};

const DEFAULT_STABLE_ENDPOINT: &str = vmux_profile::Product::stable_endpoint();
const DEFAULT_PREVIEW_ENDPOINT: &str = vmux_profile::Product::preview_endpoint();

#[derive(Clone, Debug)]
pub(super) struct UpdatePlugin {
    stable_endpoint: String,
    preview_endpoint: String,
    pubkey: String,
    initial_delay: Duration,
    poll_interval: Duration,
}

impl UpdatePlugin {
    fn default_pubkey() -> String {
        Self::pubkey_from_env(
            std::env::var("VMUX_UPDATE_PUBLIC_KEY").ok(),
            option_env!("VMUX_UPDATE_PUBLIC_KEY"),
        )
    }

    fn pubkey_from_env(runtime: Option<String>, build_time: Option<&'static str>) -> String {
        runtime
            .or_else(|| build_time.map(ToString::to_string))
            .unwrap_or_default()
    }
}

impl Default for UpdatePlugin {
    fn default() -> Self {
        Self {
            stable_endpoint: DEFAULT_STABLE_ENDPOINT.to_string(),
            preview_endpoint: DEFAULT_PREVIEW_ENDPOINT.to_string(),
            pubkey: Self::default_pubkey(),
            initial_delay: Duration::from_secs(5),
            poll_interval: Duration::from_secs(3600),
        }
    }
}

impl Plugin for UpdatePlugin {
    fn build(&self, app: &mut App) {
        let config = UpdateConfig {
            stable_endpoint: self.stable_endpoint.clone(),
            preview_endpoint: self.preview_endpoint.clone(),
            pubkey: self.pubkey.clone(),
            initial_delay: self.initial_delay,
            poll_interval: self.poll_interval,
        };
        app.insert_resource(config)
            .add_systems(Startup, spawn_checker)
            .add_systems(
                Update,
                (apply_update_progress, schedule_update_check).chain(),
            );
    }
}

fn spawn_checker(mut commands: Commands, config: Res<UpdateConfig>) {
    commands.spawn((
        Name::new("Update checker"),
        UpdateChecker::new(config.initial_delay),
    ));
}

#[derive(Resource, Clone)]
struct UpdateConfig {
    stable_endpoint: String,
    preview_endpoint: String,
    pubkey: String,
    initial_delay: Duration,
    poll_interval: Duration,
}

impl UpdateConfig {
    fn endpoint(&self, channel: UpdateChannel) -> &str {
        match channel {
            UpdateChannel::Stable => &self.stable_endpoint,
            UpdateChannel::Preview => &self.preview_endpoint,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdateStage {
    Downloading,
    Installing,
}

struct UpdateProgress {
    stage: UpdateStage,
    version: String,
    downloaded: u64,
    total: u64,
}

#[derive(Component)]
struct UpdateChecker {
    progress_rx: crossbeam_channel::Receiver<UpdateProgress>,
    progress_tx: crossbeam_channel::Sender<UpdateProgress>,
    done_rx: crossbeam_channel::Receiver<Result<Option<String>, String>>,
    done_tx: crossbeam_channel::Sender<Result<Option<String>, String>>,
    timer: Timer,
    done: bool,
    in_flight: bool,
    channel: Option<UpdateChannel>,
}

impl UpdateChecker {
    fn new(initial_delay: Duration) -> Self {
        let (progress_tx, progress_rx) = crossbeam_channel::unbounded();
        let (done_tx, done_rx) = crossbeam_channel::unbounded();
        Self {
            progress_rx,
            progress_tx,
            done_rx,
            done_tx,
            timer: Timer::from_seconds(initial_delay.as_secs_f32(), TimerMode::Once),
            done: false,
            in_flight: false,
            channel: None,
        }
    }

    fn should_start(
        manual_requested: bool,
        auto_update: bool,
        automatic_due: bool,
        channel_changed: bool,
    ) -> bool {
        manual_requested || channel_changed || (auto_update && automatic_due)
    }
}

fn apply_update_progress(
    mut checker: Single<&mut UpdateChecker>,
    mut state: Single<&mut UpdateState>,
    mut status: Single<&mut CurrentUpdateCheckStatus>,
) {
    let mut progress_items = Vec::new();
    while let Ok(progress) = checker.progress_rx.try_recv() {
        progress_items.push(progress);
    }
    for progress in progress_items {
        match progress.stage {
            UpdateStage::Downloading => {
                status.0 = UpdateCheckStatus::Downloading {
                    version: progress.version.clone(),
                };
                **state =
                    UpdateState::downloading(progress.version, progress.downloaded, progress.total);
            }
            UpdateStage::Installing => {
                status.0 = UpdateCheckStatus::Installing {
                    version: progress.version.clone(),
                };
                **state = UpdateState::installing(progress.version);
            }
        }
    }
    let mut completions = Vec::new();
    while let Ok(done) = checker.done_rx.try_recv() {
        completions.push(done);
    }
    for completion in completions {
        checker.in_flight = false;
        while checker.progress_rx.try_recv().is_ok() {}
        match completion {
            Ok(None) => {
                status.0 = UpdateCheckStatus::UpToDate;
                bevy::log::debug!("no update available");
            }
            Ok(Some(version)) => {
                bevy::log::info!("update v{version} installed, will take effect on next launch");
                status.0 = UpdateCheckStatus::Ready {
                    version: version.clone(),
                };
                **state = UpdateState::ready(version);
                checker.done = true;
            }
            Err(error) => {
                status.0 = UpdateCheckStatus::Failed;
                bevy::log::debug!("update check failed: {error}");
                if !state.is_idle() && !state.is_ready() {
                    **state = UpdateState::idle();
                }
            }
        }
    }
}

fn schedule_update_check(
    mut checker: Single<&mut UpdateChecker>,
    config: Res<UpdateConfig>,
    settings: Res<AppSettings>,
    time: Res<Time>,
    mut status: Single<&mut CurrentUpdateCheckStatus>,
    mut manual_requests: MessageReader<CheckForUpdatesRequest>,
    mut git_requests: MessageReader<GitCheckForUpdatesRequest>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let manual_requested =
        (manual_requests.read().count() + git_requests.read().count()) > 0 && !checker.in_flight;
    let channel_changed = checker
        .channel
        .is_some_and(|channel| channel != settings.update_channel);
    if channel_changed {
        checker.done = false;
    }
    if checker.channel.is_none() {
        checker.channel = Some(settings.update_channel);
    }
    if checker.done || checker.in_flight {
        return;
    }
    if config.endpoint(settings.update_channel).is_empty() {
        checker.channel = Some(settings.update_channel);
        status.0 = UpdateCheckStatus::Unavailable;
        return;
    }
    let automatic_due = if settings.auto_update {
        checker.timer.tick(time.delta());
        checker.timer.just_finished()
    } else {
        false
    };
    if !UpdateChecker::should_start(
        manual_requested,
        settings.auto_update,
        automatic_due,
        channel_changed,
    ) {
        return;
    }
    checker.timer.set_duration(config.poll_interval);
    checker.timer.set_mode(TimerMode::Repeating);
    checker.timer.reset();
    checker.in_flight = true;
    checker.channel = Some(settings.update_channel);
    status.0 = UpdateCheckStatus::Checking;
    UpdateCheckJob::spawn(
        config.endpoint(settings.update_channel).to_string(),
        config.pubkey.clone(),
        checker.progress_tx.clone(),
        checker.done_tx.clone(),
        proxy.as_deref(),
    );
}

struct UpdateCheckJob;

impl UpdateCheckJob {
    fn spawn(
        endpoint: String,
        pubkey: String,
        progress_tx: crossbeam_channel::Sender<UpdateProgress>,
        done_tx: crossbeam_channel::Sender<Result<Option<String>, String>>,
        proxy: Option<&EventLoopProxyWrapper>,
    ) {
        let wake = Self::make_wake(proxy);
        std::thread::spawn(move || {
            Self::run(&endpoint, &pubkey, &progress_tx, &done_tx, &*wake);
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

    fn progress_step(downloaded: u64, total: u64, last_marker: u64) -> Option<u64> {
        let marker = match downloaded.saturating_mul(100).checked_div(total) {
            Some(pct) => pct.min(100),
            None => downloaded / (512 * 1024),
        };
        (marker > last_marker).then_some(marker)
    }

    fn run(
        endpoint: &str,
        pubkey: &str,
        progress_tx: &crossbeam_channel::Sender<UpdateProgress>,
        done_tx: &crossbeam_channel::Sender<Result<Option<String>, String>>,
        wake: &(dyn Fn() + Send),
    ) {
        let current: semver::Version = match env!("CARGO_PKG_VERSION").parse() {
            Ok(v) => v,
            Err(e) => {
                let _ = done_tx.send(Err(format!("bad current version: {e}")));
                wake();
                return;
            }
        };
        let url = match endpoint.parse() {
            Ok(u) => u,
            Err(e) => {
                let _ = done_tx.send(Err(format!("bad endpoint URL: {e}")));
                wake();
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
                let _ = done_tx.send(Ok(None));
                wake();
                return;
            }
            Err(e) => {
                let _ = done_tx.send(Err(format!("{e}")));
                wake();
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
            if let Some(m) = Self::progress_step(downloaded.get(), total.get(), marker.get()) {
                marker.set(m);
                let _ = progress_tx.send(UpdateProgress {
                    stage: UpdateStage::Downloading,
                    version: version.clone(),
                    downloaded: downloaded.get(),
                    total: total.get(),
                });
                wake();
            }
        };
        let on_finish = || {
            let _ = progress_tx.send(UpdateProgress {
                stage: UpdateStage::Installing,
                version: version.clone(),
                downloaded: total.get(),
                total: total.get(),
            });
            wake();
        };
        match update.download_and_install_extended(on_chunk, on_finish) {
            Ok(()) => {
                let _ = done_tx.send(Ok(Some(version)));
                wake();
            }
            Err(e) => {
                let _ = done_tx.send(Err(format!("install failed: {e}")));
                wake();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_step_emits_on_percent_increase() {
        assert_eq!(UpdateCheckJob::progress_step(50, 100, 0), Some(50));
        assert_eq!(UpdateCheckJob::progress_step(50, 100, 50), None);
        assert_eq!(UpdateCheckJob::progress_step(100, 100, 50), Some(100));
    }

    #[test]
    fn progress_step_caps_at_100() {
        assert_eq!(UpdateCheckJob::progress_step(250, 100, 0), Some(100));
    }

    #[test]
    fn progress_step_unknown_total_buckets_by_512k() {
        let bucket = 512 * 1024;
        assert_eq!(UpdateCheckJob::progress_step(0, 0, 0), None);
        assert_eq!(UpdateCheckJob::progress_step(bucket + 1, 0, 0), Some(1));
        assert_eq!(UpdateCheckJob::progress_step(bucket + 1, 0, 1), None);
    }

    #[test]
    fn default_endpoints_match_release_channels() {
        let updater = UpdatePlugin::default();
        let config = UpdateConfig {
            stable_endpoint: updater.stable_endpoint,
            preview_endpoint: updater.preview_endpoint,
            pubkey: updater.pubkey,
            initial_delay: updater.initial_delay,
            poll_interval: updater.poll_interval,
        };

        assert_eq!(
            config.endpoint(UpdateChannel::Stable),
            "https://vmux.ai/updates.json"
        );
        assert_eq!(
            config.endpoint(UpdateChannel::Preview),
            "https://github.com/vmux-ai/vmux/releases/download/nightly/updates-preview.json"
        );
    }

    #[test]
    fn default_updater_checks_after_launch_and_hourly() {
        let updater = UpdatePlugin::default();

        assert_eq!(updater.initial_delay, Duration::from_secs(5));
        assert_eq!(updater.poll_interval, Duration::from_secs(3600));
    }

    #[test]
    fn manual_update_check_bypasses_auto_update_setting() {
        assert!(UpdateChecker::should_start(true, false, false, false));
    }

    #[test]
    fn automatic_update_check_requires_enabled_setting_and_due_timer() {
        assert!(UpdateChecker::should_start(false, true, true, false));
        assert!(!UpdateChecker::should_start(false, false, true, false));
        assert!(!UpdateChecker::should_start(false, true, false, false));
    }

    #[test]
    fn changing_release_channel_checks_immediately() {
        assert!(UpdateChecker::should_start(false, false, false, true));
    }

    #[test]
    fn default_pubkey_uses_runtime_env_first() {
        let pubkey = UpdatePlugin::pubkey_from_env(Some("runtime".to_string()), Some("build"));

        assert_eq!(pubkey, "runtime");
    }

    #[test]
    fn default_pubkey_falls_back_to_build_env() {
        let pubkey = UpdatePlugin::pubkey_from_env(None, Some("build"));

        assert_eq!(pubkey, "build");
    }

    #[test]
    fn default_pubkey_is_empty_when_env_is_missing() {
        let pubkey = UpdatePlugin::pubkey_from_env(None, None);

        assert_eq!(pubkey, "");
    }
}
