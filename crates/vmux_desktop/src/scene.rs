use crate::{
    command::{AppCommand, SceneCommand, ReadAppCommands},
    layout::fit_window_to_screen,
    settings::{AppSettings, load_settings},
    unit::{PIXELS_PER_METER, WindowExt},
};
use bevy::{
    animation::{animated_field, prelude::*, AnimationTargetId},
    camera::PerspectiveProjection,
    camera::Projection,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin, FreeCameraState},
    math::curve::easing::EasingCurve,
    post_process::bloom::Bloom,
    prelude::*,
    window::PrimaryWindow,
};
use bevy_cef::prelude::CefKeyboardTarget;

pub(crate) const FOV_Y: f32 = std::f32::consts::FRAC_PI_4;

const TRANSITION_DURATION: f32 = 0.3;
const BLOOM_INTENSITY: f32 = 0.15; // Bloom::NATURAL intensity
const SUNLIGHT_ILLUMINANCE: f32 = 8000.0;

fn camera_margin_px(_settings: &AppSettings) -> f32 {
    0.0
}

#[derive(Component)]
pub(crate) struct MainCamera;

#[derive(Resource, Default, PartialEq, Eq, Clone, Copy)]
pub(crate) enum InteractionMode {
    #[default]
    User,
    Player,
}

#[derive(Resource)]
pub(crate) struct CameraHome(pub Transform);

#[derive(Resource)]
pub(crate) struct ModeTransition {
    pub direction: TransitionDirection,
    pub timer: Timer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransitionDirection {
    EnterPlayer,
    ExitPlayer,
}

impl ModeTransition {
    pub(crate) fn new(direction: TransitionDirection) -> Self {
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
            .add_systems(Startup, setup.after(load_settings))
            .add_systems(
                Startup,
                fit_main_camera
                    .after(load_settings)
                    .after(fit_window_to_screen),
            )
            .add_systems(
                Update,
                (
                    on_toggle_player_mode.in_set(ReadAppCommands),
                    suppress_free_camera_when_pane_active,
                    tick_mode_transition,
                    fade_bloom_and_light,
                    setup_exit_camera_animation,
                    start_pending_animation,
                    complete_mode_transition,
                ),
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

pub(crate) fn setup(
    mut commands: Commands,
    window: Single<&Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
) {
    let mut state = FreeCameraState::default();
    state.enabled = false;

    commands.spawn((
        MainCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: FOV_Y,
            ..default()
        }),
        frame_main_camera_transform(&window, window.aspect(), camera_margin_px(&settings)),
        FreeCamera {
            sensitivity: 1.0,
            friction: 25.0,
            walk_speed: 0.5,
            run_speed: 5.0,
            ..default()
        },
        state,
    ));
}

fn fit_main_camera(
    window: Single<&Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut camera_q: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
    camera_state: Single<&FreeCameraState, With<MainCamera>>,
    mode: Res<InteractionMode>,
    transition: Option<Res<ModeTransition>>,
) {
    let Ok((mut tf, mut proj)) = camera_q.single_mut() else {
        return;
    };
    let aspect = window.aspect();

    if let Projection::Perspective(ref mut p) = *proj {
        if (p.aspect_ratio - aspect).abs() > f32::EPSILON {
            p.aspect_ratio = aspect;
        }
    }

    // Skip transform update during transitions or when camera is user-controlled
    if transition.is_some() || camera_state.enabled {
        return;
    }

    // Only reset transform in User mode
    if *mode == InteractionMode::User {
        *tf = frame_main_camera_transform(&window, aspect, camera_margin_px(&settings));
    }
}

fn update_camera_home(
    window: Single<&Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mode: Res<InteractionMode>,
    home: Option<ResMut<CameraHome>>,
) {
    if *mode != InteractionMode::Player {
        return;
    }
    let Some(mut home) = home else { return };
    home.0 = frame_main_camera_transform(&window, window.aspect(), camera_margin_px(&settings));
}

// ---------------------------------------------------------------------------
// Toggle command handler
// ---------------------------------------------------------------------------

fn on_toggle_player_mode(
    mut reader: MessageReader<AppCommand>,
    window: Single<&Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut state: Single<&mut FreeCameraState, With<MainCamera>>,
    camera: Single<Entity, With<MainCamera>>,
    kb_targets: Query<Entity, With<CefKeyboardTarget>>,
    mut mode: ResMut<InteractionMode>,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
    transition: Option<Res<ModeTransition>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Scene(SceneCommand::TogglePlayerMode) = *cmd else {
            continue;
        };

        // Ignore command during active transition
        if transition.is_some() {
            continue;
        }

        match *mode {
            InteractionMode::User => {
                // Store home transform
                let home = frame_main_camera_transform(
                    &window,
                    window.aspect(),
                    camera_margin_px(&settings),
                );
                commands.insert_resource(CameraHome(home));

                *mode = InteractionMode::Player;
                suppress.0 = true;

                // Remove keyboard targets so free camera keys work
                for e in &kb_targets {
                    commands.entity(e).remove::<CefKeyboardTarget>();
                }

                // Spawn bloom with 0 intensity (fade system will animate it)
                let mut bloom = Bloom::NATURAL;
                bloom.intensity = 0.0;
                commands.entity(*camera).insert(bloom);

                // Spawn sunlight with 0 illuminance
                commands.spawn((
                    SceneSunlight,
                    DirectionalLight {
                        illuminance: 0.0,
                        shadows_enabled: false,
                        color: Color::srgb(1.0, 0.98, 0.95),
                        ..default()
                    },
                    Transform::from_rotation(Quat::from_euler(
                        EulerRot::XYZ,
                        -0.6,
                        0.4,
                        0.0,
                    )),
                ));

                // Start transition timer
                commands.insert_resource(ModeTransition::new(TransitionDirection::EnterPlayer));

                // FreeCameraState.enabled is set by complete_mode_transition
                // after the fade-in finishes.
            }
            InteractionMode::Player => {
                start_exit_transition(&mut state, &mut suppress, &mut commands, *camera);
            }
        }
    }
}

fn start_exit_transition(
    state: &mut FreeCameraState,
    suppress: &mut ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
    commands: &mut Commands,
    _camera_entity: Entity,
) {
    // Disable free camera immediately so WASD stops during transition
    state.enabled = false;
    suppress.0 = false;

    // Start the exit transition timer
    // Camera animation is set up by setup_exit_camera_animation
    commands.insert_resource(ModeTransition::new(TransitionDirection::ExitPlayer));
}

// ---------------------------------------------------------------------------
// Transition systems
// ---------------------------------------------------------------------------

fn tick_mode_transition(
    time: Res<Time>,
    transition: Option<ResMut<ModeTransition>>,
) {
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
        AnimatableCurve::new(
            animated_field!(Transform::translation),
            translation_curve,
        ),
    );

    // Rotation curve: current -> home, eased
    let rotation_curve = EasingCurve::new(
        camera_transform.rotation,
        home.0.rotation,
        EaseFunction::CubicInOut,
    );
    clip.add_curve_to_target(
        target_id,
        AnimatableCurve::new(
            animated_field!(Transform::rotation),
            rotation_curve,
        ),
    );

    let clip_handle = clips.add(clip);
    let (graph, node_index) = AnimationGraph::from_clip(clip_handle);
    let graph_handle = graphs.add(graph);

    // Add animation components to camera
    commands.entity(*camera_entity).insert((
        Name::new("main_camera"),
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
    let Ok(mut player) = player_q.single_mut() else { return };

    player
        .start(pending.0)
        .set_speed(1.0 / TRANSITION_DURATION);

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
    mut commands: Commands,
) {
    let Some(ref transition) = transition else { return };
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

            commands.entity(*camera).remove::<Bloom>();

            for e in &sunlight_q {
                commands.entity(e).despawn();
            }

            // Snap to exact home transform
            if let Some(ref home) = home {
                **transform = home.0;
            }

            // Remove animation components
            commands
                .entity(*camera)
                .remove::<AnimationPlayer>()
                .remove::<AnimationGraphHandle>()
                .remove::<Name>();

            commands.remove_resource::<CameraHome>();
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

pub(crate) fn frame_main_camera_transform(
    window: &Window,
    aspect: f32,
    margin_px: f32,
) -> Transform {
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
