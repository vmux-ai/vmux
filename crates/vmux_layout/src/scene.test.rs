use super::*;
#[test]
fn camera_margin_is_zero() {
    assert_eq!(camera_margin_px(), 0.0);
}

#[test]
fn main_camera_uses_non_lut_tonemapping() {
    let mut app = App::new();
    app.register_required_components::<Camera3d, bevy::core_pipeline::tonemapping::Tonemapping>()
        .add_systems(Update, setup);
    app.world_mut().spawn((Window::default(), PrimaryWindow));

    app.update();

    let mut query = app
        .world_mut()
        .query_filtered::<&bevy::core_pipeline::tonemapping::Tonemapping, With<MainCamera>>();
    let tonemapping = query.single(app.world()).expect("main camera tonemapping");

    assert_eq!(
        *tonemapping,
        bevy::core_pipeline::tonemapping::Tonemapping::SomewhatBoringDisplayTransform
    );
}

#[test]
fn main_camera_grabs_cursor_with_left_mouse_drag() {
    let mut app = App::new();
    app.register_required_components::<Camera3d, FreeCamera>()
        .add_systems(Update, setup);
    app.world_mut().spawn((Window::default(), PrimaryWindow));

    app.update();

    let config = app
        .world_mut()
        .query_filtered::<&FreeCamera, With<MainCamera>>()
        .single(app.world())
        .expect("main camera free camera config");

    assert_eq!(config.mouse_key_cursor_grab, MouseButton::Left);
}

#[test]
fn scene_plugin_chains_exit_transition_systems_in_order() {
    let source = include_str!("scene.rs");
    let update_registration = source
        .split("impl Plugin for ScenePlugin")
        .nth(1)
        .and_then(|tail| tail.split("pub fn setup").next())
        .and_then(|build| build.split(".add_systems(\n                Update,").nth(1))
        .and_then(|update| {
            update
                .split(".add_systems(\n                PostUpdate,")
                .next()
        })
        .unwrap_or_default();
    let systems = [
        "on_interactive_mode_command.in_set(ReadAppCommands)",
        "suppress_free_camera_when_pane_active",
        "tick_mode_transition",
        "fade_bloom_and_light",
        "setup_exit_camera_animation",
        "start_pending_animation",
        "complete_mode_transition",
    ];
    let mut remainder = update_registration;

    for system in systems {
        let index = remainder
            .find(system)
            .expect("transition system registered");
        remainder = &remainder[index + system.len()..];
    }

    assert!(remainder.contains(".chain()"));
}

#[test]
fn exit_transition_wires_main_camera_animation_target() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<AnimationClip>>()
        .init_resource::<Assets<AnimationGraph>>()
        .insert_resource(CameraHome(Transform::IDENTITY))
        .insert_resource(ModeTransition::new(TransitionDirection::ExitPlayer))
        .add_systems(Update, setup_exit_camera_animation);

    let camera = app
        .world_mut()
        .spawn((MainCamera, Transform::from_xyz(3.0, 2.0, 1.0)))
        .id();

    app.update();

    let target = AnimationTargetId::from_name(&Name::new("main_camera"));
    assert_eq!(app.world().get::<AnimationTargetId>(camera), Some(&target));
    assert_eq!(
        app.world()
            .get::<AnimatedBy>(camera)
            .map(|animated| animated.0),
        Some(camera)
    );
}

#[test]
fn exiting_player_mode_resets_free_camera_state() {
    let mut app = App::new();
    let mut transition = ModeTransition::new(TransitionDirection::ExitPlayer);
    transition
        .timer
        .tick(std::time::Duration::from_secs_f32(TRANSITION_DURATION));
    let home = Transform::from_xyz(1.0, 2.0, 3.0);
    let (_, node) = AnimationGraph::from_clip(Handle::<AnimationClip>::default());
    let mut state = FreeCameraState::default();
    state.enabled = false;
    state.pitch = 1.0;
    state.yaw = 2.0;
    state.speed_multiplier = 3.0;
    state.velocity = Vec3::new(4.0, 5.0, 6.0);

    app.add_plugins(MinimalPlugins)
        .insert_resource(InteractionMode::Player)
        .insert_resource(transition)
        .insert_resource(CameraHome(home))
        .insert_resource(PendingAnimationStart(node))
        .add_systems(Update, complete_mode_transition);
    app.world_mut()
        .spawn((Window::default(), PrimaryWindow, Transform::default()));
    let camera = app
        .world_mut()
        .spawn((MainCamera, Transform::default(), state, Bloom::NATURAL))
        .id();
    app.world_mut().entity_mut(camera).insert((
        AnimationPlayer::default(),
        AnimationGraphHandle(Handle::<AnimationGraph>::default()),
        Name::new("main_camera"),
        AnimationTargetId::from_name(&Name::new("main_camera")),
        AnimatedBy(camera),
    ));
    app.world_mut().spawn(SceneSunlight);
    let window_entity = app
        .world_mut()
        .spawn((
            crate::window::VmuxWindow,
            Transform::from_xyz(9.0, 9.0, 9.0).with_scale(Vec3::splat(9.0)),
        ))
        .id();

    app.update();

    assert!(*app.world().resource::<InteractionMode>() == InteractionMode::User);
    assert!(!app.world().contains_resource::<ModeTransition>());
    assert!(!app.world().contains_resource::<CameraHome>());
    assert!(!app.world().contains_resource::<PendingAnimationStart>());
    assert!(app.world().get::<Bloom>(camera).is_none());
    assert!(app.world().get::<AnimationPlayer>(camera).is_none());
    assert!(app.world().get::<AnimationGraphHandle>(camera).is_none());
    assert!(app.world().get::<AnimationTargetId>(camera).is_none());
    assert!(app.world().get::<AnimatedBy>(camera).is_none());
    assert!(app.world().get::<Name>(camera).is_none());
    assert_eq!(app.world().get::<Transform>(camera), Some(&home));
    let mut sunlight_q = app
        .world_mut()
        .query_filtered::<Entity, With<SceneSunlight>>();
    assert!(sunlight_q.iter(app.world()).next().is_none());

    let state = app
        .world_mut()
        .query_filtered::<&FreeCameraState, With<MainCamera>>()
        .single(app.world())
        .expect("main camera state");
    assert!(!state.enabled);
    assert_eq!(state.pitch, 0.0);
    assert_eq!(state.yaw, 0.0);
    assert_eq!(state.speed_multiplier, 1.0);
    assert_eq!(state.velocity, Vec3::ZERO);
    assert!(state.rotation_curve.is_none());

    let window = app
        .world_mut()
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .single(app.world())
        .expect("primary window");
    let expected_window_transform = frame_window_transform(window);
    let window_transform = app
        .world()
        .get::<Transform>(window_entity)
        .expect("window transform");
    assert_eq!(*window_transform, expected_window_transform);
}
