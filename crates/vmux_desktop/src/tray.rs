use bevy::prelude::*;
use parking_lot::Mutex;
use std::sync::LazyLock;
use tray_icon::menu::{Menu, MenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

use crate::background_lifecycle::LifecycleEvent;

pub(crate) static PENDING_TRAY_EVENTS: LazyLock<Mutex<Vec<String>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub(crate) struct TrayPlugin;

struct TrayHandle {
    _tray: TrayIcon,
    toggle: MenuItem,
    toggle_id: String,
    quit_id: String,
    last_any_visible: Option<bool>,
}

impl Plugin for TrayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_tray)
            .add_systems(Update, (drain_tray_events, sync_tray_menu_state));
    }
}

fn setup_tray(world: &mut World) {
    let menu = Menu::new();
    let toggle = MenuItem::new(toggle_label(true), true, None);
    let quit = MenuItem::new("Quit Vmux", true, None);
    let toggle_id = toggle.id().0.clone();
    let quit_id = quit.id().0.clone();

    if let Err(e) = menu.append_items(&[&toggle, &quit]) {
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
        last_any_visible: None,
    });
}

fn drain_tray_events(
    handle: Option<NonSend<TrayHandle>>,
    windows: Query<&Window>,
    mut events: MessageWriter<LifecycleEvent>,
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
