use bevy::prelude::*;
use moonshine_save::prelude::*;
use vmux_history::LastActivatedAt;

use super::{
    pane::{Pane, PaneSplit, first_leaf_descendant, leaf_pane_bundle},
    stack::Stack,
};

pub(super) struct IdentityPlugin;

impl Plugin for IdentityPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PaneId>()
            .register_type::<SpawnSeq>()
            .init_resource::<SpawnCounter>()
            .add_systems(Update, repair_stacks_parented_to_splits)
            .add_systems(Update, stamp_spawn_seq)
            .add_systems(Update, assign_pane_ids)
            .add_systems(
                Startup,
                reseed_spawn_counter.in_set(crate::LayoutStartupSet::Post),
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

#[derive(Resource, Default)]
pub struct SpawnCounter(pub u64);

fn assign_pane_ids(panes: Query<Entity, (With<Pane>, Without<PaneId>)>, mut commands: Commands) {
    for entity in &panes {
        commands
            .entity(entity)
            .insert(PaneId(uuid::Uuid::new_v4().to_string()));
    }
}

fn stamp_spawn_seq(
    mut counter: ResMut<SpawnCounter>,
    new_panes: Query<Entity, (With<Pane>, Without<SpawnSeq>)>,
    mut commands: Commands,
) {
    for pane in &new_panes {
        counter.0 += 1;
        commands.entity(pane).insert(SpawnSeq(counter.0));
    }
}

fn reseed_spawn_counter(seqs: Query<&SpawnSeq>, mut counter: ResMut<SpawnCounter>) {
    let max = seqs.iter().map(|seq| seq.0).max().unwrap_or(0);
    if counter.0 <= max {
        counter.0 = max + 1;
    }
}

fn repair_stacks_parented_to_splits(
    splits: Query<
        (Entity, &Children),
        (With<PaneSplit>, Or<(Added<PaneSplit>, Changed<Children>)>),
    >,
    pane_children: Query<&Children, With<Pane>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
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
        let mut leaf = first_leaf_descendant(split, &pane_children, &leaf_panes);
        if leaf == split {
            leaf = commands
                .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(split)))
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
            .add_systems(Update, repair_stacks_parented_to_splits);
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
            .init_resource::<SpawnCounter>()
            .add_systems(Update, stamp_spawn_seq);

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
            .init_resource::<SpawnCounter>()
            .add_systems(Update, reseed_spawn_counter);

        app.world_mut().spawn((Pane, SpawnSeq(7)));
        app.world_mut().spawn((Pane, SpawnSeq(3)));
        app.update();

        assert_eq!(app.world().resource::<SpawnCounter>().0, 8);
    }

    #[test]
    fn assign_pane_ids_fills_missing_and_keeps_existing() {
        let mut app = App::new();
        app.add_systems(Update, assign_pane_ids);
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
