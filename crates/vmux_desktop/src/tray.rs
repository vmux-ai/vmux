use bevy::prelude::*;
use parking_lot::Mutex;
use std::sync::LazyLock;
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

use crate::background_lifecycle::LifecycleEvent;
#[cfg(feature = "recording")]
use crate::recording::{RecordingControl, RecordingStatus};

pub(crate) static PENDING_TRAY_EVENTS: LazyLock<Mutex<Vec<String>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub(crate) struct TrayPlugin;

struct TrayHandle {
    _tray: TrayIcon,
    toggle: MenuItem,
    toggle_id: String,
    quit_id: String,
    #[cfg(feature = "recording")]
    pause: MenuItem,
    #[cfg(feature = "recording")]
    pause_id: String,
    #[cfg(feature = "recording")]
    resume: MenuItem,
    #[cfg(feature = "recording")]
    resume_id: String,
    #[cfg(feature = "recording")]
    done: MenuItem,
    #[cfg(feature = "recording")]
    done_id: String,
    last_any_visible: Option<bool>,
    #[cfg(feature = "recording")]
    last_status: Option<RecordingStatus>,
}

impl Plugin for TrayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_tray).add_systems(
            Update,
            (drain_tray_events, sync_tray_menu_state, sync_tray_recording),
        );
    }
}

fn setup_tray(world: &mut World) {
    let menu = Menu::new();
    let toggle = MenuItem::new(toggle_label(true), true, None);
    #[cfg(feature = "recording")]
    let pause = MenuItem::new("Pause Recording", false, None);
    #[cfg(feature = "recording")]
    let resume = MenuItem::new("Resume Recording", false, None);
    #[cfg(feature = "recording")]
    let done = MenuItem::new("Finish Recording", false, None);
    let quit = MenuItem::new("Quit Vmux", true, None);
    let toggle_id = toggle.id().0.clone();
    let quit_id = quit.id().0.clone();
    #[cfg(feature = "recording")]
    let pause_id = pause.id().0.clone();
    #[cfg(feature = "recording")]
    let resume_id = resume.id().0.clone();
    #[cfg(feature = "recording")]
    let done_id = done.id().0.clone();

    #[cfg(feature = "recording")]
    let append_result = menu.append_items(&[
        &toggle,
        &PredefinedMenuItem::separator(),
        &pause,
        &resume,
        &done,
        &PredefinedMenuItem::separator(),
        &quit,
    ]);
    #[cfg(not(feature = "recording"))]
    let append_result = menu.append_items(&[&toggle, &PredefinedMenuItem::separator(), &quit]);
    if let Err(e) = append_result {
        tracing::error!(error = %e, "failed to append tray menu items");
        return;
    }

    let icon = load_tray_icon();
    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Vmux")
        .with_icon(icon)
        .with_icon_as_template(true)
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "failed to build tray icon");
            return;
        }
    };

    world.insert_non_send(TrayHandle {
        _tray: tray,
        toggle,
        toggle_id,
        quit_id,
        #[cfg(feature = "recording")]
        pause,
        #[cfg(feature = "recording")]
        pause_id,
        #[cfg(feature = "recording")]
        resume,
        #[cfg(feature = "recording")]
        resume_id,
        #[cfg(feature = "recording")]
        done,
        #[cfg(feature = "recording")]
        done_id,
        last_any_visible: None,
        #[cfg(feature = "recording")]
        last_status: None,
    });
}

fn drain_tray_events(
    handle: Option<NonSend<TrayHandle>>,
    windows: Query<&Window>,
    mut events: MessageWriter<LifecycleEvent>,
    #[cfg(feature = "recording")] mut controls: MessageWriter<RecordingControl>,
) {
    let Some(handle) = handle else { return };
    let drained = std::mem::take(&mut *PENDING_TRAY_EVENTS.lock());
    let any_visible = windows.iter().any(|w| w.visible);
    for event_id in drained {
        if event_id == handle.toggle_id {
            events.write(toggle_lifecycle_event(any_visible));
        } else if event_id == handle.quit_id {
            events.write(LifecycleEvent::QuitVmux);
        } else {
            #[cfg(feature = "recording")]
            if event_id == handle.pause_id {
                controls.write(RecordingControl::Pause);
                continue;
            } else if event_id == handle.resume_id {
                controls.write(RecordingControl::Resume);
                continue;
            } else if event_id == handle.done_id {
                controls.write(RecordingControl::Done);
                continue;
            }
            tracing::debug!(id = %event_id, "unhandled tray menu event id");
        }
    }
}

fn sync_tray_menu_state(handle: Option<NonSendMut<TrayHandle>>, windows: Query<&Window>) {
    let Some(mut handle) = handle else { return };
    let any_visible = windows.iter().any(|w| w.visible);
    if handle.last_any_visible == Some(any_visible) {
        return;
    }
    handle.last_any_visible = Some(any_visible);
    handle.toggle.set_text(toggle_label(any_visible));
}

#[cfg(feature = "recording")]
fn sync_tray_recording(status: Res<RecordingStatus>, handle: Option<NonSendMut<TrayHandle>>) {
    let Some(mut handle) = handle else { return };
    if handle.last_status == Some(*status) {
        return;
    }
    handle.last_status = Some(*status);

    let recording = !matches!(*status, RecordingStatus::Idle);
    // Template mode renders monochrome; turn it off so the red REC dot shows.
    handle._tray.set_icon_as_template(!recording);
    let icon = if recording {
        load_tray_icon_recording()
    } else {
        load_tray_icon()
    };
    let _ = handle._tray.set_icon(Some(icon));

    handle
        .pause
        .set_enabled(matches!(*status, RecordingStatus::Recording));
    handle
        .resume
        .set_enabled(matches!(*status, RecordingStatus::Paused));
    handle.done.set_enabled(recording);
}

#[cfg(not(feature = "recording"))]
fn sync_tray_recording() {}

fn toggle_label(any_visible: bool) -> &'static str {
    if any_visible {
        "Close Window"
    } else {
        "Open Window"
    }
}

fn toggle_lifecycle_event(any_visible: bool) -> LifecycleEvent {
    if any_visible {
        LifecycleEvent::HideAllWindows
    } else {
        LifecycleEvent::ShowAllWindows
    }
}

fn load_tray_icon() -> tray_icon::Icon {
    let rgba = tray_icon_rgba();
    tray_icon::Icon::from_rgba(rgba, 16, 16).expect("valid placeholder rgba")
}

#[cfg(feature = "recording")]
fn load_tray_icon_recording() -> tray_icon::Icon {
    let rgba = tray_icon_recording_rgba();
    tray_icon::Icon::from_rgba(rgba, 16, 16).expect("valid recording rgba")
}

/// A solid red dot — the universal "recording" indicator. Shown with template
/// mode off so the red is preserved.
#[cfg(feature = "recording")]
fn tray_icon_recording_rgba() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    let (cx, cy, r) = (7.5_f32, 7.5_f32, 5.5_f32);
    for y in 0..16 {
        for x in 0..16 {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= r * r {
                rgba.extend_from_slice(&[224, 38, 38, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    rgba
}

fn tray_icon_rgba() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0_i32..16 {
        for x in 0_i32..16 {
            let dy = y - 3;
            let visible = (0..=10).contains(&dy)
                && ((x - (3 + dy / 2)).abs() <= 1 || (x - (12 - dy / 2)).abs() <= 1);
            if visible {
                rgba.extend_from_slice(&[0, 0, 0, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    #[test]
    fn tray_module_not_a_placeholder() {
        let source = include_str!("tray.rs");
        let tray_builder = ["Tray", "Icon", "Builder"].concat();
        let tray_type = ["tray_icon", "::", "Tray", "Icon"].concat();
        assert!(
            source.contains(&tray_builder) || source.contains(&tray_type),
            "tray.rs must wire tray-icon, not be a stub"
        );
        let open_needle = ["\"Open ", "Window\""].concat();
        assert!(
            source.contains(&open_needle),
            "tray toggle must expose an 'Open Window' label"
        );
        let close_needle = ["\"Close ", "Window\""].concat();
        assert!(
            source.contains(&close_needle),
            "tray toggle must expose a 'Close Window' label"
        );
        let quit_needle = ["\"Quit ", "Vmux\""].concat();
        assert!(
            source.contains(&quit_needle),
            "tray must expose a 'Quit Vmux' menu item"
        );
    }

    #[test]
    fn toggle_label_reflects_visibility() {
        assert_eq!(super::toggle_label(true), "Close Window");
        assert_eq!(super::toggle_label(false), "Open Window");
    }

    #[test]
    fn toggle_event_routes_by_visibility() {
        use super::LifecycleEvent;
        assert!(matches!(
            super::toggle_lifecycle_event(true),
            LifecycleEvent::HideAllWindows
        ));
        assert!(matches!(
            super::toggle_lifecycle_event(false),
            LifecycleEvent::ShowAllWindows
        ));
    }

    #[test]
    fn tray_syncs_toggle_label_with_window_visibility() {
        let source = include_str!("tray.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source");

        assert!(source.contains("sync_tray_menu_state"));
        assert!(source.contains("set_text"));
        assert!(source.contains("toggle_label"));
    }

    #[test]
    fn tray_icon_has_visible_pixels() {
        let rgba = super::tray_icon_rgba();

        assert_eq!(rgba.len(), 16 * 16 * 4);
        assert!(
            rgba.chunks_exact(4).any(|pixel| pixel[3] != 0),
            "tray icon must not be fully transparent"
        );
    }

    #[test]
    fn tray_icon_uses_macos_template_mode() {
        let source = include_str!("tray.rs");

        assert!(source.contains("with_icon_as_template(true)"));
    }
}
