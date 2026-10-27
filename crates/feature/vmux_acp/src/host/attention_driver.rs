use bevy::prelude::*;
use vmux_layout::stack::{FocusedStack, Stack};

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AttentionContext<'w, 's> {
    windows: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    pub(super) focused: FocusedStack<'w, 's>,
    stacks: Query<'w, 's, (), With<Stack>>,
    child_of: Query<'w, 's, &'static ChildOf>,
}

impl AttentionContext<'_, '_> {
    pub(super) fn foreground(&self) -> bool {
        self.windows
            .iter()
            .next()
            .map(|window| window.focused && window.visible)
            .unwrap_or(false)
    }

    pub(super) fn stack(&self, entity: Entity) -> Option<Entity> {
        self.stacks
            .get(entity)
            .is_ok()
            .then_some(entity)
            .or_else(|| self.child_of.get(entity).ok().map(|child| child.parent()))
    }

    pub(super) fn viewed(&self, entity: Entity) -> bool {
        self.foreground() && self.focused.stack == self.stack(entity)
    }
}
