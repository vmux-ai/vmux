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
#[path = "active.test.rs"]
mod tests;
