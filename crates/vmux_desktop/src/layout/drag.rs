use bevy::prelude::*;
use vmux_side_sheet::event::SideSheetDragCommand;

use crate::layout::pane::Pane;
use crate::layout::tab::Tab;

pub(crate) fn handle_drag_commands(
    mut commands: Commands,
    mut events: MessageReader<SideSheetDragCommand>,
) {
    for event in events.read() {
        let event = event.clone();
        commands.queue(move |world: &mut World| match event {
            SideSheetDragCommand::MoveTab {
                from_pane,
                from_index,
                to_pane,
                to_index,
            } => {
                move_tab_impl(world, from_pane, from_index, to_pane, to_index);
            }
            SideSheetDragCommand::SwapPane { .. } => {}
            SideSheetDragCommand::SplitPane { .. } => {}
        });
    }
}

pub(crate) fn move_tab_impl(
    world: &mut World,
    from_pane_id: u64,
    from_index: usize,
    to_pane_id: u64,
    to_index: usize,
) {
    let from_pane = Entity::from_bits(from_pane_id);
    let to_pane = Entity::from_bits(to_pane_id);

    if !world.get_entity(from_pane).is_ok_and(|e| e.contains::<Pane>()) {
        return;
    }
    if !world.get_entity(to_pane).is_ok_and(|e| e.contains::<Pane>()) {
        return;
    }

    let Some(tab) = world
        .get::<Children>(from_pane)
        .and_then(|c| c.get(from_index).copied())
    else {
        return;
    };

    if !world.get_entity(tab).is_ok_and(|e| e.contains::<Tab>()) {
        return;
    }

    crate::layout::swap::move_to_index(world, tab, to_pane, to_index);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_pane_with_tabs(world: &mut World, n: usize) -> (Entity, Vec<Entity>) {
        let pane = world.spawn(Pane).id();
        let tabs: Vec<Entity> = (0..n)
            .map(|_| world.spawn((Tab::default(), ChildOf(pane))).id())
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
}
