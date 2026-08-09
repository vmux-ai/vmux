use bevy::prelude::*;
use bevy::window::{Monitor, MonitorSelection, PrimaryWindow, Window, WindowPosition};

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, relocate_window_to_live_display);
    }
}

pub(crate) struct DisplayPlugin;

fn monitor_rect(monitor: &Monitor) -> IRect {
    let min = monitor.physical_position;
    let size = IVec2::new(
        monitor.physical_width as i32,
        monitor.physical_height as i32,
    );
    IRect::from_corners(min, min + size)
}

fn window_off_all_monitors(window: IRect, monitors: &[IRect]) -> bool {
    monitors.iter().all(|m| m.intersect(window).is_empty())
}

/// When the monitor set changes (sleep/wake, unplug), recenter the primary window on the primary
/// display if its frame no longer intersects any live monitor. With zero monitors (mid-sleep) there
/// is nothing to place onto, so we wait for a monitor to reappear.
fn relocate_window_to_live_display(
    monitors_added: Query<(), Added<Monitor>>,
    monitors_removed: RemovedComponents<Monitor>,
    monitors: Query<&Monitor>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) {
    if monitors_added.is_empty() && monitors_removed.is_empty() {
        return;
    }
    if monitors.is_empty() {
        return;
    }
    let Ok(mut window) = window.single_mut() else {
        return;
    };
    let WindowPosition::At(pos) = window.position else {
        return;
    };
    let size = window.resolution.physical_size().as_ivec2();
    let window_rect = IRect::from_corners(pos, pos + size);
    let monitor_rects: Vec<IRect> = monitors.iter().map(monitor_rect).collect();
    if window_off_all_monitors(window_rect, &monitor_rects) {
        window.position = WindowPosition::Centered(MonitorSelection::Primary);
    }
}

#[cfg(test)]
#[path = "display.test.rs"]
mod tests;
