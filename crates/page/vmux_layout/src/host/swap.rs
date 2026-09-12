use bevy::prelude::*;

pub fn swap_siblings(
    commands: &mut Commands,
    parent: Entity,
    children: &Children,
    kind_positions: &[usize],
    a: usize,
    b: usize,
) {
    if a == b {
        return;
    }
    let Some(&pos_a) = kind_positions.get(a) else {
        return;
    };
    let Some(&pos_b) = kind_positions.get(b) else {
        return;
    };

    let mut ordered: Vec<Entity> = children.iter().collect();
    ordered.swap(pos_a, pos_b);

    for &child in &ordered {
        commands.entity(child).remove::<ChildOf>();
    }
    for &child in &ordered {
        commands.entity(child).insert(ChildOf(parent));
    }
}

pub fn move_sibling(
    commands: &mut Commands,
    parent: Entity,
    children: &Children,
    kind_positions: &[usize],
    from: usize,
    to: usize,
) {
    if from == to {
        return;
    }
    let mut kinds = kind_positions
        .iter()
        .map(|position| children[*position])
        .collect::<Vec<_>>();
    if from >= kinds.len() || to >= kinds.len() {
        return;
    }
    let moved = kinds.remove(from);
    kinds.insert(to, moved);
    let mut ordered: Vec<Entity> = children.iter().collect();
    for (position, entity) in kind_positions.iter().zip(kinds) {
        ordered[*position] = entity;
    }
    for &child in &ordered {
        commands.entity(child).remove::<ChildOf>();
    }
    for &child in &ordered {
        commands.entity(child).insert(ChildOf(parent));
    }
}

pub fn find_kind_index(
    entity: Entity,
    children: &Children,
    kind_positions: &[usize],
) -> Option<usize> {
    kind_positions
        .iter()
        .position(|&pos| children[pos] == entity)
}

pub fn resolve_prev(active_idx: usize) -> Option<(usize, usize)> {
    active_idx.checked_sub(1).map(|p| (active_idx, p))
}

pub fn resolve_next(active_idx: usize, len: usize) -> Option<(usize, usize)> {
    (active_idx + 1 < len).then(|| (active_idx, active_idx + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn move_sibling_supports_arbitrary_reordering_without_moving_other_kinds() {
        let mut app = App::new();
        let parent = app.world_mut().spawn_empty().id();
        let first = app.world_mut().spawn(ChildOf(parent)).id();
        let separator = app.world_mut().spawn(ChildOf(parent)).id();
        let second = app.world_mut().spawn(ChildOf(parent)).id();
        let third = app.world_mut().spawn(ChildOf(parent)).id();

        app.world_mut()
            .run_system_once(move |children: Query<&Children>, mut commands: Commands| {
                let siblings = children.get(parent).unwrap();
                move_sibling(&mut commands, parent, siblings, &[0, 2, 3], 0, 2);
            })
            .unwrap();

        assert_eq!(
            app.world()
                .get::<Children>(parent)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            [second, separator, third, first]
        );
    }
}
