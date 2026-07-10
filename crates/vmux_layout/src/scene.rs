use crate::{
    LayoutStartupSet, fit_window_to_screen,
    unit::{PIXELS_PER_METER, WindowExt},
};
use bevy::{
    animation::{AnimatedBy, AnimationTargetId, animated_field, prelude::*},
    camera::PerspectiveProjection,
    camera::Projection,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin, FreeCameraState},
    core_pipeline::tonemapping::Tonemapping,
    math::curve::easing::EasingCurve,
    post_process::bloom::Bloom,
    prelude::*,
    window::PrimaryWindow,
};
use bevy_cef::prelude::CefKeyboardTarget;
use vmux_command::{AppCommand, ReadAppCommands, SceneCommand, SceneInteractiveModeCommand};

pub const FOV_Y: f32 = std::f32::consts::FRAC_PI_4;

const TRANSITION_DURATION: f32 = 0.3;
const BLOOM_INTENSITY: f32 = 0.15; // Bloom::NATURAL intensity
const SUNLIGHT_ILLUMINANCE: f32 = 8000.0;

fn camera_margin_px() -> f32 {
    0.0
}

#[derive(Component)]
pub struct MainCamera;

#[derive(Resource, Default, PartialEq, Eq, Clone, Copy)]
pub enum InteractionMode {
    #[default]
    User,
    Player,
}

#[derive(Resource)]
pub struct CameraHome(pub Transform);

#[derive(Resource)]
pub struct ModeTransition {
    pub direction: TransitionDirection,
    pub timer: Timer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TransitionDirection {
    EnterPlayer,
    ExitPlayer,
}

impl ModeTransition {
    pub fn new(direction: TransitionDirection) -> Self {
        Self {
            direction,
            timer: Timer::from_seconds(TRANSITION_DURATION, TimerMode::Once),
        }
    }

    pub fn progress(&self) -> f32 {
        self.timer.fraction()
    }
}

#[derive(Resource)]
struct PendingAnimationStart(AnimationNodeIndex);

#[derive(Component)]
struct SceneSunlight;

#[derive(Default)]
pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FreeCameraPlugin)
            .init_resource::<InteractionMode>()
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup.in_set(LayoutStartupSet::Window))
            .add_systems(
                Startup,
                fit_main_camera
                    .after(fit_window_to_screen)
                    .in_set(LayoutStartupSet::Post),
            )
            .add_systems(
                Update,
                (
                    on_interactive_mode_command.in_set(ReadAppCommands),
                    suppress_free_camera_when_pane_active,
                    tick_mode_transition,
                    fade_bloom_and_light,
                    setup_exit_camera_animation,
                    start_pending_animation,
                    complete_mode_transition,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                (
                    fit_main_camera.after(fit_window_to_screen),
                    update_camera_home.after(fit_window_to_screen),
                ),
            );

        #[cfg(target_os = "macos")]
        app.insert_resource(ClearColor(Color::NONE));
    }
}

pub fn setup(mut commands: Commands, window: Single<&Window, With<PrimaryWindow>>) {
    let mut state = FreeCameraState::default();
    state.enabled = false;

    commands.spawn((
        MainCamera,
        Camera3d::default(),
        Tonemapping::SomewhatBoringDisplayTransform,
        Projection::Perspective(PerspectiveProjection {
            fov: FOV_Y,
            ..default()
        }),
        frame_main_camera_transform(&window, window.aspect(), camera_margin_px()),
        FreeCamera {
            sensitivity: 1.0,
            friction: 25.0,
            walk_speed: 0.5,
            run_speed: 5.0,
            mouse_key_cursor_grab: MouseButton::Left,
            ..default()
        },
        state,
    ));
}

fn fit_main_camera(
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera_q: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
    camera_state: Single<&FreeCameraState, With<MainCamera>>,
    mode: Res<InteractionMode>,
    transition: Option<Res<ModeTransition>>,
) {
    let Ok((mut tf, mut proj)) = camera_q.single_mut() else {
        return;
    };
    let aspect = window.aspect();

    if let Projection::Perspective(ref mut p) = *proj
        && (p.aspect_ratio - aspect).abs() > f32::EPSILON
    {
        p.aspect_ratio = aspect;
    }

    // Skip transform update during transitions or when camera is user-controlled
    if transition.is_some() || camera_state.enabled {
        return;
    }

    // Only reset transform in User mode
    if *mode == InteractionMode::User {
        *tf = frame_main_camera_transform(&window, aspect, camera_margin_px());
    }
}

fn update_camera_home(
    window: Single<&Window, With<PrimaryWindow>>,
    mode: Res<InteractionMode>,
    home: Option<ResMut<CameraHome>>,
) {
    if *mode != InteractionMode::Player {
        return;
    }
    let Some(mut home) = home else { return };
    home.0 = frame_main_camera_transform(&window, window.aspect(), camera_margin_px());
}

// ---------------------------------------------------------------------------
fn on_interactive_mode_command(
    mut reader: MessageReader<AppCommand>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut state: Single<&mut FreeCameraState, With<MainCamera>>,
    camera: Single<Entity, With<MainCamera>>,
    kb_targets: Query<Entity, With<CefKeyboardTarget>>,
    mut mode: ResMut<InteractionMode>,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
    transition: Option<Res<ModeTransition>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Scene(SceneCommand::InteractiveMode(command)) = *cmd else {
            continue;
        };
        let target = match command {
            SceneInteractiveModeCommand::User => InteractionMode::User,
            SceneInteractiveModeCommand::Player => InteractionMode::Player,
            SceneInteractiveModeCommand::Toggle => match *mode {
                InteractionMode::User => InteractionMode::Player,
                InteractionMode::Player => InteractionMode::User,
            },
        };

        if transition.is_some() {
            continue;
        }
        if *mode == target {
            continue;
        }

        match target {
            InteractionMode::User => {
                start_exit_transition(&mut state, &mut suppress, &mut commands, *camera);
            }
            InteractionMode::Player => {
                start_enter_transition(
                    &window,
                    &kb_targets,
                    &mut mode,
                    &mut suppress,
                    &mut commands,
                    *camera,
                );
            }
        }
    }
}

fn start_enter_transition(
    window: &Window,
    kb_targets: &Query<Entity, With<CefKeyboardTarget>>,
    mode: &mut ResMut<InteractionMode>,
    suppress: &mut ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
    commands: &mut Commands,
    camera: Entity,
) {
    let home = frame_main_camera_transform(window, window.aspect(), camera_margin_px());
    commands.insert_resource(CameraHome(home));

    **mode = InteractionMode::Player;
    suppress.0 = true;

    for e in kb_targets {
        commands.entity(e).remove::<CefKeyboardTarget>();
    }

    let mut bloom = Bloom::NATURAL;
    bloom.intensity = 0.0;
    commands.entity(camera).insert(bloom);

    commands.spawn((
        SceneSunlight,
        DirectionalLight {
            illuminance: 0.0,
            shadow_maps_enabled: false,
            color: Color::srgb(1.0, 0.98, 0.95),
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.6, 0.4, 0.0)),
    ));

    commands.insert_resource(ModeTransition::new(TransitionDirection::EnterPlayer));
}

fn reset_free_camera_state(state: &mut FreeCameraState) {
    state.enabled = false;
    state.pitch = 0.0;
    state.yaw = 0.0;
    state.speed_multiplier = 1.0;
    state.velocity = Vec3::ZERO;
    state.rotation_curve = None;
}

fn frame_window_transform(window: &Window) -> Transform {
    let m = window.meters();
    Transform {
        translation: Vec3::new(0.0, m.y * 0.5, 0.0),
        scale: Vec3::new(m.x, m.y, 1.0),
        ..default()
    }
}

fn start_exit_transition(
    state: &mut FreeCameraState,
    suppress: &mut ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
    commands: &mut Commands,
    _camera_entity: Entity,
) {
    // Disable free camera immediately so WASD stops during transition
    reset_free_camera_state(state);
    suppress.0 = false;

    // Start the exit transition timer
    // Camera animation is set up by setup_exit_camera_animation
    commands.insert_resource(ModeTransition::new(TransitionDirection::ExitPlayer));
}

// ---------------------------------------------------------------------------
// Transition systems
// ---------------------------------------------------------------------------

fn tick_mode_transition(time: Res<Time>, transition: Option<ResMut<ModeTransition>>) {
    if let Some(mut t) = transition {
        t.timer.tick(time.delta());
    }
}

/// Attempt to ease via Bevy's `EasingCurve<f32>` with `sample()`,
/// then apply to bloom + light.
fn fade_bloom_and_light(
    transition: Option<Res<ModeTransition>>,
    mut bloom_q: Query<&mut Bloom, With<MainCamera>>,
    mut light_q: Query<&mut DirectionalLight, With<SceneSunlight>>,
) {
    let Some(transition) = transition else { return };
    let t = ease_cubic_in_out(transition.progress());

    let factor = match transition.direction {
        TransitionDirection::EnterPlayer => t,
        TransitionDirection::ExitPlayer => 1.0 - t,
    };

    if let Ok(mut bloom) = bloom_q.single_mut() {
        bloom.intensity = BLOOM_INTENSITY * factor;
    }
    if let Ok(mut light) = light_q.single_mut() {
        light.illuminance = SUNLIGHT_ILLUMINANCE * factor;
    }
}

fn ease_cubic_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn setup_exit_camera_animation(
    transition: Option<Res<ModeTransition>>,
    home: Option<Res<CameraHome>>,
    camera_transform: Single<&Transform, With<MainCamera>>,
    camera_entity: Single<Entity, With<MainCamera>>,
    mut clips: ResMut<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
    mut setup_done: Local<bool>,
) {
    let Some(ref transition) = transition else {
        *setup_done = false;
        return;
    };
    if transition.direction != TransitionDirection::ExitPlayer {
        *setup_done = false;
        return;
    }
    if *setup_done {
        return;
    }
    *setup_done = true;

    let Some(ref home) = home else { return };

    let target_id = AnimationTargetId::from_name(&Name::new("main_camera"));

    let mut clip = AnimationClip::default();

    // Translation curve: current -> home, eased
    let translation_curve = EasingCurve::new(
        camera_transform.translation,
        home.0.translation,
        EaseFunction::CubicInOut,
    );
    clip.add_curve_to_target(
        target_id,
        AnimatableCurve::new(animated_field!(Transform::translation), translation_curve),
    );

    // Rotation curve: current -> home, eased
    let rotation_curve = EasingCurve::new(
        camera_transform.rotation,
        home.0.rotation,
        EaseFunction::CubicInOut,
    );
    clip.add_curve_to_target(
        target_id,
        AnimatableCurve::new(animated_field!(Transform::rotation), rotation_curve),
    );

    let clip_handle = clips.add(clip);
    let (graph, node_index) = AnimationGraph::from_clip(clip_handle);
    let graph_handle = graphs.add(graph);

    // Add animation components to camera
    commands.entity(*camera_entity).insert((
        Name::new("main_camera"),
        target_id,
        AnimatedBy(*camera_entity),
        AnimationGraphHandle(graph_handle),
        AnimationPlayer::default(),
    ));

    commands.insert_resource(PendingAnimationStart(node_index));
}

fn start_pending_animation(
    pending: Option<Res<PendingAnimationStart>>,
    mut player_q: Query<&mut AnimationPlayer, With<MainCamera>>,
    mut commands: Commands,
) {
    let Some(pending) = pending else { return };
    let Ok(mut player) = player_q.single_mut() else {
        return;
    };

    player.start(pending.0).set_speed(1.0 / TRANSITION_DURATION);

    commands.remove_resource::<PendingAnimationStart>();
}

fn complete_mode_transition(
    transition: Option<Res<ModeTransition>>,
    mut state: Single<&mut FreeCameraState, With<MainCamera>>,
    camera: Single<Entity, With<MainCamera>>,
    sunlight_q: Query<Entity, With<SceneSunlight>>,
    mut mode: ResMut<InteractionMode>,
    home: Option<Res<CameraHome>>,
    mut transform: Single<&mut Transform, With<MainCamera>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut window_transform_q: Query<
        &mut Transform,
        (With<crate::window::VmuxWindow>, Without<MainCamera>),
    >,
    mut commands: Commands,
) {
    let Some(ref transition) = transition else {
        return;
    };
    if !transition.timer.just_finished() {
        return;
    }

    match transition.direction {
        TransitionDirection::EnterPlayer => {
            // Fade-in done, enable free camera movement
            state.enabled = true;
        }
        TransitionDirection::ExitPlayer => {
            // Animation done, clean up
            *mode = InteractionMode::User;
            reset_free_camera_state(&mut state);

            commands.entity(*camera).remove::<Bloom>();

            for e in &sunlight_q {
                commands.entity(e).despawn();
            }

            // Snap to exact home transform
            if let Some(ref home) = home {
                **transform = home.0;
            }

            let window_transform = frame_window_transform(&window);
            for mut transform in &mut window_transform_q {
                *transform = window_transform;
            }

            // Remove animation components
            commands
                .entity(*camera)
                .remove::<AnimationPlayer>()
                .remove::<AnimationGraphHandle>()
                .remove::<AnimationTargetId>()
                .remove::<AnimatedBy>()
                .remove::<Name>();

            commands.remove_resource::<CameraHome>();
            commands.remove_resource::<PendingAnimationStart>();
        }
    }

    commands.remove_resource::<ModeTransition>();
}

// ---------------------------------------------------------------------------
// Suppress system
// ---------------------------------------------------------------------------

fn suppress_free_camera_when_pane_active(
    mode: Res<InteractionMode>,
    transition: Option<Res<ModeTransition>>,
    kb_targets: Query<(), With<CefKeyboardTarget>>,
    mut state: Single<&mut FreeCameraState, With<MainCamera>>,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
) {
    // Only applies in Player mode with no active transition
    if *mode != InteractionMode::Player || transition.is_some() {
        return;
    }

    let no_target = kb_targets.is_empty();
    state.enabled = no_target;
    suppress.0 = no_target;
}

// ---------------------------------------------------------------------------
// Camera framing
// ---------------------------------------------------------------------------

pub fn frame_main_camera_transform(window: &Window, aspect: f32, margin_px: f32) -> Transform {
    let m = window.meters();
    let margin = margin_px / PIXELS_PER_METER;
    let center = Vec3::new(0.0, m.y * 0.5, 0.0);

    let half_fov_y = FOV_Y * 0.5;
    let tan_half_fov_y = half_fov_y.tan();

    let dist_to_fit_height = ((m.y * 0.5) + margin) / tan_half_fov_y;
    let tan_half_fov_x = tan_half_fov_y * aspect;
    let dist_to_fit_width = ((m.x * 0.5) + margin) / tan_half_fov_x;
    let dist = dist_to_fit_height.max(dist_to_fit_width);

    Transform::from_xyz(center.x, center.y, center.z + dist).looking_at(center, Vec3::Y)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_margin_is_zero() {
        assert_eq!(camera_margin_px(), 0.0);
    }

    #[test]
    fn main_camera_uses_non_lut_tonemapping() {
        let mut app = App::new();
        app.register_required_components::<
            Camera3d,
            bevy::core_pipeline::tonemapping::Tonemapping,
        >()
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
}
