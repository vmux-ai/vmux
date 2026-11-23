use bevy::ecs::system::Commands;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

#[derive(SystemParam)]
pub struct SiblingOrder<'w, 's> {
    commands: Commands<'w, 's>,
}

impl SiblingOrder<'_, '_> {
    pub fn index(entity: Entity, children: &Children, kind_positions: &[usize]) -> Option<usize> {
        kind_positions
            .iter()
            .position(|&position| children[position] == entity)
    }

    pub fn previous(active: usize) -> Option<(usize, usize)> {
        active.checked_sub(1).map(|previous| (active, previous))
    }

    pub fn next(active: usize, len: usize) -> Option<(usize, usize)> {
        (active + 1 < len).then(|| (active, active + 1))
    }

    pub fn swap(
        &mut self,
        parent: Entity,
        children: &Children,
        kind_positions: &[usize],
        a: usize,
        b: usize,
    ) {
        if a == b {
            return;
        }
        let Some(&position_a) = kind_positions.get(a) else {
            return;
        };
        let Some(&position_b) = kind_positions.get(b) else {
            return;
        };
        let mut reordered = children.iter().collect::<Vec<_>>();
        reordered.swap(position_a, position_b);
        self.apply(parent, reordered);
    }

    pub fn move_to(
        &mut self,
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
        let mut reordered = children.iter().collect::<Vec<_>>();
        for (position, entity) in kind_positions.iter().zip(kinds) {
            reordered[*position] = entity;
        }
        self.apply(parent, reordered);
    }

    fn apply(&mut self, parent: Entity, children: Vec<Entity>) {
        for &child in &children {
            self.commands.entity(child).remove::<ChildOf>();
        }
        for child in children {
            self.commands.entity(child).insert(ChildOf(parent));
        }
    }
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
            .run_system_once(move |children: Query<&Children>, mut order: SiblingOrder| {
                let siblings = children.get(parent).unwrap();
                order.move_to(parent, siblings, &[0, 2, 3], 0, 2);
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
