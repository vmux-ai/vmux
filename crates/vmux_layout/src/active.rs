use bevy::prelude::*;
use vmux_core::Active;
use vmux_history::LastActivatedAt;

use crate::pane::{Pane, PaneSplit};
use crate::space::Space;
use crate::stack::Stack;
use crate::tab::Tab;

fn pick_active(candidates: impl IntoIterator<Item = (Entity, i64)>) -> Option<Entity> {
    candidates
        .into_iter()
        .max_by_key(|(_, ts)| *ts)
        .map(|(entity, _)| entity)
}

pub fn ensure_active_tab(
    spaces: Query<&Children, With<Space>>,
    tabs: Query<(Entity, &LastActivatedAt, Has<Active>), With<Tab>>,
    mut commands: Commands,
) {
    for children in &spaces {
        let mut candidates = Vec::new();
        let mut has_active = false;
        for child in children.iter() {
            if let Ok((entity, ts, active)) = tabs.get(child) {
                candidates.push((entity, ts.0));
                has_active |= active;
            }
        }
        if has_active || candidates.is_empty() {
            continue;
        }
        if let Some(target) = pick_active(candidates) {
            commands.entity(target).insert(Active);
        }
    }
}

pub fn ensure_active_stack(
    leaves: Query<&Children, (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<(Entity, &LastActivatedAt, Has<Active>), With<Stack>>,
    mut commands: Commands,
) {
    for children in &leaves {
        let mut candidates = Vec::new();
        let mut has_active = false;
        for child in children.iter() {
            if let Ok((entity, ts, active)) = stacks.get(child) {
                candidates.push((entity, ts.0));
                has_active |= active;
            }
        }
        if has_active || candidates.is_empty() {
            continue;
        }
        if let Some(target) = pick_active(candidates) {
            commands.entity(target).insert(Active);
        }
    }
}

pub fn ensure_active_branch(
    splits: Query<&Children, With<PaneSplit>>,
    branches: Query<(Entity, Option<&LastActivatedAt>, Has<Active>), With<Pane>>,
    mut commands: Commands,
) {
    for children in &splits {
        let mut candidates = Vec::new();
        let mut has_active = false;
        for child in children.iter() {
            if let Ok((entity, ts, active)) = branches.get(child) {
                candidates.push((entity, ts.map(|t| t.0).unwrap_or(0)));
                has_active |= active;
            }
        }
        if has_active || candidates.is_empty() {
            continue;
        }
        if let Some(target) = pick_active(candidates) {
            commands.entity(target).insert(Active);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_active_tab_marks_max_last_activated_child() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, ensure_active_tab);
        let space = app.world_mut().spawn(Space).id();
        let _old = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(space)))
            .id();
        let newer = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(5), ChildOf(space)))
            .id();
        app.update();
        assert!(app.world().entity(newer).contains::<Active>());
        let active_count = app
            .world_mut()
            .query_filtered::<Entity, (With<Tab>, With<Active>)>()
            .iter(app.world())
            .count();
        assert_eq!(active_count, 1);
    }

    #[test]
    fn ensure_active_tab_is_noop_when_one_already_active() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, ensure_active_tab);
        let space = app.world_mut().spawn(Space).id();
        let already = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), Active, ChildOf(space)))
            .id();
        let _newer = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(5), ChildOf(space)))
            .id();
        app.update();
        assert!(app.world().entity(already).contains::<Active>());
        let active_count = app
            .world_mut()
            .query_filtered::<Entity, (With<Tab>, With<Active>)>()
            .iter(app.world())
            .count();
        assert_eq!(active_count, 1);
    }
}
