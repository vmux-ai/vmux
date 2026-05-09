use crate::event::SideSheetDragCommand;
use bevy::prelude::*;

use crate::pane::Pane;
use crate::stack::Stack;

pub fn handle_drag_commands(
    mut commands: Commands,
    mut events: MessageReader<SideSheetDragCommand>,
) {
    for event in events.read() {
        let event = event.clone();
        commands.queue(move |world: &mut World| match event {
            SideSheetDragCommand::MoveStack {
                from_pane,
                from_index,
                to_pane,
                to_index,
            } => {
                move_tab_impl(world, from_pane, from_index, to_pane, to_index);
            }
            SideSheetDragCommand::SwapPane { pane, target } => {
                swap_pane_impl(world, pane, target);
            }
            SideSheetDragCommand::SplitPane { .. } => {}
        });
    }
}

pub fn swap_pane_impl(world: &mut World, a_id: u64, b_id: u64) {
    let a = Entity::from_bits(a_id);
    let b = Entity::from_bits(b_id);
    if a == b {
        return;
    }

    if !world.get_entity(a).is_ok_and(|e| e.contains::<Pane>()) {
        return;
    }
    if !world.get_entity(b).is_ok_and(|e| e.contains::<Pane>()) {
        return;
    }

    let a_parent = world.get::<ChildOf>(a).map(|p| p.parent());
    let b_parent = world.get::<ChildOf>(b).map(|p| p.parent());
    let a_idx = a_parent.and_then(|p| {
        world
            .get::<Children>(p)
            .and_then(|c| c.iter().position(|e| e == a))
    });
    let b_idx = b_parent.and_then(|p| {
        world
            .get::<Children>(p)
            .and_then(|c| c.iter().position(|e| e == b))
    });

    match (a_parent, b_parent, a_idx, b_idx) {
        (Some(ap), Some(bp), Some(ai), Some(bi)) if ap == bp => {
            crate::swap::move_to_index(world, a, ap, bi);
            crate::swap::move_to_index(world, b, ap, ai);
        }
        (Some(ap), Some(bp), Some(ai), Some(bi)) => {
            crate::swap::move_to_index(world, a, bp, bi);
            crate::swap::move_to_index(world, b, ap, ai);
        }
        _ => {}
    }
}

pub fn move_tab_impl(
    world: &mut World,
    from_pane_id: u64,
    from_index: usize,
    to_pane_id: u64,
    to_index: usize,
) {
    let from_pane = Entity::from_bits(from_pane_id);
    let to_pane = Entity::from_bits(to_pane_id);

    if !world
        .get_entity(from_pane)
        .is_ok_and(|e| e.contains::<Pane>())
    {
        return;
    }
    if !world
        .get_entity(to_pane)
        .is_ok_and(|e| e.contains::<Pane>())
    {
        return;
    }

    let Some(tab) = world
        .get::<Children>(from_pane)
        .and_then(|c| c.get(from_index).copied())
    else {
        return;
    };

    if !world.get_entity(tab).is_ok_and(|e| e.contains::<Stack>()) {
        return;
    }

    crate::swap::move_to_index(world, tab, to_pane, to_index);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::{PaneSplit, PaneSplitDirection};

    fn spawn_split(world: &mut World, dir: PaneSplitDirection) -> Entity {
        world.spawn(PaneSplit { direction: dir }).id()
    }

    fn spawn_pane_with_tabs(world: &mut World, n: usize) -> (Entity, Vec<Entity>) {
        let pane = world.spawn(Pane).id();
        let tabs: Vec<Entity> = (0..n)
            .map(|_| world.spawn((Stack::default(), ChildOf(pane))).id())
            .collect();
        (pane, tabs)
    }

    #[test]
    fn move_tab_within_pane_reorders() {
        let mut world = World::new();
        let (pane, tabs) = spawn_pane_with_tabs(&mut world, 3);
        let pane_id = pane.to_bits();

        move_tab_impl(&mut world, pane_id, 2, pane_id, 0);

        let kids = world.get::<Children>(pane).unwrap();
        assert_eq!(&**kids, &[tabs[2], tabs[0], tabs[1]]);
    }

    #[test]
    fn move_tab_across_panes() {
        let mut world = World::new();
        let (p1, t1) = spawn_pane_with_tabs(&mut world, 2);
        let (p2, t2) = spawn_pane_with_tabs(&mut world, 1);

        move_tab_impl(&mut world, p1.to_bits(), 0, p2.to_bits(), 0);

        let p1_kids = world.get::<Children>(p1).unwrap();
        assert_eq!(&**p1_kids, &[t1[1]]);

        let p2_kids = world.get::<Children>(p2).unwrap();
        assert_eq!(&**p2_kids, &[t1[0], t2[0]]);
    }

    #[test]
    fn move_tab_rejects_non_pane_source() {
        let mut world = World::new();
        let not_a_pane = world.spawn_empty().id();
        let (p, tabs) = spawn_pane_with_tabs(&mut world, 1);

        move_tab_impl(&mut world, not_a_pane.to_bits(), 0, p.to_bits(), 0);

        let kids = world.get::<Children>(p).unwrap();
        assert_eq!(&**kids, &[tabs[0]]);
    }

    #[test]
    fn move_tab_rejects_non_pane_destination() {
        let mut world = World::new();
        let (p, _) = spawn_pane_with_tabs(&mut world, 2);
        let not_a_pane = world.spawn_empty().id();

        move_tab_impl(&mut world, p.to_bits(), 0, not_a_pane.to_bits(), 0);

        let kids = world.get::<Children>(not_a_pane);
        assert!(kids.is_none() || kids.unwrap().is_empty());
    }

    #[test]
    fn swap_pane_same_parent_swaps_positions() {
        let mut world = World::new();
        let split = spawn_split(&mut world, PaneSplitDirection::Row);
        let a = world.spawn((Pane, ChildOf(split))).id();
        let b = world.spawn((Pane, ChildOf(split))).id();

        swap_pane_impl(&mut world, a.to_bits(), b.to_bits());

        let kids = world.get::<Children>(split).unwrap();
        assert_eq!(&**kids, &[b, a]);
    }

    #[test]
    fn swap_pane_cross_parent_exchanges_slots() {
        let mut world = World::new();
        let root = spawn_split(&mut world, PaneSplitDirection::Row);
        let outer_a = world.spawn((Pane, ChildOf(root))).id();
        let col = world
            .spawn((
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                ChildOf(root),
            ))
            .id();
        let inner_a = world.spawn((Pane, ChildOf(col))).id();
        let inner_b = world.spawn((Pane, ChildOf(col))).id();

        swap_pane_impl(&mut world, outer_a.to_bits(), inner_b.to_bits());

        let root_kids = world.get::<Children>(root).unwrap();
        assert_eq!(&**root_kids, &[inner_b, col]);

        let col_kids = world.get::<Children>(col).unwrap();
        assert_eq!(&**col_kids, &[inner_a, outer_a]);
    }

    #[test]
    fn swap_pane_rejects_non_pane_ids() {
        let mut world = World::new();
        let split = spawn_split(&mut world, PaneSplitDirection::Row);
        let a = world.spawn((Pane, ChildOf(split))).id();
        let not_pane = world.spawn(ChildOf(split)).id();

        swap_pane_impl(&mut world, a.to_bits(), not_pane.to_bits());

        let kids = world.get::<Children>(split).unwrap();
        assert_eq!(&**kids, &[a, not_pane]);
    }
}
