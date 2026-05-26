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
    show_id: String,
    quit_id: String,
}

impl Plugin for TrayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_tray)
            .add_systems(Update, drain_tray_events);
    }
}

fn setup_tray(world: &mut World) {
    let menu = Menu::new();
    let show = MenuItem::new("Show Vmux", true, None);
    let quit = MenuItem::new("Quit Vmux", true, None);
    let show_id = show.id().0.clone();
    let quit_id = quit.id().0.clone();

    if let Err(e) = menu.append_items(&[&show, &quit]) {
        tracing::error!(error = %e, "failed to append tray menu items");
        return;
    }

    let icon = load_tray_icon();
    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Vmux")
        .with_icon(icon)
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "failed to build tray icon");
            return;
        }
    };

    world.insert_non_send_resource(TrayHandle {
        _tray: tray,
        show_id,
        quit_id,
    });
}

fn drain_tray_events(
    handle: Option<NonSend<TrayHandle>>,
    mut events: MessageWriter<LifecycleEvent>,
) {
    let Some(handle) = handle else { return };
    let drained = std::mem::take(&mut *PENDING_TRAY_EVENTS.lock());
    for event_id in drained {
        if event_id == handle.show_id {
            events.write(LifecycleEvent::ShowAllWindows);
        } else if event_id == handle.quit_id {
            events.write(LifecycleEvent::QuitVmux);
        } else {
            tracing::debug!(id = %event_id, "unhandled tray menu event id");
        }
    }
}

fn load_tray_icon() -> tray_icon::Icon {
    let rgba = vec![0u8; 16 * 16 * 4];
    tray_icon::Icon::from_rgba(rgba, 16, 16).expect("valid placeholder rgba")
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
        let show_needle = ["\"Show", " Vm", "ux\""].concat();
        assert!(
            source.contains(&show_needle),
            "tray must expose a 'Show Vmux' menu item"
        );
        let quit_needle = ["\"Quit", " Vm", "ux\""].concat();
        assert!(
            source.contains(&quit_needle),
            "tray must expose a 'Quit Vmux' menu item"
        );
    }
}
