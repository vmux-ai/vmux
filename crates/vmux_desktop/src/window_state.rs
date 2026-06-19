use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowPosition};
use vmux_layout::window::WindowGeometry;

#[cfg(not(target_os = "macos"))]
use bevy::window::{MonitorSelection, WindowMode};

/// Captures below this logical size are treated as transient and ignored.
const MIN_WINDOW_SIZE: f32 = 100.0;

/// Live fullscreen signal. macOS writes it from `glass.rs` (NSWindow styleMask);
/// other platforms derive it from `window.mode`.
#[derive(Resource, Default, Debug)]
pub struct WindowFullscreen(pub bool);

/// Loaded fullscreen intent awaiting application. Inserted by
/// `apply_geometry_on_load`, consumed by the platform fullscreen-restore system
/// (post-reveal on macOS), which then sets [`WindowRestoreComplete`].
#[derive(Resource, Debug)]
pub struct PendingFullscreenRestore(pub bool);

/// Set once startup geometry restore is finished. Capture is gated on this so
/// the transient windowed startup state can't overwrite a saved `fullscreen`.
#[derive(Resource, Default, Debug)]
pub struct WindowRestoreComplete;

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
        #[cfg(not(target_os = "macos"))]
        app.add_systems(
            Update,
            (
                sync_fullscreen_signal_from_mode,
                restore_fullscreen_non_macos,
            ),
        );
    }
}

/// Spawn the persisted geometry singleton if none was loaded from `store.ron`
/// (first run, or an older store predating this feature). Gated on restore
/// completion so it never races the scene load into a duplicate.
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

/// Apply a freshly loaded (or spawned) `WindowGeometry` to the primary window
/// once. Windowed frame is applied immediately (window is hidden until reveal on
/// macOS, so flicker-free); fullscreen intent is deferred via
/// [`PendingFullscreenRestore`].
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

/// Sync the live window frame into the persisted `WindowGeometry`, marking the
/// scene dirty (via `Changed<WindowGeometry>`) for the debounced auto-save.
/// Position/size track the windowed frame only; while fullscreen they are left
/// untouched so exiting fullscreen lands on the prior frame.
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

#[cfg(not(target_os = "macos"))]
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

#[cfg(not(target_os = "macos"))]
fn restore_fullscreen_non_macos(
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
}
