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

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy::window::{Monitor, Window};
use bevy::winit::{EventLoopProxyWrapper, UpdateMode, WinitSettings, WinitUserEvent};
use bevy_cef_core::prelude::{
    Browsers, MessageLoopWakePolicy, windowless_frame_interval_from_refresh_millihertz,
};
use std::time::Duration;

#[cfg(feature = "tray")]
use vmux_terminal as terminal;
#[cfg(feature = "tray")]
use vmux_terminal::{PtyExited, Terminal};

pub struct RuntimePlugin;

impl Plugin for RuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(platform::RuntimePlatformPlugin)
            .add_message::<LifecycleEvent>()
            .add_systems(Update, handle_lifecycle_events)
            .add_systems(Update, sync_winit_power_mode.after(handle_lifecycle_events))
            .add_systems(Update, keep_awake_while_revealing);
    }
}

const FOCUSED_FRAME_INTERVAL: Duration = Duration::from_secs(1);
const UNFOCUSED_FRAME_INTERVAL: Duration = Duration::from_secs(1);
const HIDDEN_FRAME_INTERVAL: Duration = Duration::from_secs(60);
const BACKGROUND_CEF_WAKE_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Message, Debug, Clone, Copy)]
pub enum LifecycleEvent {
    HideAllWindows,
    #[cfg(feature = "tray")]
    ShowAllWindows,
    #[cfg(feature = "tray")]
    QuitVmux,
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

/// The wake exists so Bevy can service CEF, not so it can match the display.
///
/// On macOS CEF is pumped by its own CFRunLoop timer, so this wake only has to be often enough to
/// upload OSR textures and run the schedule. Following a 120 Hz panel doubled the work for a
/// surface that is not animating, so the foreground rate is floored at 60 Hz.
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

fn handle_lifecycle_events(world: &mut World) {
    let drained: Vec<LifecycleEvent> = {
        let mut events = world.resource_mut::<Messages<LifecycleEvent>>();
        events.drain().collect()
    };

    for event in drained {
        match event {
            LifecycleEvent::HideAllWindows => {
                let mut q = world.query::<&mut Window>();
                for mut w in q.iter_mut(world) {
                    w.visible = false;
                }
                hide_all_osr_webviews(world);
            }
            #[cfg(feature = "tray")]
            LifecycleEvent::ShowAllWindows => {
                let mut q = world.query::<&mut Window>();
                for mut w in q.iter_mut(world) {
                    w.visible = true;
                }
            }
            #[cfg(feature = "tray")]
            LifecycleEvent::QuitVmux => {
                let live = {
                    let mut q = world.query_filtered::<(), (With<Terminal>, Without<PtyExited>)>();
                    q.iter(world).count()
                };
                if live > 0 && !terminal::confirm_quit_dialog(live) {
                    continue;
                }
                world
                    .resource_mut::<Messages<AppExit>>()
                    .write(AppExit::Success);
            }
        }
    }
}

fn hide_all_osr_webviews(world: &mut World) {
    if let Some(browsers) = world.get_non_send::<Browsers>() {
        browsers.set_all_osr_hidden();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_lifecycle_events_uses_world_for_confirm_dialog() {
        let source = include_str!("runtime.rs");
        let exclusive_marker = ["world", ": ", "&mut", " World"].concat();
        assert!(
            source.contains(&exclusive_marker),
            "handle_lifecycle_events must be an exclusive &mut World system to call confirm_quit_dialog"
        );
        let confirm_call = ["confirm", "_quit_dialog"].concat();
        assert!(
            source.contains(&confirm_call),
            "QuitVmux arm must call terminal::confirm_quit_dialog"
        );
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

    #[test]
    fn native_mouse_motion_publishes_latest_sample_before_waking() {
        let source = include_str!("runtime/macos.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn install_live_resize_monitor").next())
            .unwrap_or_default();

        assert!(monitor.contains("NSEventMask::MouseMoved"));
        assert!(monitor.contains("NSEventMask::LeftMouseDown"));
        assert!(monitor.contains("WinitUserEvent::WakeUp"));
        assert!(monitor.contains("vmux_layout::native_pointer::publish"));
        assert!(!monitor.contains("forward_pointer_move"));
        assert!(!monitor.contains("vmux_layout::pane::wake_on_move"));
        assert!(monitor.contains("let global_mask = NSEventMask::LeftMouseDown"));
    }

    #[test]
    fn native_mouse_wake_throttle_has_a_trailing_wake() {
        let source = include_str!("runtime/macos.rs");
        let throttle = source
            .split("fn native_throttle")
            .nth(1)
            .and_then(|tail| tail.split("fn install_native_mouse_wake_monitor").next())
            .unwrap_or_default();

        assert!(throttle.contains("sync_channel::<()>(1)"));
        assert!(throttle.contains("recv_timeout"));
        assert!(throttle.contains("pending_interval_ns.fetch_min"));
        assert!(!throttle.contains("while wake_rx.try_recv().is_ok()"));
        assert!(!throttle.contains("thread_pending_interval_ns.store"));
        assert!(!throttle.contains("LAST_NATIVE_MOUSE_WAKE.lock()"));
    }

    #[test]
    fn native_mouse_monitor_tracks_left_button_state() {
        let source = include_str!("runtime/macos.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn install_live_resize_monitor").next())
            .unwrap_or_default();

        assert!(monitor.contains("vmux_browser::set_native_left_mouse_down(true)"));
        assert!(monitor.contains("vmux_browser::set_native_left_mouse_down(false)"));
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
    fn primary_window_activation_takes_the_key_window() {
        let native = include_str!("runtime/macos.rs");
        assert!(native.contains("activateIgnoringOtherApps"));
        assert!(native.contains("makeKeyAndOrderFront"));
    }

    #[test]
    fn native_mouse_monitor_does_not_wait_for_window_creation() {
        let source = include_str!("runtime/macos.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn install_live_resize_monitor").next())
            .unwrap_or_default();

        assert!(monitor.contains("proxy: Option<Res<EventLoopProxyWrapper>>"));
        assert!(!monitor.contains("PrimaryWindow"));
        assert!(!monitor.contains("appkit_window_ptr"));
    }

    #[test]
    fn startup_activation_waits_for_visible_window() {
        let source = include_str!("runtime/macos.rs")
            .split("fn activate_primary_window_on_startup")
            .nth(1)
            .and_then(|tail| tail.split("fn grab_key_window_on_pane_hover").next())
            .unwrap_or_default();

        assert!(source.contains("if !window.visible"));
    }

    #[test]
    fn app_activation_starts_during_boot() {
        let update = platform_systems(Update);
        assert!(
            update.contains(&"activate_app_during_boot".to_string()),
            "update systems: {update:?}"
        );

        let boot = include_str!("runtime/macos.rs")
            .split("fn activate_app_during_boot")
            .nth(1)
            .and_then(|tail| tail.split("type NativeThrottle").next())
            .unwrap_or_default();
        assert!(boot.contains("APP_ACTIVATION_BUDGET"));
        assert!(boot.contains("WinitUserEvent::WakeUp"));
    }

    #[test]
    fn native_mouse_down_offers_the_click_to_the_page() {
        let source = include_str!("runtime/macos.rs");
        let monitor = source
            .split("fn install_native_mouse_wake_monitor")
            .nth(1)
            .and_then(|tail| tail.split("fn install_live_resize_monitor").next())
            .unwrap_or_default();

        assert!(monitor.contains("event_location_in_window_physical_px"));
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

    #[test]
    fn hide_lifecycle_suspends_osr_webviews() {
        let source = include_str!("runtime.rs");

        assert!(source.contains("hide_all_osr_webviews(world)"));
        assert!(source.contains("set_all_osr_hidden"));
    }
}
