use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowPosition};
use vmux_layout::window::WindowGeometry;

#[cfg(not(all(target_os = "macos", feature = "native-glass")))]
use bevy::window::{MonitorSelection, WindowMode};

pub(crate) struct WindowStatePlugin;

impl Plugin for WindowStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WindowFullscreen>().add_systems(
            Update,
            (
                ensure_geometry_singleton,
                apply_geometry_on_load,
                capture_window_geometry.run_if(resource_exists::<WindowRestoreComplete>),
            )
                .chain(),
        );
        #[cfg(not(all(target_os = "macos", feature = "native-glass")))]
        app.add_systems(
            Update,
            (
                sync_fullscreen_signal_from_mode,
                restore_fullscreen_from_window_mode,
            ),
        );
    }
}

const MIN_WINDOW_SIZE: f32 = 100.0;

#[derive(Resource, Default, Debug)]
pub struct WindowFullscreen(pub bool);

#[derive(Resource, Debug)]
pub struct PendingFullscreenRestore(pub bool);

#[derive(Resource, Default, Debug)]
pub struct WindowRestoreComplete;

fn ensure_geometry_singleton(
    restore: Res<crate::boot_status::RestoreComplete>,
    existing: Query<(), With<WindowGeometry>>,
    mut commands: Commands,
) {
    if !restore.0 || !existing.is_empty() {
        return;
    }
    commands.spawn(WindowGeometry::default());
}

fn apply_geometry_on_load(
    geometry: Query<&WindowGeometry, Added<WindowGeometry>>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    pending: Option<Res<PendingFullscreenRestore>>,
    restore_done: Option<Res<WindowRestoreComplete>>,
    mut commands: Commands,
) {
    let Some(geom) = geometry.iter().next().copied() else {
        return;
    };
    if let Ok(mut window) = window.single_mut() {
        if let Some(pos) = geom.position {
            window.position = WindowPosition::At(pos);
        }
        if let Some(size) = geom.size {
            window.resolution.set(size.x, size.y);
        }
    }
    if pending.is_none() && restore_done.is_none() {
        commands.insert_resource(PendingFullscreenRestore(geom.fullscreen));
    }
}

fn capture_window_geometry(
    fullscreen: Res<WindowFullscreen>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut geometry: Query<&mut WindowGeometry>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let Ok(mut geom) = geometry.single_mut() else {
        return;
    };

    let mut next = *geom;
    next.fullscreen = fullscreen.0;
    if !fullscreen.0 {
        if let WindowPosition::At(p) = window.position {
            next.position = Some(p);
        }
        let size = window.resolution.size();
        if size.x >= MIN_WINDOW_SIZE && size.y >= MIN_WINDOW_SIZE {
            next.size = Some(size);
        }
    }
    if next != *geom {
        *geom = next;
    }
}

#[cfg(not(all(target_os = "macos", feature = "native-glass")))]
fn sync_fullscreen_signal_from_mode(
    window: Query<&Window, With<PrimaryWindow>>,
    mut fullscreen: ResMut<WindowFullscreen>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let is_fullscreen = matches!(
        window.mode,
        WindowMode::BorderlessFullscreen(_) | WindowMode::Fullscreen(..)
    );
    if fullscreen.0 != is_fullscreen {
        fullscreen.0 = is_fullscreen;
    }
}

#[cfg(not(all(target_os = "macos", feature = "native-glass")))]
fn restore_fullscreen_from_window_mode(
    pending: Option<Res<PendingFullscreenRestore>>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let Some(pending) = pending else {
        return;
    };
    if pending.0
        && let Ok(mut window) = window.single_mut()
    {
        window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Primary);
    }
    commands.remove_resource::<PendingFullscreenRestore>();
    commands.insert_resource(WindowRestoreComplete);
}

#[cfg(test)]
mod tests {
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
}
