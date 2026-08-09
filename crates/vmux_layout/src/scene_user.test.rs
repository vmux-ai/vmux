use super::*;

#[test]
fn user_camera_uses_fixed_vertical_projection() {
    let mut app = App::new();
    app.add_systems(Update, setup);
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    app.update();

    let projection = app
        .world_mut()
        .query_filtered::<&Projection, With<MainCamera>>()
        .single(app.world())
        .expect("main camera projection");

    assert!(matches!(
        projection,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { .. },
            ..
        })
    ));
}
