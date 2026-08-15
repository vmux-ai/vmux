//! Disposing of an empty stack a launcher never used.
//!
//! Opening the launcher on an empty pane hands it a fresh stack to fill. Dismissing without picking
//! anything has to undo that, and it is more than a despawn: Cmd+T creates a whole tab to hold the
//! one stack, so abandoning it should take the tab too, and which of the two happened decides where
//! the keyboard goes back to.
//!
//! All of that is workspace shape, so the launcher only reports
//! [`PendingStackAbandoned`](vmux_core::launcher::PendingStackAbandoned) and this answers it.

use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::CefKeyboardTarget;
use vmux_core::launcher::PendingStackAbandoned;
use vmux_history::LastActivatedAt;

use crate::cef::Browser;
use crate::stack::Stack;
use crate::tab::{Tab, pick_after_close};

pub(crate) struct PendingStackPlugin;

impl Plugin for PendingStackPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PendingStackAbandoned>().add_systems(
            Update,
            discard_abandoned_pending_stacks.before(crate::stack::ComputeFocusSet),
        );
    }
}

fn discard_abandoned_pending_stacks(
    mut abandoned: MessageReader<PendingStackAbandoned>,
    tab_q: Query<(Entity, &LastActivatedAt), With<Tab>>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    stack_q: Query<Entity, With<Stack>>,
    content_browsers: Query<Entity, With<Browser>>,
    mut commands: Commands,
) {
    for event in abandoned.read() {
        let closed_tab = Tab::close_if_only_holds(
            event.stack,
            &tab_q,
            &child_of_q,
            &all_children,
            &stack_q,
            &mut commands,
        );
        if closed_tab {
            continue;
        }
        commands.entity(event.stack).despawn();
        let Some(previous) = event.previous_stack else {
            continue;
        };
        let Ok(children) = all_children.get(previous) else {
            continue;
        };
        for child in children.iter() {
            if content_browsers.contains(child) {
                commands.entity(child).try_insert(CefKeyboardTarget);
            }
        }
    }
}

impl Tab {
    /// Despawns the tab holding `stack` when that stack is the only thing in it, and hands the
    /// active marker to a sibling. Reports whether it did.
    ///
    /// Refuses when the tab is the last one: a workspace with no tabs has nothing to show.
    fn close_if_only_holds(
        stack: Entity,
        tab_q: &Query<(Entity, &LastActivatedAt), With<Tab>>,
        child_of_q: &Query<&ChildOf>,
        all_children: &Query<&Children>,
        stack_q: &Query<Entity, With<Stack>>,
        commands: &mut Commands,
    ) -> bool {
        let Some(tab) = Self::ancestor_of(stack, tab_q, child_of_q) else {
            return false;
        };
        if subtree_holds_another_stack(tab, stack, all_children, stack_q) {
            return false;
        }
        let siblings = Self::siblings_of(tab, tab_q, child_of_q, all_children);
        if siblings.len() <= 1 {
            return false;
        }
        if let Some(next) = pick_after_close(tab, &siblings) {
            commands.entity(next).insert(LastActivatedAt::now());
        }
        commands.entity(tab).despawn();
        true
    }

    fn ancestor_of(
        entity: Entity,
        tab_q: &Query<(Entity, &LastActivatedAt), With<Tab>>,
        child_of_q: &Query<&ChildOf>,
    ) -> Option<Entity> {
        let mut current = entity;
        while let Ok(parent) = child_of_q.get(current).map(Relationship::get) {
            if tab_q.get(parent).is_ok() {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    fn siblings_of(
        tab: Entity,
        tab_q: &Query<(Entity, &LastActivatedAt), With<Tab>>,
        child_of_q: &Query<&ChildOf>,
        all_children: &Query<&Children>,
    ) -> Vec<Entity> {
        let Ok(parent) = child_of_q.get(tab).map(Relationship::get) else {
            return vec![tab];
        };
        let Ok(children) = all_children.get(parent) else {
            return vec![tab];
        };
        children.iter().filter(|e| tab_q.get(*e).is_ok()).collect()
    }
}

fn subtree_holds_another_stack(
    entity: Entity,
    ignored_stack: Entity,
    all_children: &Query<&Children>,
    stack_q: &Query<Entity, With<Stack>>,
) -> bool {
    (stack_q.contains(entity) && entity != ignored_stack)
        || all_children.get(entity).is_ok_and(|children| {
            children.iter().any(|child| {
                subtree_holds_another_stack(child, ignored_stack, all_children, stack_q)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Workspace {
        app: App,
        root: Entity,
    }

    impl Workspace {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<PendingStackAbandoned>()
                .add_systems(Update, discard_abandoned_pending_stacks);
            let root = app.world_mut().spawn_empty().id();
            Self { app, root }
        }

        fn tab(&mut self) -> Entity {
            self.app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt::now(), ChildOf(self.root)))
                .id()
        }

        fn stack_in(&mut self, tab: Entity) -> Entity {
            self.app
                .world_mut()
                .spawn((Stack::default(), ChildOf(tab)))
                .id()
        }

        fn abandon(&mut self, stack: Entity) {
            self.app.world_mut().write_message(PendingStackAbandoned {
                stack,
                previous_stack: None,
            });
            self.app.update();
        }

        fn exists(&self, entity: Entity) -> bool {
            self.app.world().get_entity(entity).is_ok()
        }
    }

    /// Cmd+T makes a tab just to hold the stack the launcher opens on, so walking away from the
    /// launcher has to take the tab with it — otherwise every dismissed Cmd+T leaves an empty tab.
    #[test]
    fn abandoning_the_only_stack_in_a_tab_closes_the_tab() {
        let mut workspace = Workspace::new();
        let keeper = workspace.tab();
        workspace.stack_in(keeper);
        let throwaway = workspace.tab();
        let stack = workspace.stack_in(throwaway);

        workspace.abandon(stack);

        assert!(!workspace.exists(throwaway));
        assert!(workspace.exists(keeper));
    }

    #[test]
    fn abandoning_a_stack_beside_others_closes_only_that_stack() {
        let mut workspace = Workspace::new();
        let tab = workspace.tab();
        let sibling = workspace.stack_in(tab);
        let stack = workspace.stack_in(tab);

        workspace.abandon(stack);

        assert!(!workspace.exists(stack));
        assert!(workspace.exists(sibling));
        assert!(workspace.exists(tab));
    }

    /// Closing the last tab would leave a workspace with nothing to show, so the stack goes and
    /// the tab stays.
    #[test]
    fn abandoning_the_only_stack_in_the_only_tab_keeps_the_tab() {
        let mut workspace = Workspace::new();
        let tab = workspace.tab();
        let stack = workspace.stack_in(tab);

        workspace.abandon(stack);

        assert!(!workspace.exists(stack));
        assert!(workspace.exists(tab));
    }
}
