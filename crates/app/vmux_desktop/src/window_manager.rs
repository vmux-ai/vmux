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
        app.add_plugins((
            vmux_command::CommandTypePlugin::<NewWindowRequest>::default(),
            vmux_command::CommandTypePlugin::<CloseFocusedWindowRequest>::default(),
        ))
        .add_message::<CloseVmuxWindow>()
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

impl vmux_command::CommandRequest for NewWindowRequest {
    fn definitions() -> Vec<vmux_command::CommandDefinition> {
        vec![
            vmux_command::CommandDefinition::new("new_window", "New Window", "Layout > Window")
                .accelerator("super+n")
                .hidden()
                .direct("Super+N"),
        ]
    }
}

impl TryFrom<&vmux_command::CommandInvocation> for NewWindowRequest {
    type Error = ();

    fn try_from(invocation: &vmux_command::CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "new_window").then_some(Self).ok_or(())
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct CloseFocusedWindowRequest;

impl vmux_command::CommandRequest for CloseFocusedWindowRequest {
    fn definitions() -> Vec<vmux_command::CommandDefinition> {
        vec![
            vmux_command::CommandDefinition::new("close_window", "Close Window", "Layout > Window")
                .accelerator("super+shift+w")
                .hidden(),
        ]
    }
}

impl TryFrom<&vmux_command::CommandInvocation> for CloseFocusedWindowRequest {
    type Error = ();

    fn try_from(invocation: &vmux_command::CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "close_window").then_some(Self).ok_or(())
    }
}

fn open_windows(
    mut reader: MessageReader<NewWindowRequest>,
    mut focused: ResMut<FocusedWindow>,
    mut commands: Commands,
) {
    for _ in reader.read() {
        let window = commands
            .spawn((crate::window_config(true), NewWindowWorkspace))
            .id();
        focused.0 = Some(window);
    }
}

fn close_focused_windows(
    mut reader: MessageReader<CloseFocusedWindowRequest>,
    focused: Res<FocusedWindow>,
    mut close: MessageWriter<CloseVmuxWindow>,
) {
    for _ in reader.read() {
        if let Some(window) = focused.0 {
            close.write(CloseVmuxWindow(window));
        }
    }
}

fn close_windows(
    mut requests: MessageReader<CloseVmuxWindow>,
    windows: Query<(Entity, Has<PrimaryWindow>), With<Window>>,
    roots: Query<(Entity, &HostWindow), With<VmuxWindow>>,
    mut lifecycle: MessageWriter<crate::runtime::LifecycleEvent>,
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
            lifecycle.write(crate::runtime::LifecycleEvent::HideAllWindows);
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
            .init_resource::<FocusedWindow>()
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
        assert_eq!(app.world().resource::<FocusedWindow>().0, Some(windows[0]));
    }

    #[test]
    fn closing_one_of_two_windows_despawns_only_its_shell() {
        let mut app = App::new();
        app.add_message::<crate::runtime::LifecycleEvent>()
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
        app.add_message::<crate::runtime::LifecycleEvent>()
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
        app.add_message::<crate::runtime::LifecycleEvent>()
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
