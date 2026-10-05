use bevy::prelude::*;
use moonshine_save::prelude::*;
use vmux_ecs::persistence::PersistenceAppExt;
use vmux_history::LastActivatedAt;

use super::{Pane, PaneSplit};
use crate::stack::Stack;

pub(super) struct IdentityPlugin;

impl Plugin for IdentityPlugin {
    fn build(&self, app: &mut App) {
        app.register_persisted::<PaneId>()
            .register_type::<SpawnSeq>()
            .add_systems(Update, repair_stack_parents)
            .add_systems(Update, stamp_spawn_seq)
            .add_systems(Update, assign_ids)
            .add_systems(
                Startup,
                (spawn_spawn_counter, reseed_spawn_counter)
                    .chain()
                    .in_set(crate::LayoutStartupSet::Post),
            );
    }
}

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct PaneId(pub String);

#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct SpawnSeq(pub u64);

#[derive(Component, Default)]
pub struct SpawnCounter(pub u64);

fn spawn_spawn_counter(mut commands: Commands) {
    commands.spawn((Name::new("Pane spawn counter"), SpawnCounter::default()));
}

fn assign_ids(panes: Query<Entity, (With<Pane>, Without<PaneId>)>, mut commands: Commands) {
    for entity in &panes {
        commands
            .entity(entity)
            .insert(PaneId(uuid::Uuid::new_v4().to_string()));
    }
}

fn stamp_spawn_seq(
    mut counter: Single<&mut SpawnCounter>,
    new_panes: Query<Entity, (With<Pane>, Without<SpawnSeq>)>,
    mut commands: Commands,
) {
    for pane in &new_panes {
        counter.0 += 1;
        commands.entity(pane).insert(SpawnSeq(counter.0));
    }
}

fn reseed_spawn_counter(seqs: Query<&SpawnSeq>, mut counter: Single<&mut SpawnCounter>) {
    let max = seqs.iter().map(|seq| seq.0).max().unwrap_or(0);
    if counter.0 <= max {
        counter.0 = max + 1;
    }
}

fn repair_stack_parents(
    splits: Query<
        (Entity, &Children),
        (With<PaneSplit>, Or<(Added<PaneSplit>, Changed<Children>)>),
    >,
    panes: super::tree::PaneHierarchy,
    stacks: Query<(), With<Stack>>,
    mut commands: Commands,
) {
    for (split, children) in &splits {
        let direct_stacks: Vec<Entity> = children
            .iter()
            .filter(|&child| stacks.contains(child))
            .collect();
        if direct_stacks.is_empty() {
            continue;
        }
        let mut leaf = panes.first_leaf(split);
        if leaf == split {
            leaf = commands
                .spawn((Pane::bundle(), LastActivatedAt::now(), ChildOf(split)))
                .id();
        }
        warn!(
            "Repairing {} stack(s) parented directly to pane split {:?}",
            direct_stacks.len(),
            split
        );
        for stack in direct_stacks {
            commands.entity(stack).insert(ChildOf(leaf));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::PaneSplitDirection;
    use bevy::ecs::relationship::Relationship;

    #[test]
    fn repair_direct_stack_child_of_split_moves_it_to_leaf() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, repair_stack_parents);
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(split)))
            .id();
        let leaf = app.world_mut().spawn((Pane, ChildOf(split))).id();

        app.update();

        assert_eq!(
            app.world().get::<ChildOf>(stack).map(Relationship::get),
            Some(leaf)
        );
        assert!(
            app.world()
                .get::<Children>(split)
                .is_some_and(|children| !children.contains(&stack))
        );
    }

    #[test]
    fn stamp_spawn_seq_assigns_increasing_values_to_new_panes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, stamp_spawn_seq);
        app.world_mut().spawn(SpawnCounter::default());

        let a = app.world_mut().spawn(Pane).id();
        app.update();
        let b = app.world_mut().spawn(Pane).id();
        app.update();

        let a_seq = app.world().get::<SpawnSeq>(a).expect("a stamped").0;
        let b_seq = app.world().get::<SpawnSeq>(b).expect("b stamped").0;
        assert!(b_seq > a_seq);
    }

    #[test]
    fn reseed_spawn_counter_exceeds_max_existing() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, reseed_spawn_counter);

        app.world_mut().spawn(SpawnCounter::default());
        app.world_mut().spawn((Pane, SpawnSeq(7)));
        app.world_mut().spawn((Pane, SpawnSeq(3)));
        app.update();

        let counter = app
            .world_mut()
            .query::<&SpawnCounter>()
            .single(app.world())
            .unwrap();
        assert_eq!(counter.0, 8);
    }

    #[test]
    fn assign_pane_ids_fills_missing_and_keeps_existing() {
        let mut app = App::new();
        app.add_systems(Update, assign_ids);
        let bare = app.world_mut().spawn(Pane).id();
        let kept = app
            .world_mut()
            .spawn((Pane, PaneId("fixed".to_string())))
            .id();

        app.update();

        let assigned = app.world().get::<PaneId>(bare).expect("id assigned");
        assert!(!assigned.0.is_empty());
        assert_eq!(app.world().get::<PaneId>(kept).unwrap().0, "fixed");
    }
}
