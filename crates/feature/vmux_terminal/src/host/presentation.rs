use bevy::prelude::*;
use vmux_ecs::{EntityTarget, LastActivatedAt, ProcessPresentationRequest};
use vmux_layout::pane::PanePlacement;
use vmux_layout::stack::Stack;
use vmux_session::Session;

use crate::{ReattachedTerminalBundle, RetainOnProcessExit, TerminalPlugin};

pub(super) fn present(
    trigger: On<ProcessPresentationRequest>,
    stacks: Query<(Entity, &EntityTarget<Session>)>,
    mut placement: PanePlacement,
    mut commands: Commands,
) {
    let request = trigger.event();
    let Some(stack) = stacks
        .iter()
        .find(|(_, target)| target.entity() == request.owner)
        .map(|(stack, _)| stack)
    else {
        return;
    };
    let Ok(agent_pane) = placement
        .child_of_q
        .get(stack)
        .map(|child_of| child_of.parent())
    else {
        return;
    };
    let target_pane = placement.resolve_spiral(
        agent_pane,
        TerminalPlugin::URL,
        false,
        &mut std::collections::HashSet::new(),
    );
    let tab = commands
        .spawn((Stack::bundle(), LastActivatedAt(0), ChildOf(target_pane)))
        .id();
    commands.spawn((
        ReattachedTerminalBundle::new(request.process_id),
        RetainOnProcessExit,
        ChildOf(tab),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_ecs::ProcessId;
    use vmux_layout::pane::Pane;
    use vmux_layout::tab::Tab;

    #[test]
    fn process_presentation_does_not_take_focus_from_owner() {
        let mut app = App::new();
        app.add_observer(present);
        let tab = app.world_mut().spawn(Tab::bundle()).id();
        let pane = app.world_mut().spawn((Pane::bundle(), ChildOf(tab))).id();
        let session = app.world_mut().spawn(Session).id();
        let owner = app
            .world_mut()
            .spawn((
                Stack::bundle(),
                LastActivatedAt(10),
                ChildOf(pane),
                EntityTarget::<Session>::new(session),
            ))
            .id();

        app.world_mut().trigger(ProcessPresentationRequest {
            owner: session,
            process_id: ProcessId::new(),
        });
        app.world_mut().flush();

        let stack_times = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<(Entity, &LastActivatedAt), With<Stack>>();
            query
                .iter(world)
                .map(|(entity, activated)| (entity, activated.0))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            stack_times
                .iter()
                .find(|(entity, _)| *entity == owner)
                .map(|(_, activated)| *activated),
            Some(10)
        );
        assert_eq!(
            stack_times
                .iter()
                .find(|(entity, _)| *entity != owner)
                .map(|(_, activated)| *activated),
            Some(0)
        );
    }
}
