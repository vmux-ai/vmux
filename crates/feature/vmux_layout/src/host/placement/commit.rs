use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_ecs::{EntityTarget, OperationState, Order, PageOpenRequest, PageOpenTarget};
use vmux_history::LastActivatedAt;

use crate::pane::{Pane, PaneSplitDirection, PaneTree, SpawnCounter, SpawnSeq};
use crate::stack::Stack;

use super::query::Scope;
use super::{
    CreatedStack, OpenIntent, Pending, PlacementAdmissionSet, PlacementCommitSet, PlacementQuery,
    PlacementStarting, PreviousNavigation, Routed, StackIntent, StackPlaced, Unplaced,
};

pub(super) struct PlacementCommitPlugin;

impl Plugin for PlacementCommitPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(open)
            .add_observer(place)
            .add_systems(
                Update,
                commit
                    .in_set(PlacementCommitSet)
                    .in_set(crate::host::command::LayoutRequestSet::Handle)
                    .after(PlacementAdmissionSet),
            )
            .add_systems(
                PostUpdate,
                commit
                    .in_set(PlacementCommitSet)
                    .before(crate::presentation::PresentationSettleSet)
                    .before(crate::projection::LayoutUiProjectionSet::Source),
            )
            .add_systems(PostUpdate, retire.after(PlacementCommitSet));
    }
}

#[derive(EntityEvent)]
struct CommitOpen {
    #[event_target]
    operation: Entity,
}

#[derive(EntityEvent)]
struct CommitStack {
    #[event_target]
    operation: Entity,
}

fn commit(world: &mut World, operations: &mut QueryState<(Entity, Option<&Order>), With<Pending>>) {
    let mut pending = Vec::new();
    for (entity, order) in operations.iter(world) {
        pending.push((order.map_or(u32::MAX, |order| order.0), entity));
    }
    pending.sort_by_key(|(order, entity)| (*order, entity.to_bits()));
    for (_, operation) in pending {
        if world.get_entity(operation).is_err() {
            continue;
        }
        world.trigger(PlacementStarting { operation });
        world.flush();
        if world.get_entity(operation).is_err() {
            continue;
        }
        if world.get::<OpenIntent>(operation).is_some() {
            world.trigger(CommitOpen { operation });
        } else if world.get::<StackIntent>(operation).is_some() {
            world.trigger(CommitStack { operation });
        }
        world.flush();
    }
}

#[derive(SystemParam)]
struct Writer<'w, 's> {
    tree: PaneTree<'w, 's>,
    commands: Commands<'w, 's>,
    counter: Single<'w, 's, &'static mut SpawnCounter>,
    sequences: Query<'w, 's, &'static SpawnSeq>,
    requests: MessageWriter<'w, PageOpenRequest>,
    responses: MessageWriter<'w, vmux_ecs::agent::AgentCommandResponse>,
}

impl Writer<'_, '_> {
    fn touch(&mut self, pane: Entity) {
        for sequence in &self.sequences {
            self.counter.0 = self.counter.0.max(sequence.0);
        }
        self.counter.0 += 1;
        self.tree.set_spawn_sequence(pane, SpawnSeq(self.counter.0));
    }

    fn split(
        &mut self,
        anchor: Entity,
        direction: PaneSplitDirection,
        stacks: &[Entity],
        focus: bool,
        split: bool,
    ) -> Entity {
        if split {
            let target = self
                .tree
                .split_or_extend(anchor, direction, stacks, focus, true);
            self.touch(target);
            return target;
        }
        let (holder, target) = self.tree.spawn_split(anchor, direction, stacks, focus);
        self.touch(holder);
        self.touch(target);
        target
    }

    fn focus(&mut self, stack: Entity, pane: Entity, tab: Option<Entity>) {
        self.commands.entity(stack).insert(LastActivatedAt::now());
        self.commands.entity(pane).insert(LastActivatedAt::now());
        if let Some(tab) = tab {
            self.commands.entity(tab).insert(LastActivatedAt::now());
        }
    }

    fn open(&mut self, stack: Entity, intent: &OpenIntent, registered: bool) {
        self.requests.write(PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url: intent.url.clone(),
            request_id: (!registered).then_some(intent.request_id),
        });
    }
}

fn open(
    event: On<CommitOpen>,
    operations: Query<(&EntityTarget<Pane>, &Scope, &OpenIntent, &OperationState)>,
    routed: Query<(Entity, &EntityTarget<Stack>), With<Routed>>,
    layout: PlacementQuery,
    mut writer: Writer,
) {
    let operation = event.operation;
    let Ok((origin, scope, intent, state)) = operations.get(operation) else {
        return;
    };
    if !matches!(state, OperationState::Pending | OperationState::Running)
        || !layout.valid(origin.entity(), *scope)
    {
        if !layout.registered(&intent.url) {
            writer.responses.write(
                vmux_ecs::agent::AgentReply::new(vmux_api::protocol::AgentRequestId(
                    intent.request_id,
                ))
                .response(vmux_api::protocol::AgentCommandResult::Error(
                    "placement source changed before commit".into(),
                )),
            );
        }
        writer.commands.entity(operation).despawn();
        return;
    }
    let anchor = origin.entity();
    let registered = layout.registered(&intent.url);
    let reuse = layout.reuse(anchor, &intent.url, intent.direction.is_none());
    let (stack, pane, requested, created) = if let Some(stack) = reuse {
        let pane = layout.parent(stack).unwrap_or(anchor);
        let requested = layout.stack_url(stack) != Some(intent.url.as_str());
        if requested {
            writer.open(stack, intent, registered);
        }
        (stack, pane, requested, layout.fresh(stack))
    } else {
        let (mut pane, split) = if let Some(direction) = intent.direction {
            match layout.sibling(anchor, direction) {
                Some(sibling) => (sibling, None),
                None => (anchor, Some(PaneSplitDirection::from(direction))),
            }
        } else {
            layout.destination(anchor, &intent.url)
        };
        if let Some(direction) = split {
            pane = writer.split(
                pane,
                direction,
                &layout.stacks(pane),
                intent.focus,
                layout.is_split(pane),
            );
        } else if layout.placement(&intent.url).refresh {
            writer.touch(pane);
        }
        let activity = if intent.focus {
            LastActivatedAt::now()
        } else {
            LastActivatedAt(0)
        };
        let stack = writer
            .commands
            .spawn((Stack::bundle(), activity, ChildOf(pane)))
            .insert(vmux_ecs::PageMetadata {
                url: intent.url.clone(),
                ..Default::default()
            })
            .id();
        writer.open(stack, intent, registered);
        (stack, pane, true, true)
    };
    if intent.focus {
        writer.focus(stack, pane, layout.tab(pane).or_else(|| layout.tab(anchor)));
    }
    if !requested {
        writer.commands.entity(operation).despawn();
        return;
    }
    for (previous, target) in &routed {
        if previous != operation && target.entity() == stack {
            writer.commands.entity(previous).despawn();
        }
    }
    writer
        .commands
        .entity(operation)
        .remove::<Pending>()
        .insert((
            EntityTarget::<Stack>::new(stack),
            PreviousNavigation(layout.navigation(stack)),
            Routed,
        ));
    if created {
        writer.commands.entity(operation).insert(CreatedStack);
    }
}

fn place(
    event: On<CommitStack>,
    operations: Query<(
        &EntityTarget<Pane>,
        &EntityTarget<Stack>,
        &Scope,
        &StackIntent,
        &OperationState,
    )>,
    layout: PlacementQuery,
    mut writer: Writer,
) {
    let operation = event.operation;
    let Ok((origin, target, scope, intent, state)) = operations.get(operation) else {
        return;
    };
    let stack = target.entity();
    if !matches!(state, OperationState::Pending | OperationState::Running)
        || !layout.valid(origin.entity(), *scope)
        || !layout.unadmitted(stack)
    {
        if layout.stack_exists(stack) && layout.parent(stack).is_some() {
            writer.commands.entity(stack).remove::<Unplaced>();
        }
        writer.commands.entity(operation).remove::<Pending>();
        writer.commands.trigger(StackPlaced {
            operation,
            stack,
            result: Err("placement source changed before commit".into()),
        });
        return;
    }
    let anchor = if intent.primary_anchor {
        layout.primary(origin.entity())
    } else {
        origin.entity()
    };
    let (mut pane, split) = if let Some(direction) = intent.direction {
        (anchor, Some(PaneSplitDirection::from(direction)))
    } else if intent.automatic {
        layout.destination(anchor, &intent.url)
    } else {
        (anchor, None)
    };
    if let Some(direction) = split {
        let stacks: Vec<_> = layout
            .stacks(pane)
            .into_iter()
            .filter(|existing| *existing != stack)
            .collect();
        pane = writer.split(
            pane,
            direction,
            &stacks,
            intent.focus,
            layout.is_split(pane),
        );
    }
    writer
        .commands
        .entity(stack)
        .insert(ChildOf(pane))
        .remove::<Unplaced>();
    writer.touch(pane);
    if intent.focus {
        writer.focus(stack, pane, layout.tab(pane).or_else(|| layout.tab(anchor)));
    }
    writer.commands.entity(operation).remove::<Pending>();
    writer.commands.trigger(StackPlaced {
        operation,
        stack,
        result: Ok(pane),
    });
}

fn retire(
    routed: Query<
        (
            Entity,
            &EntityTarget<Stack>,
            &OpenIntent,
            &PreviousNavigation,
        ),
        With<Routed>,
    >,
    stacks: Query<Option<&vmux_ecs::PageNavigation>, With<Stack>>,
    tasks: Query<&vmux_ecs::PageOpenTask>,
    mut commands: Commands,
) {
    for (operation, target, intent, previous) in &routed {
        let accepted = stacks
            .get(target.entity())
            .is_ok_and(|navigation| navigation.map(|navigation| navigation.id()) != previous.0)
            || tasks
                .iter()
                .any(|task| task.stack == target.entity() && task.url == intent.url);
        if accepted || stacks.get(target.entity()).is_err() {
            commands.entity(operation).despawn();
        }
    }
}
