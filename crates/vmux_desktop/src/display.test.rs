use super::*;
use bevy::window::OnMonitor;

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

#[test]
fn off_all_monitors_detects_stranded_window() {
    let monitor = IRect::from_corners(IVec2::ZERO, IVec2::new(1920, 1080));
    let stranded = IRect::from_corners(IVec2::new(5000, 5000), IVec2::new(6280, 5720));
    let overlapping = IRect::from_corners(IVec2::new(100, 100), IVec2::new(1380, 820));

    assert!(window_off_all_monitors(stranded, &[monitor]));
    assert!(!window_off_all_monitors(overlapping, &[monitor]));
    assert!(window_off_all_monitors(stranded, &[]));
}

fn relocate_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, relocate_window_to_live_display);
    app
}

fn spawn_primary_window(app: &mut App, position: IVec2) -> Entity {
    app.world_mut()
        .spawn((
            Window {
                position: WindowPosition::At(position),
                ..default()
            },
            PrimaryWindow,
        ))
        .id()
}

#[test]
fn stranded_window_is_recentered_on_primary() {
    let mut app = relocate_app();
    let window = spawn_primary_window(&mut app, IVec2::new(5000, 5000));
    app.world_mut().spawn(test_monitor(0, 0, 1920, 1080));

    app.update();

    assert!(matches!(
        app.world().get::<Window>(window).unwrap().position,
        WindowPosition::Centered(MonitorSelection::Primary)
    ));
}

#[test]
fn window_on_a_monitor_is_left_in_place() {
    let mut app = relocate_app();
    let window = spawn_primary_window(&mut app, IVec2::new(100, 100));
    app.world_mut().spawn(test_monitor(0, 0, 1920, 1080));

    app.update();

    assert!(matches!(
        app.world().get::<Window>(window).unwrap().position,
        WindowPosition::At(p) if p == IVec2::new(100, 100)
    ));
}

#[test]
fn zero_monitors_does_not_relocate() {
    let mut app = relocate_app();
    let window = spawn_primary_window(&mut app, IVec2::new(100, 100));
    let monitor = app.world_mut().spawn(test_monitor(0, 0, 1920, 1080)).id();
    app.update();

    app.world_mut().get_mut::<Window>(window).unwrap().position =
        WindowPosition::At(IVec2::new(5000, 5000));
    app.world_mut().entity_mut(monitor).despawn();
    app.update();

    assert!(matches!(
        app.world().get::<Window>(window).unwrap().position,
        WindowPosition::At(p) if p == IVec2::new(5000, 5000)
    ));
}
