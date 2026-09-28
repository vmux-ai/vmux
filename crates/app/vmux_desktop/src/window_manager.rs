use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::HostWindow;
use vmux_layout::window::{FocusedWindow, NewWindowWorkspace, VmuxWindow};

pub(crate) struct WindowManagerPlugin;

impl Plugin for WindowManagerPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_command::CommandRuntimePlugin>() {
            app.add_plugins(vmux_command::CommandRuntimePlugin);
        }
        app.add_message::<NewWindowRequest>()
            .add_message::<CloseFocusedWindowRequest>()
            .add_message::<CloseVmuxWindow>()
            .add_systems(
                Startup,
                spawn_window_commands.in_set(vmux_command::RegisterCommandDefinitions),
            )
            .add_observer(request_new_window)
            .add_observer(request_close_focused_window)
            .add_systems(
                Update,
                (open_windows, close_focused_windows)
                    .chain()
                    .in_set(vmux_command::ReadCommandRequests)
                    .after(vmux_layout::window::WindowFocusSet),
            )
            .add_systems(Update, close_windows.after(close_focused_windows));
    }
}

#[derive(Message, Clone, Copy)]
pub(crate) struct CloseVmuxWindow(pub Entity);

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct NewWindowRequest;

#[derive(Component)]
struct NewWindowBinding;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct CloseFocusedWindowRequest;

#[derive(Component)]
struct CloseFocusedWindowBinding;

fn spawn_window_commands(mut commands: Commands) {
    let mut definitions =
        vmux_command::CommandDefinitions::from_ron(include_str!("window_manager.ron"));
    commands.spawn((definitions.take("new_window"), NewWindowBinding));
    commands.spawn((definitions.take("close_window"), CloseFocusedWindowBinding));
    definitions.assert_all_registered();
}

fn request_new_window(
    trigger: On<vmux_command::CommandDispatch>,
    bindings: Query<(), With<NewWindowBinding>>,
    mut requests: MessageWriter<NewWindowRequest>,
) {
    if bindings.contains(trigger.event().command()) {
        requests.write(NewWindowRequest);
    }
}

fn request_close_focused_window(
    trigger: On<vmux_command::CommandDispatch>,
    bindings: Query<(), With<CloseFocusedWindowBinding>>,
    mut requests: MessageWriter<CloseFocusedWindowRequest>,
) {
    if bindings.contains(trigger.event().command()) {
        requests.write(CloseFocusedWindowRequest);
    }
}

fn open_windows(
    mut reader: MessageReader<NewWindowRequest>,
    focused: FocusedWindow,
    mut commands: Commands,
) {
    for _ in reader.read() {
        if let Some(window) = focused.entity() {
            commands.entity(window).remove::<vmux_core::Active>();
        }
        commands.spawn((
            crate::window_config(true),
            NewWindowWorkspace,
            vmux_core::Active,
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;

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
                .contains::<vmux_core::Active>()
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
}
