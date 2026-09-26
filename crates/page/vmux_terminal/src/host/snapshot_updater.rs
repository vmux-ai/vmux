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
    pid_map: Option<Res<PidToEntity>>,
    mut state: ResMut<CommandBarProjection>,
) {
    let changed = pid_map
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    if !changed && !state.terminals.terminal_page_url.is_empty() {
        return;
    }
    let mut running = HashMap::new();
    if let Some(pid_map) = pid_map.as_deref() {
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

    #[test]
    fn writes_url_and_no_running_terminals() {
        let mut app = App::new();
        app.init_resource::<CommandBarProjection>()
            .add_systems(Update, update_terminals_snapshot);
        app.update();
        let snap = &app.world().resource::<CommandBarProjection>().terminals;
        assert_eq!(snap.terminal_page_url, TERMINAL_PAGE_URL);
        assert!(snap.running.is_empty());
    }

    #[test]
    fn running_terminals_are_keyed_by_the_url_the_row_carries() {
        let mut app = App::new();
        app.init_resource::<CommandBarProjection>()
            .add_systems(Update, update_terminals_snapshot);
        let pane = app.world_mut().spawn_empty().id();
        app.world_mut()
            .insert_resource([(4321, pane)].into_iter().collect::<PidToEntity>());

        app.update();

        let snap = &app.world().resource::<CommandBarProjection>().terminals;
        assert_eq!(snap.running.get("vmux://terminal/4321"), Some(&pane));
    }
}
