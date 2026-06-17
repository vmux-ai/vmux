#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use bevy::window::{Monitor, OnMonitor, Window};

    fn test_monitor(x: i32, y: i32, w: u32, h: u32) -> Monitor {
        Monitor {
            name: Some("test".to_string()),
            physical_height: h,
            physical_width: w,
            physical_position: IVec2::new(x, y),
            refresh_rate_millihertz: Some(60_000),
            scale_factor: 1.0,
            video_modes: Vec::new(),
        }
    }

    #[test]
    fn despawning_monitor_keeps_linked_window() {
        let mut world = World::new();
        let monitor = world.spawn(test_monitor(0, 0, 1920, 1080)).id();
        let window = world.spawn(Window::default()).id();
        world.entity_mut(window).insert(OnMonitor(monitor));

        world.entity_mut(monitor).despawn();

        assert!(
            world.get_entity(window).is_ok(),
            "window must survive monitor despawn (linked_spawn cascade must be gone)"
        );
    }
}
