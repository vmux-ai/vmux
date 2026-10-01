use bevy::prelude::*;
use bevy::window::{Monitor, MonitorSelection, PrimaryWindow, WindowPosition};
use bevy_cef::prelude::HostWindow;
use vmux_ecs::WindowFullscreen;
#[cfg(not(all(target_os = "macos", feature = "native-glass")))]
use vmux_ecs::WindowFullscreenSet;
use vmux_ecs::host::persistence::WorkspaceRestore;
use vmux_layout::window::{
    CloseFocusedWindowRequest, FocusedWindow, NewWindowRequest, NewWindowWorkspace, VmuxWindow,
    WindowGeometry,
};

#[cfg(not(all(target_os = "macos", feature = "native-glass")))]
use bevy::window::WindowMode;

pub(crate) struct WindowPlugin;

impl Plugin for WindowPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CloseVmuxWindow>()
            .add_message::<ExitFullscreenRequest>()
            .add_systems(PreUpdate, ensure_state)
            .add_systems(
                Update,
                (open_windows, close_focused_windows)
                    .chain()
                    .in_set(vmux_command::ReadCommandRequests)
                    .after(vmux_layout::window::WindowFocusSet),
            )
            .add_systems(Update, close_windows.after(close_focused_windows))
            .add_systems(
                Update,
                (
                    ensure_geometry_singleton,
                    apply_geometry_on_load,
                    capture_geometry,
                )
                    .chain(),
            )
            .add_systems(Update, relocate_to_live_display);
        #[cfg(not(all(target_os = "macos", feature = "native-glass")))]
        app.add_systems(
            Update,
            (sync_fullscreen, restore_fullscreen).in_set(WindowFullscreenSet),
        );
    }
}

#[derive(Message, Clone, Copy)]
pub(crate) struct CloseVmuxWindow(pub Entity);

#[derive(Message, Clone, Copy)]
pub(crate) struct ExitFullscreenRequest;

#[derive(Component, Debug)]
pub(crate) struct PendingFullscreenRestore(pub bool);

#[derive(Component, Default, Debug)]
pub(crate) struct WindowRestoreComplete;

const MIN_WINDOW_SIZE: f32 = 100.0;

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

fn relocate_to_live_display(
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
    let monitor_rects: Vec<IRect> = monitors.iter().map(monitor_rect).collect();
    for mut window in &mut windows {
        let WindowPosition::At(position) = window.position else {
            continue;
        };
        let size = window.resolution.physical_size().as_ivec2();
        let window_rect = IRect::from_corners(position, position + size);
        if window_off_all_monitors(window_rect, &monitor_rects) {
            window.position = WindowPosition::Centered(MonitorSelection::Primary);
        }
    }
}

fn open_windows(
    mut reader: MessageReader<NewWindowRequest>,
    focused: FocusedWindow,
    mut commands: Commands,
) {
    for _ in reader.read() {
        if let Some(window) = focused.entity() {
            commands.entity(window).remove::<vmux_ecs::Active>();
        }
        commands.spawn((
            crate::window_config(true),
            NewWindowWorkspace,
            vmux_ecs::Active,
        ));
    }
}

fn close_focused_windows(
    mut reader: MessageReader<CloseFocusedWindowRequest>,
    focused: FocusedWindow,
    mut close: MessageWriter<CloseVmuxWindow>,
) {
    for _ in reader.read() {
        if let Some(window) = focused.entity() {
            close.write(CloseVmuxWindow(window));
        }
    }
}

fn close_windows(
    mut requests: MessageReader<CloseVmuxWindow>,
    windows: Query<(Entity, Has<PrimaryWindow>), With<Window>>,
    roots: Query<(Entity, &HostWindow), With<VmuxWindow>>,
    mut hide_windows: MessageWriter<crate::runtime::HideAllWindowsRequest>,
    mut commands: Commands,
) {
    let mut remaining: Vec<(Entity, bool)> = windows.iter().collect();
    for request in requests.read() {
        let Some(index) = remaining
            .iter()
            .position(|(window, _)| *window == request.0)
        else {
            continue;
        };
        if remaining.len() <= 1 {
            hide_windows.write(crate::runtime::HideAllWindowsRequest);
            continue;
        }
        let (_, primary) = remaining.remove(index);
        if primary && let Some((next, _)) = remaining.first() {
            commands.entity(*next).insert(PrimaryWindow);
        }
        for (root, host) in &roots {
            if host.0 == request.0 {
                commands.entity(root).despawn();
            }
        }
        if windows.get(request.0).is_ok() {
            commands.entity(request.0).despawn();
        }
    }
}

fn ensure_state(
    windows: Query<Entity, (With<Window>, Without<WindowFullscreen>)>,
    mut commands: Commands,
) {
    for entity in &windows {
        commands.entity(entity).insert(WindowFullscreen::default());
    }
}

fn ensure_geometry_singleton(
    restore: Single<&WorkspaceRestore>,
    existing: Query<(), With<WindowGeometry>>,
    mut commands: Commands,
) {
    if !restore.complete || !existing.is_empty() {
        return;
    }
    commands.spawn(WindowGeometry::default());
}

fn apply_geometry_on_load(
    geometry: Query<&WindowGeometry, Added<WindowGeometry>>,
    mut window: Query<
        (
            Entity,
            &mut Window,
            Has<PendingFullscreenRestore>,
            Has<WindowRestoreComplete>,
        ),
        With<PrimaryWindow>,
    >,
    mut commands: Commands,
) {
    let Some(geom) = geometry.iter().next().copied() else {
        return;
    };
    if let Ok((entity, mut window, pending, restore_done)) = window.single_mut() {
        if let Some(pos) = geom.position {
            window.position = WindowPosition::At(pos);
        }
        if let Some(size) = geom.size {
            window.resolution.set(size.x, size.y);
        }
        if !pending && !restore_done {
            commands
                .entity(entity)
                .insert(PendingFullscreenRestore(geom.fullscreen));
        }
    }
}

fn capture_geometry(
    window: Query<(&Window, &WindowFullscreen), (With<PrimaryWindow>, With<WindowRestoreComplete>)>,
    mut geometry: Query<&mut WindowGeometry>,
) {
    let Ok((window, fullscreen)) = window.single() else {
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
fn sync_fullscreen(mut windows: Query<(&Window, &mut WindowFullscreen)>) {
    for (window, mut fullscreen) in &mut windows {
        let is_fullscreen = matches!(
            window.mode,
            WindowMode::BorderlessFullscreen(_) | WindowMode::Fullscreen(..)
        );
        if fullscreen.0 != is_fullscreen {
            fullscreen.0 = is_fullscreen;
        }
    }
}

#[cfg(not(all(target_os = "macos", feature = "native-glass")))]
fn restore_fullscreen(
    mut window: Query<(Entity, &mut Window, &PendingFullscreenRestore), With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let Ok((entity, mut window, pending)) = window.single_mut() else {
        return;
    };
    if pending.0 {
        window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Primary);
    }
    commands
        .entity(entity)
        .remove::<PendingFullscreenRestore>()
        .insert(WindowRestoreComplete);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy::window::OnMonitor;

    fn test_monitor(x: i32, y: i32, width: u32, height: u32) -> Monitor {
        Monitor {
            name: Some("test".to_string()),
            physical_height: height,
            physical_width: width,
            physical_position: IVec2::new(x, y),
            refresh_rate_millihertz: Some(60_000),
            scale_factor: 1.0,
            video_modes: Vec::new(),
        }
    }

    fn relocate_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, relocate_to_live_display);
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
    fn despawning_monitor_keeps_linked_window() {
        let mut world = World::new();
        let monitor = world.spawn(test_monitor(0, 0, 1920, 1080)).id();
        let window = world.spawn(Window::default()).id();
        world.entity_mut(window).insert(OnMonitor(monitor));

        world.entity_mut(monitor).despawn();

        assert!(world.get_entity(window).is_ok());
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
            WindowPosition::At(position) if position == IVec2::new(100, 100)
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
            WindowPosition::At(position) if position == IVec2::new(5000, 5000)
        ));
    }

    #[test]
    fn new_window_command_spawns_a_full_window_request() {
        let mut app = App::new();
        app.add_message::<NewWindowRequest>()
            .add_message::<CloseVmuxWindow>()
            .add_systems(Update, open_windows);
        app.world_mut()
            .resource_mut::<Messages<NewWindowRequest>>()
            .write(NewWindowRequest);

        app.update();

        let windows: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, (With<Window>, With<NewWindowWorkspace>)>()
            .iter(app.world())
            .collect();
        assert_eq!(windows.len(), 1);
        assert!(
            app.world()
                .entity(windows[0])
                .contains::<vmux_ecs::Active>()
        );
    }

    #[test]
    fn closing_one_of_two_windows_despawns_only_its_shell() {
        let mut app = App::new();
        app.add_message::<crate::runtime::HideAllWindowsRequest>()
            .add_message::<CloseVmuxWindow>()
            .add_systems(Update, close_windows);
        let first = app.world_mut().spawn(Window::default()).id();
        let second = app.world_mut().spawn(Window::default()).id();
        let first_root = app.world_mut().spawn((VmuxWindow, HostWindow(first))).id();
        let second_root = app.world_mut().spawn((VmuxWindow, HostWindow(second))).id();
        app.world_mut()
            .resource_mut::<Messages<CloseVmuxWindow>>()
            .write(CloseVmuxWindow(second));

        app.update();

        assert!(app.world().get_entity(first).is_ok());
        assert!(app.world().get_entity(first_root).is_ok());
        assert!(app.world().get_entity(second).is_err());
        assert!(app.world().get_entity(second_root).is_err());
    }

    #[test]
    fn closing_every_window_in_one_update_keeps_the_last_shell() {
        let mut app = App::new();
        app.add_message::<crate::runtime::HideAllWindowsRequest>()
            .add_message::<CloseVmuxWindow>()
            .add_systems(Update, close_windows);
        let first = app.world_mut().spawn(Window::default()).id();
        let second = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .resource_mut::<Messages<CloseVmuxWindow>>()
            .write(CloseVmuxWindow(first));
        app.world_mut()
            .resource_mut::<Messages<CloseVmuxWindow>>()
            .write(CloseVmuxWindow(second));

        app.update();

        let windows: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<Window>>()
            .iter(app.world())
            .collect();
        assert_eq!(windows, vec![second]);
    }

    #[test]
    fn closing_the_primary_window_promotes_the_remaining_window() {
        let mut app = App::new();
        app.add_message::<crate::runtime::HideAllWindowsRequest>()
            .add_message::<CloseVmuxWindow>()
            .add_systems(Update, close_windows);
        let primary = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let remaining = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .resource_mut::<Messages<CloseVmuxWindow>>()
            .write(CloseVmuxWindow(primary));

        app.update();

        assert!(app.world().get_entity(primary).is_err());
        assert!(app.world().entity(remaining).contains::<PrimaryWindow>());
    }

    fn geometry_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut().spawn((
            Window {
                resolution: (1200, 800).into(),
                position: WindowPosition::At(IVec2::new(40, 60)),
                ..default()
            },
            PrimaryWindow,
            WindowFullscreen::default(),
        ));
        app
    }

    #[test]
    fn apply_geometry_sets_window_position_and_size() {
        let mut app = geometry_app();
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
        let mut app = geometry_app();
        app.add_systems(Update, apply_geometry_on_load);
        app.world_mut().spawn(WindowGeometry {
            fullscreen: true,
            position: None,
            size: None,
        });
        app.update();

        let pending = app
            .world_mut()
            .query_filtered::<&PendingFullscreenRestore, With<PrimaryWindow>>()
            .single(app.world())
            .unwrap();
        assert!(pending.0);
    }

    #[test]
    fn capture_records_windowed_frame_when_not_fullscreen() {
        let mut app = geometry_app();
        let primary = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .entity_mut(primary)
            .insert(WindowRestoreComplete);
        app.world_mut().spawn(WindowGeometry::default());
        app.add_systems(Update, capture_geometry);
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
        let mut app = geometry_app();
        let primary = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .entity_mut(primary)
            .insert((WindowFullscreen(true), WindowRestoreComplete));
        app.world_mut().spawn(WindowGeometry {
            fullscreen: false,
            position: Some(IVec2::new(7, 8)),
            size: Some(Vec2::new(900.0, 600.0)),
        });
        app.add_systems(Update, capture_geometry);
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
        let mut app = geometry_app();
        let primary = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .entity_mut(primary)
            .insert(PendingFullscreenRestore(false));
        app.add_systems(Update, restore_fullscreen);
        app.update();

        assert!(
            app.world()
                .entity(primary)
                .contains::<WindowRestoreComplete>()
        );
        assert!(
            !app.world()
                .entity(primary)
                .contains::<PendingFullscreenRestore>()
        );
    }
}
