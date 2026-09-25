#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", test))]
mod native;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(target_os = "macos"))]
use other as platform;

#[cfg(target_os = "macos")]
pub(crate) use macos::ensure_native_window_active;

use bevy::prelude::*;
#[cfg(feature = "tray")]
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::window::{Monitor, Window};
use bevy::winit::{EventLoopProxyWrapper, UpdateMode, WinitSettings, WinitUserEvent};
use bevy_cef_core::prelude::{
    Browsers, MessageLoopWakePolicy, windowless_frame_interval_from_refresh_millihertz,
};
use std::time::Duration;

#[cfg(feature = "tray")]
use vmux_terminal::{PtyExited, Terminal};

pub struct RuntimePlugin;

impl Plugin for RuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(platform::RuntimePlatformPlugin)
            .add_message::<HideAllWindowsRequest>()
            .add_systems(Update, hide_all_windows)
            .add_systems(Update, keep_awake_while_revealing);
        #[cfg(feature = "tray")]
        app.add_message::<ShowAllWindowsRequest>()
            .add_message::<QuitRequest>()
            .add_systems(Update, show_all_windows.after(hide_all_windows))
            .add_systems(Update, request_quit.after(show_all_windows))
            .add_systems(Update, start_quit_confirmation.after(request_quit))
            .add_systems(
                Update,
                resolve_quit_confirmation.after(start_quit_confirmation),
            )
            .add_systems(Update, sync_winit_power_mode.after(request_quit));
        #[cfg(not(feature = "tray"))]
        app.add_systems(Update, sync_winit_power_mode.after(hide_all_windows));
    }
}

const FOCUSED_FRAME_INTERVAL: Duration = Duration::from_secs(1);
const UNFOCUSED_FRAME_INTERVAL: Duration = Duration::from_secs(1);
const HIDDEN_FRAME_INTERVAL: Duration = Duration::from_secs(60);
const BACKGROUND_CEF_WAKE_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Message, Debug, Clone, Copy)]
pub(crate) struct HideAllWindowsRequest;

#[cfg(feature = "tray")]
#[derive(Message, Debug, Clone, Copy)]
pub(crate) struct ShowAllWindowsRequest;

#[cfg(feature = "tray")]
#[derive(Message, Debug, Clone, Copy)]
pub(crate) struct QuitRequest;

#[cfg(feature = "tray")]
#[derive(Component)]
struct QuitConfirmation {
    count: usize,
    wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
}

#[cfg(feature = "tray")]
#[derive(Component)]
struct QuitConfirmationTask(Task<bool>);

#[cfg(feature = "tray")]
fn start_quit_confirmation(
    confirmations: Query<(Entity, &QuitConfirmation), Added<QuitConfirmation>>,
    mut commands: Commands,
) {
    for (entity, confirmation) in &confirmations {
        let count = confirmation.count;
        let wake = confirmation.wake.clone();
        let task = IoTaskPool::get().spawn(async move {
            let description = if count == 1 {
                "A terminal is still running. Quit anyway?".to_string()
            } else {
                format!("{count} terminals are still running. Quit anyway?")
            };
            let result = rfd::AsyncMessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("Quit Vmux?")
                .set_description(description)
                .set_buttons(rfd::MessageButtons::OkCancel)
                .show()
                .await;
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            matches!(result, rfd::MessageDialogResult::Ok)
        });
        commands.entity(entity).insert(QuitConfirmationTask(task));
    }
}

pub(crate) fn foreground_winit_settings(
    live_resize: bool,
    native_pointer_inside: bool,
) -> WinitSettings {
    let focused_mode = if live_resize {
        UpdateMode::Reactive {
            wait: Duration::from_millis(16),
            react_to_device_events: false,
            react_to_user_events: true,
            react_to_window_events: false,
        }
    } else {
        UpdateMode::Reactive {
            wait: FOCUSED_FRAME_INTERVAL,
            react_to_device_events: false,
            react_to_user_events: true,
            react_to_window_events: !native_pointer_inside,
        }
    };
    WinitSettings {
        focused_mode,
        unfocused_mode: UpdateMode::reactive_low_power(UNFOCUSED_FRAME_INTERVAL),
    }
}

fn hidden_winit_settings() -> WinitSettings {
    WinitSettings {
        focused_mode: UpdateMode::reactive_low_power(HIDDEN_FRAME_INTERVAL),
        unfocused_mode: UpdateMode::reactive_low_power(HIDDEN_FRAME_INTERVAL),
    }
}

fn sync_winit_power_mode(
    mut settings: ResMut<WinitSettings>,
    wake_policy: Option<Res<MessageLoopWakePolicy>>,
    windows: Query<&Window>,
    monitors: Query<&Monitor>,
) {
    let all_hidden = windows.iter().all(|w| !w.visible);
    let any_visible = windows.iter().any(|w| w.visible);
    let any_focused = windows.iter().any(|w| w.visible && w.focused);
    let live_resize = platform::live_resize_active();
    let native_pointer_inside = platform::native_pointer_inside();
    let next = if all_hidden {
        hidden_winit_settings()
    } else {
        foreground_winit_settings(live_resize, native_pointer_inside)
    };
    if settings.focused_mode != next.focused_mode || settings.unfocused_mode != next.unfocused_mode
    {
        *settings = next;
    }
    if let Some(policy) = wake_policy {
        policy.set_min_wake_interval(cef_wake_interval(
            all_hidden,
            any_visible,
            any_focused,
            foreground_cef_wake_interval(monitors.iter().map(|m| m.refresh_rate_millihertz)),
        ));
    }
}

const MIN_FOREGROUND_CEF_WAKE_INTERVAL: Duration = Duration::from_nanos(16_666_666);

fn foreground_cef_wake_interval(refresh_rates: impl IntoIterator<Item = Option<u32>>) -> Duration {
    let display = windowless_frame_interval_from_refresh_millihertz(
        refresh_rates.into_iter().flatten().max(),
    );
    display.max(MIN_FOREGROUND_CEF_WAKE_INTERVAL)
}

fn cef_wake_interval(
    all_hidden: bool,
    any_visible: bool,
    any_focused: bool,
    foreground_interval: Duration,
) -> Duration {
    if all_hidden || !any_visible || !any_focused {
        BACKGROUND_CEF_WAKE_INTERVAL
    } else {
        foreground_interval
    }
}

fn keep_awake_while_revealing(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    pending: Query<(), With<vmux_layout::PendingWebviewReveal>>,
) {
    if pending.is_empty() {
        return;
    }
    if let Some(proxy) = proxy {
        let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
    }
}

fn hide_all_windows(
    mut requests: MessageReader<HideAllWindowsRequest>,
    mut windows: Query<&mut Window>,
    browsers: Option<NonSend<Browsers>>,
) {
    if requests.read().count() == 0 {
        return;
    }
    for mut window in &mut windows {
        window.visible = false;
    }
    if let Some(browsers) = browsers {
        browsers.set_all_osr_hidden();
    }
}

#[cfg(feature = "tray")]
fn show_all_windows(
    mut requests: MessageReader<ShowAllWindowsRequest>,
    mut windows: Query<&mut Window>,
) {
    if requests.read().count() == 0 {
        return;
    }
    for mut window in &mut windows {
        window.visible = true;
    }
}

#[cfg(feature = "tray")]
fn request_quit(
    mut requests: MessageReader<QuitRequest>,
    terminals: Query<(), (With<Terminal>, Without<PtyExited>)>,
    confirmation: Query<(), With<QuitConfirmation>>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut exits: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    if requests.read().count() == 0 || !confirmation.is_empty() {
        return;
    }
    let live = terminals.iter().count();
    if live > 0 {
        commands.spawn(QuitConfirmation {
            count: live,
            wake: wake.map(|proxy| (**proxy).clone()),
        });
        return;
    }
    exits.write(AppExit::Success);
}

#[cfg(feature = "tray")]
fn resolve_quit_confirmation(
    mut confirmations: Query<(Entity, &mut QuitConfirmationTask)>,
    mut exits: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    for (entity, mut confirmation) in &mut confirmations {
        let Some(confirmed) = future::block_on(future::poll_once(&mut confirmation.0)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if confirmed {
            exits.write(AppExit::Success);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "tray")]
    #[test]
    fn quit_without_live_terminal_exits_immediately() {
        let mut app = App::new();
        app.add_message::<QuitRequest>()
            .add_message::<AppExit>()
            .add_systems(Update, request_quit);
        app.world_mut()
            .resource_mut::<Messages<QuitRequest>>()
            .write(QuitRequest);

        app.update();

        let exits: Vec<AppExit> = app
            .world_mut()
            .resource_mut::<Messages<AppExit>>()
            .drain()
            .collect();
        assert_eq!(exits, vec![AppExit::Success]);
    }

    #[test]
    fn no_continuous_update_mode_anywhere_in_workspace() {
        use std::path::Path;
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        let banned = ["UpdateMode", "::", "Continuous"].concat();
        let mut offenders = Vec::new();
        for root in ["crates", "patches"] {
            let dir = workspace_root.join(root);
            if !dir.exists() {
                continue;
            }
            walk_rs_files(&dir, &mut |path, source| {
                if path.ends_with("runtime.rs") {
                    return;
                }
                for (lineno, line) in source.lines().enumerate() {
                    let stripped = line.trim_start();
                    if stripped.starts_with("//") || stripped.starts_with("///") {
                        continue;
                    }
                    if line.contains(&banned) {
                        offenders.push(format!(
                            "{}:{}: {}",
                            path.display(),
                            lineno + 1,
                            line.trim()
                        ));
                    }
                }
            });
        }
        assert!(
            offenders.is_empty(),
            "Bevy `UpdateMode::Continuous` is banned in vmux (causes 100-200% idle CPU). Use `UpdateMode::Reactive` and route missing wake sources via `EventLoopProxy::send_event(WinitUserEvent::WakeUp)`. See AGENTS.md. Offenders:\n{}",
            offenders.join("\n")
        );
    }

    fn walk_rs_files(dir: &std::path::Path, visit: &mut dyn FnMut(&std::path::Path, &str)) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                    continue;
                }
                walk_rs_files(&path, visit);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
                && let Ok(source) = std::fs::read_to_string(&path)
            {
                visit(&path, &source);
            }
        }
    }

    #[test]
    fn foreground_power_mode_is_reactive_when_focused() {
        let settings = foreground_winit_settings(false, false);

        let UpdateMode::Reactive {
            wait,
            react_to_device_events,
            react_to_user_events,
            react_to_window_events,
        } = settings.focused_mode
        else {
            panic!(
                "focused mode must be Reactive, got {:?}",
                settings.focused_mode
            );
        };
        assert_eq!(wait, Duration::from_secs(1));
        assert!(react_to_user_events);
        assert_eq!(
            settings.unfocused_mode,
            UpdateMode::reactive_low_power(Duration::from_secs(1))
        );
        assert!(!react_to_device_events);
        assert!(react_to_window_events);

        let UpdateMode::Reactive {
            wait: resize_wait,
            react_to_window_events: resize_window,
            ..
        } = foreground_winit_settings(true, false).focused_mode
        else {
            panic!("focused mode must be Reactive");
        };
        assert_eq!(resize_wait, Duration::from_millis(16));
        assert!(!resize_window);

        let layout_hover = foreground_winit_settings(false, true);
        let UpdateMode::Reactive {
            react_to_window_events: layout_window,
            ..
        } = layout_hover.focused_mode
        else {
            panic!("focused mode must be Reactive");
        };
        assert!(!layout_window);
    }

    fn platform_systems(label: impl bevy::ecs::schedule::ScheduleLabel) -> Vec<String> {
        use bevy::ecs::schedule::{NodeId, Schedules};

        let mut app = App::new();
        app.add_plugins(platform::RuntimePlatformPlugin);
        let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
        let Some(mut schedule) = schedules.remove(label) else {
            return Vec::new();
        };
        schedule.initialize(app.world_mut()).unwrap();
        let graph = schedule.graph();

        let mut names = Vec::new();
        for (parent, child) in graph.hierarchy().graph().all_edges() {
            let (NodeId::Set(set), NodeId::System(_)) = (parent, child) else {
                continue;
            };
            let Some(set) = graph.system_sets.get(set) else {
                continue;
            };
            let rendered = format!("{set:?}");
            if let Some(path) = rendered.strip_prefix("SystemTypeSet:")
                && let Some(name) = path.rsplit("::").next()
            {
                names.push(name.to_string());
            }
        }
        names
    }

    #[test]
    fn startup_installs_the_mouse_wake_monitor_and_activates_the_window() {
        let startup = platform_systems(Startup);
        assert!(
            startup.contains(&"install_native_mouse_wake_monitor".to_string()),
            "startup systems: {startup:?}"
        );
        assert!(
            startup.contains(&"activate_primary_window_on_startup".to_string()),
            "startup systems: {startup:?}"
        );
    }

    #[test]
    fn app_activation_starts_during_boot() {
        let update = platform_systems(Update);
        assert!(
            update.contains(&"activate_app_during_boot".to_string()),
            "update systems: {update:?}"
        );
    }

    #[test]
    fn hidden_power_mode_ignores_stale_window_focus() {
        let settings = hidden_winit_settings();

        assert_eq!(
            settings.focused_mode,
            UpdateMode::reactive_low_power(Duration::from_secs(60))
        );
        assert_eq!(
            settings.unfocused_mode,
            UpdateMode::reactive_low_power(Duration::from_secs(60))
        );
    }

    #[test]
    fn cef_wake_policy_follows_display_refresh_but_not_past_60hz() {
        assert_eq!(
            foreground_cef_wake_interval([Some(60_000)]),
            MIN_FOREGROUND_CEF_WAKE_INTERVAL
        );
        assert_eq!(
            foreground_cef_wake_interval([Some(144_000)]),
            MIN_FOREGROUND_CEF_WAKE_INTERVAL,
            "a faster panel must not make the app wake faster"
        );
        assert!(
            foreground_cef_wake_interval([Some(30_000)]) > MIN_FOREGROUND_CEF_WAKE_INTERVAL,
            "a slower panel still wakes less often"
        );
        assert_eq!(
            cef_wake_interval(false, true, true, Duration::from_millis(7)),
            Duration::from_millis(7)
        );
    }

    #[test]
    fn cef_wake_policy_throttles_visible_unfocused() {
        assert_eq!(
            cef_wake_interval(false, true, false, Duration::from_millis(7)),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn cef_wake_policy_throttles_hidden() {
        assert_eq!(
            cef_wake_interval(false, false, true, Duration::from_millis(7)),
            Duration::from_secs(1)
        );
        assert_eq!(
            cef_wake_interval(true, true, true, Duration::from_millis(7)),
            Duration::from_secs(1)
        );
    }
}
