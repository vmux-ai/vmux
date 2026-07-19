use bevy::prelude::*;

/// Swap two same-type siblings within a parent's Children.
/// `kind_positions` are indices into Children of entities matching the filter.
/// `a` and `b` are indices into that filtered list.
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

pub fn move_child_to_parent(
    commands: &mut Commands,
    child: Entity,
    target_parent: Entity,
    target_children: &Children,
    target: Option<Entity>,
    after: bool,
) {
    if target == Some(child) {
        return;
    }

    let mut ordered: Vec<Entity> = target_children
        .iter()
        .filter(|&entity| entity != child)
        .collect();
    let insert_at = target
        .and_then(|target| ordered.iter().position(|&entity| entity == target))
        .map(|index| index + usize::from(after))
        .unwrap_or(ordered.len());
    ordered.insert(insert_at, child);

    commands.entity(child).remove::<ChildOf>();
    for &entity in target_children {
        if entity != child {
            commands.entity(entity).remove::<ChildOf>();
        }
    }
    for entity in ordered {
        commands.entity(entity).insert(ChildOf(target_parent));
    }
}

/// Find the index of `entity` within the filtered positions list.
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
