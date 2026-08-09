use super::*;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<WindowFullscreen>();
    app.world_mut().spawn((
        Window {
            resolution: (1200, 800).into(),
            position: WindowPosition::At(IVec2::new(40, 60)),
            ..default()
        },
        PrimaryWindow,
    ));
    app
}

#[test]
fn apply_geometry_sets_window_position_and_size() {
    let mut app = app();
    app.add_systems(Update, apply_geometry_on_load);
    app.world_mut().spawn(WindowGeometry {
        fullscreen: false,
        position: Some(IVec2::new(123, 456)),
        size: Some(Vec2::new(640.0, 480.0)),
    });
    app.update();

    let window = app
        .world_mut()
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .single(app.world())
        .unwrap();
    assert!(matches!(window.position, WindowPosition::At(p) if p == IVec2::new(123, 456)));
    assert_eq!(window.resolution.physical_width(), 640);
    assert_eq!(window.resolution.physical_height(), 480);
}

#[test]
fn apply_geometry_inserts_pending_fullscreen_intent() {
    let mut app = app();
    app.add_systems(Update, apply_geometry_on_load);
    app.world_mut().spawn(WindowGeometry {
        fullscreen: true,
        position: None,
        size: None,
    });
    app.update();

    let pending = app.world().get_resource::<PendingFullscreenRestore>();
    assert!(pending.is_some_and(|p| p.0));
}

#[test]
fn capture_records_windowed_frame_when_not_fullscreen() {
    let mut app = app();
    app.world_mut().spawn(WindowGeometry::default());
    app.add_systems(Update, capture_window_geometry);
    app.update();

    let geom = app
        .world_mut()
        .query::<&WindowGeometry>()
        .single(app.world())
        .unwrap();
    assert_eq!(geom.position, Some(IVec2::new(40, 60)));
    assert_eq!(geom.size, Some(Vec2::new(1200.0, 800.0)));
    assert!(!geom.fullscreen);
}

#[test]
fn capture_preserves_windowed_frame_while_fullscreen() {
    let mut app = app();
    app.insert_resource(WindowFullscreen(true));
    app.world_mut().spawn(WindowGeometry {
        fullscreen: false,
        position: Some(IVec2::new(7, 8)),
        size: Some(Vec2::new(900.0, 600.0)),
    });
    app.add_systems(Update, capture_window_geometry);
    app.update();

    let geom = app
        .world_mut()
        .query::<&WindowGeometry>()
        .single(app.world())
        .unwrap();
    assert!(geom.fullscreen);
    assert_eq!(geom.position, Some(IVec2::new(7, 8)));
    assert_eq!(geom.size, Some(Vec2::new(900.0, 600.0)));
}

#[cfg(not(all(target_os = "macos", feature = "native-glass")))]
#[test]
fn window_mode_restore_marks_geometry_capture_ready() {
    let mut app = app();
    app.insert_resource(PendingFullscreenRestore(false))
        .add_systems(Update, restore_fullscreen_from_window_mode);
    app.update();

    assert!(app.world().contains_resource::<WindowRestoreComplete>());
    assert!(!app.world().contains_resource::<PendingFullscreenRestore>());
}
