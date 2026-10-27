use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::SwitchTabRequest;
use vmux_command::CommandBarDismiss;
use vmux_command::ReadCommandRequests;
use vmux_ecs::launcher::{HostsLauncher, StackInPaneChosen};

use crate::{
    pane::{Pane, PaneSplit},
    stack::{CloseStackRequest, Stack},
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum LayoutRequestSet {
    Dispatch,
    Prepare,
    Handle,
}

pub(super) struct LayoutRequestPlugin;

impl Plugin for LayoutRequestPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                LayoutRequestSet::Dispatch,
                LayoutRequestSet::Prepare,
                LayoutRequestSet::Handle,
            )
                .chain()
                .in_set(ReadCommandRequests),
        )
        .add_plugins(UiEventPlugin::<(SwitchTabRequest,)>::default())
        .add_observer(switch_tab);
    }
}

fn switch_tab(
    trigger: On<UiInput<SwitchTabRequest>>,
    child_of: Query<&ChildOf>,
    launcher_hosts: Query<(), With<HostsLauncher>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stacks: Query<(), With<Stack>>,
    mut chosen: MessageWriter<StackInPaneChosen>,
    mut close: MessageWriter<CloseStackRequest>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let request = &trigger.event().payload;
    let target = leaf_panes
        .iter()
        .find(|pane| pane.to_bits() == request.pane)
        .and_then(|pane| pane_children.get(pane).ok())
        .and_then(|children| {
            children
                .iter()
                .filter(|stack| stacks.contains(*stack))
                .nth(request.index)
        });
    let source = if launcher_hosts.contains(webview) {
        child_of
            .get(webview)
            .ok()
            .map(Relationship::get)
            .filter(|stack| stacks.contains(*stack))
    } else {
        None
    };
    chosen.write(StackInPaneChosen {
        pane_bits: request.pane,
        index: request.index,
    });
    if let (Some(source), Some(target)) = (source, target)
        && source != target
    {
        close.write(CloseStackRequest::tidying(source));
    }
    commands.trigger(CommandBarDismiss::new(webview, true));
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::Messages;

    use super::*;

    #[test]
    fn selecting_existing_stack_tidies_launcher_stack() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutRequestPlugin))
            .add_message::<StackInPaneChosen>()
            .add_message::<CloseStackRequest>();
        let pane = app.world_mut().spawn(Pane::bundle()).id();
        let launcher = app.world_mut().spawn((Stack::bundle(), ChildOf(pane))).id();
        app.world_mut().spawn((Stack::bundle(), ChildOf(pane)));
        let webview = app
            .world_mut()
            .spawn((HostsLauncher, ChildOf(launcher)))
            .id();

        app.world_mut().trigger(UiInput::<SwitchTabRequest> {
            webview,
            payload: SwitchTabRequest {
                pane: pane.to_bits(),
                index: 1,
            },
        });

        let requests = app
            .world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].stack, launcher);
        assert_eq!(requests[0].reason, crate::stack::CloseStackReason::Tidying);
    }
}
