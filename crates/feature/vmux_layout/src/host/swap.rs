use bevy::prelude::*;

pub struct SiblingOrder {
    parent: Entity,
    children: Vec<Entity>,
}

impl SiblingOrder {
    pub fn swapped(
        parent: Entity,
        children: &Children,
        kind_positions: &[usize],
        a: usize,
        b: usize,
    ) -> Option<Self> {
        if a == b {
            return None;
        }
        let &position_a = kind_positions.get(a)?;
        let &position_b = kind_positions.get(b)?;
        let mut children = children.iter().collect::<Vec<_>>();
        children.swap(position_a, position_b);
        Some(Self { parent, children })
    }

    pub fn moved(
        parent: Entity,
        children: &Children,
        kind_positions: &[usize],
        from: usize,
        to: usize,
    ) -> Option<Self> {
        if from == to {
            return None;
        }
        let mut kinds = kind_positions
            .iter()
            .map(|position| children[*position])
            .collect::<Vec<_>>();
        if from >= kinds.len() || to >= kinds.len() {
            return None;
        }
        let moved = kinds.remove(from);
        kinds.insert(to, moved);
        let mut children = children.iter().collect::<Vec<_>>();
        for (position, entity) in kind_positions.iter().zip(kinds) {
            children[*position] = entity;
        }
        Some(Self { parent, children })
    }
}

impl Command for SiblingOrder {
    type Out = ();

    fn apply(self, world: &mut World) {
        for child in &self.children {
            world.entity_mut(*child).remove::<ChildOf>();
        }
        for child in self.children {
            world.entity_mut(child).insert(ChildOf(self.parent));
        }
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
                if let Some(order) = SiblingOrder::moved(parent, siblings, &[0, 2, 3], 0, 2) {
                    commands.queue(order);
                }
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
