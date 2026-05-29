use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy::window::{Monitor, Window};
use bevy::winit::{UpdateMode, WinitSettings};
use bevy_cef_core::prelude::{
    Browsers, MessageLoopWakePolicy, windowless_frame_interval_from_refresh_millihertz,
};
use std::time::Duration;

use vmux_terminal as terminal;
use vmux_terminal::{PtyExited, Terminal};

const FOCUSED_FRAME_INTERVAL: Duration = Duration::from_secs(1);
const UNFOCUSED_FRAME_INTERVAL: Duration = Duration::from_secs(1);
const HIDDEN_FRAME_INTERVAL: Duration = Duration::from_secs(60);
const BACKGROUND_CEF_WAKE_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Message, Debug, Clone, Copy)]
pub enum LifecycleEvent {
    HideAllWindows,
    ShowAllWindows,
    QuitVmux,
}

pub struct BackgroundLifecyclePlugin;

impl Plugin for BackgroundLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LifecycleEvent>()
            .add_systems(Update, handle_lifecycle_events)
            .add_systems(Update, sync_winit_power_mode.after(handle_lifecycle_events));
    }
}

pub(crate) fn foreground_winit_settings() -> WinitSettings {
    WinitSettings {
        focused_mode: UpdateMode::Reactive {
            wait: FOCUSED_FRAME_INTERVAL,
            react_to_device_events: true,
            react_to_user_events: true,
            react_to_window_events: true,
        },
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
    let next = if all_hidden {
        hidden_winit_settings()
    } else {
        foreground_winit_settings()
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

fn foreground_cef_wake_interval(refresh_rates: impl IntoIterator<Item = Option<u32>>) -> Duration {
    windowless_frame_interval_from_refresh_millihertz(refresh_rates.into_iter().flatten().max())
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
            LifecycleEvent::ShowAllWindows => {
                let mut q = world.query::<&mut Window>();
                for mut w in q.iter_mut(world) {
                    w.visible = true;
                }
            }
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
        let source = include_str!("background_lifecycle.rs");
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
                if path.ends_with("background_lifecycle.rs") {
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
        let settings = foreground_winit_settings();

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
        assert!(react_to_device_events);
        assert!(react_to_user_events);
        assert!(react_to_window_events);
        assert_eq!(
            settings.unfocused_mode,
            UpdateMode::reactive_low_power(Duration::from_secs(1))
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
    fn cef_wake_policy_matches_display_refresh() {
        assert_eq!(
            foreground_cef_wake_interval([Some(60_000)]),
            Duration::from_nanos(16_666_666)
        );
        assert!(foreground_cef_wake_interval([Some(144_000)]) < Duration::from_millis(8));
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
        let source = include_str!("background_lifecycle.rs");

        assert!(source.contains("hide_all_osr_webviews(world)"));
        assert!(source.contains("set_all_osr_hidden"));
    }
}
