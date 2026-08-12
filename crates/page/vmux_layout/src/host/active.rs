use bevy::prelude::*;
use vmux_core::Active;
use vmux_history::LastActivatedAt;

use crate::pane::{Pane, PaneSplit};
use crate::space::Space;
use crate::stack::Stack;
use crate::tab::Tab;

fn apply_active(entries: &[(Entity, i64, bool)], commands: &mut Commands) {
    let Some(&(target, _, _)) = entries.iter().max_by_key(|(_, ts, _)| *ts) else {
        return;
    };
    for &(entity, _, active) in entries {
        if entity == target && !active {
            commands.entity(entity).insert(Active);
        } else if entity != target && active {
            commands.entity(entity).remove::<Active>();
        }
    }
}

pub fn ensure_active_space(
    spaces: Query<(Entity, Option<&LastActivatedAt>, Has<Active>), With<Space>>,
    mut commands: Commands,
) {
    let entries: Vec<(Entity, i64, bool)> = spaces
        .iter()
        .map(|(entity, ts, active)| (entity, ts.map(|t| t.0).unwrap_or(0), active))
        .collect();
    apply_active(&entries, &mut commands);
}

pub fn ensure_active_tab(
    spaces: Query<&Children, With<Space>>,
    tabs: Query<(&LastActivatedAt, Has<Active>), With<Tab>>,
    mut commands: Commands,
) {
    for children in &spaces {
        let mut entries = Vec::new();
        for child in children.iter() {
            if let Ok((ts, active)) = tabs.get(child) {
                entries.push((child, ts.0, active));
            }
        }
        apply_active(&entries, &mut commands);
    }
}

pub fn ensure_active_stack(
    leaves: Query<&Children, (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<(&LastActivatedAt, Has<Active>), With<Stack>>,
    mut commands: Commands,
) {
    for children in &leaves {
        let mut entries = Vec::new();
        for child in children.iter() {
            if let Ok((ts, active)) = stacks.get(child) {
                entries.push((child, ts.0, active));
            }
        }
        apply_active(&entries, &mut commands);
    }
}

pub fn ensure_active_branch(
    splits: Query<&Children, With<PaneSplit>>,
    branches: Query<(Option<&LastActivatedAt>, Has<Active>), With<Pane>>,
    mut commands: Commands,
) {
    for children in &splits {
        let mut entries = Vec::new();
        for child in children.iter() {
            if let Ok((ts, active)) = branches.get(child) {
                entries.push((child, ts.map(|t| t.0).unwrap_or(0), active));
            }
        }
        apply_active(&entries, &mut commands);
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
        let older = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(space)))
            .id();
        let newer = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(5), ChildOf(space)))
            .id();
        app.update();
        assert!(app.world().entity(newer).contains::<Active>());
        assert!(!app.world().entity(older).contains::<Active>());
        let active_count = app
            .world_mut()
            .query_filtered::<Entity, (With<Tab>, With<Active>)>()
            .iter(app.world())
            .count();
        assert_eq!(active_count, 1);
    }

    #[test]
    fn ensure_active_tab_moves_active_off_stale_child() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, ensure_active_tab);
        let space = app.world_mut().spawn(Space).id();
        let stale = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), Active, ChildOf(space)))
            .id();
        let newer = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(5), ChildOf(space)))
            .id();
        app.update();
        assert!(app.world().entity(newer).contains::<Active>());
        assert!(!app.world().entity(stale).contains::<Active>());
    }

    #[test]
    fn ensure_active_space_marks_max_last_activated_space() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, ensure_active_space);
        let older = app.world_mut().spawn((Space, LastActivatedAt(1))).id();
        let newer = app.world_mut().spawn((Space, LastActivatedAt(9))).id();
        app.update();
        assert!(app.world().entity(newer).contains::<Active>());
        assert!(!app.world().entity(older).contains::<Active>());
    }
}
