use bevy::prelude::*;
use std::collections::HashMap;
use vmux_command::snapshot::CommandBarProjection;
use vmux_layout::event::TERMINAL_PAGE_URL;

use crate::pid::{Pid, PidToEntity};

pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update_terminals_snapshot.in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
        );
    }
}

fn update_terminals_snapshot(
    pid_maps: Query<Ref<PidToEntity>>,
    mut state: Single<&mut CommandBarProjection>,
) {
    let pid_map = pid_maps.iter().next();
    let changed = pid_map
        .as_ref()
        .is_some_and(|map| map.is_changed() || map.is_added());
    if !changed && !state.terminals.terminal_page_url.is_empty() {
        return;
    }
    let mut running = HashMap::new();
    if let Some(pid_map) = pid_map {
        for (pid, entity) in pid_map.iter() {
            running.insert(Pid(pid).page_url(), entity);
        }
    }
    state.terminals.running = running;
    state.terminals.terminal_page_url = TERMINAL_PAGE_URL.to_string();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projection(app: &App) -> &CommandBarProjection {
        app.world()
            .iter_entities()
            .find_map(|entity| entity.get::<CommandBarProjection>())
            .unwrap()
    }

    #[test]
    fn writes_url_and_no_running_terminals() {
        let mut app = App::new();
        app.add_systems(Update, update_terminals_snapshot);
        app.world_mut().spawn(CommandBarProjection::default());
        app.update();
        let snap = &projection(&app).terminals;
        assert_eq!(snap.terminal_page_url, TERMINAL_PAGE_URL);
        assert!(snap.running.is_empty());
    }

    #[test]
    fn running_terminals_are_keyed_by_the_url_the_row_carries() {
        let mut app = App::new();
        app.add_systems(Update, update_terminals_snapshot);
        app.world_mut().spawn(CommandBarProjection::default());
        let pane = app.world_mut().spawn_empty().id();
        app.world_mut()
            .spawn([(4321, pane)].into_iter().collect::<PidToEntity>());

        app.update();

        let snap = &projection(&app).terminals;
        assert_eq!(snap.running.get("vmux://terminal/4321"), Some(&pane));
    }
}
