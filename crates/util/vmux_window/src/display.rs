use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::{Added, Query, RemovedComponents};
use bevy_math::{IRect, IVec2};
use bevy_window::{Monitor, MonitorSelection, Window, WindowPosition};

pub struct DisplayRelocationPlugin;

impl Plugin for DisplayRelocationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, relocate);
    }
}

impl DisplayRelocationPlugin {
    fn monitor_rect(monitor: &Monitor) -> IRect {
        let min = monitor.physical_position;
        let size = IVec2::new(
            monitor.physical_width as i32,
            monitor.physical_height as i32,
        );
        IRect::from_corners(min, min + size)
    }

    fn window_off_all_monitors(window: IRect, monitors: &[IRect]) -> bool {
        monitors
            .iter()
            .all(|monitor| monitor.intersect(window).is_empty())
    }
}

fn relocate(
    monitors_added: Query<(), Added<Monitor>>,
    monitors_removed: RemovedComponents<Monitor>,
    monitors: Query<&Monitor>,
    mut windows: Query<&mut Window>,
) {
    if monitors_added.is_empty() && monitors_removed.is_empty() {
        return;
    }
    if monitors.is_empty() {
        return;
    }
    let monitor_rects: Vec<IRect> = monitors
        .iter()
        .map(DisplayRelocationPlugin::monitor_rect)
        .collect();
    for mut window in &mut windows {
        let WindowPosition::At(position) = window.position else {
            continue;
        };
        let size = window.resolution.physical_size().as_ivec2();
        let window_rect = IRect::from_corners(position, position + size);
        if DisplayRelocationPlugin::window_off_all_monitors(window_rect, &monitor_rects) {
            window.position = WindowPosition::Centered(MonitorSelection::Primary);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_monitor(x: i32, y: i32, width: u32, height: u32) -> Monitor {
        Monitor {
            name: None,
            physical_position: IVec2::new(x, y),
            physical_width: width,
            physical_height: height,
            refresh_rate_millihertz: Some(60_000),
            scale_factor: 1.0,
            video_modes: Vec::new(),
        }
    }

    #[test]
    fn relocates_offscreen_window_and_preserves_visible_window() {
        let mut app = App::new();
        app.add_plugins(DisplayRelocationPlugin);
        app.world_mut().spawn(test_monitor(0, 0, 1920, 1080));
        let offscreen = app
            .world_mut()
            .spawn(Window {
                position: WindowPosition::At(IVec2::new(3000, 100)),
                resolution: (800, 600).into(),
                ..Default::default()
            })
            .id();
        let visible = app
            .world_mut()
            .spawn(Window {
                position: WindowPosition::At(IVec2::new(100, 100)),
                resolution: (800, 600).into(),
                ..Default::default()
            })
            .id();
        app.update();

        assert_eq!(
            app.world().get::<Window>(offscreen).unwrap().position,
            WindowPosition::Centered(MonitorSelection::Primary)
        );
        assert_eq!(
            app.world().get::<Window>(visible).unwrap().position,
            WindowPosition::At(IVec2::new(100, 100))
        );
    }
}
