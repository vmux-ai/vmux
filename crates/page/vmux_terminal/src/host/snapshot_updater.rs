use bevy::prelude::*;
use std::collections::HashMap;
use vmux_command::snapshot::CommandBarTerminalsSnapshot;
use vmux_layout::event::TERMINAL_PAGE_URL;

use crate::pid::{Pid, PidToEntity};

/// Publishes the terminal entries the command bar searches over.
pub struct TerminalSnapshotPlugin;

impl Plugin for TerminalSnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update_terminals_snapshot.in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
        );
    }
}

fn update_terminals_snapshot(
    pid_map: Option<Res<PidToEntity>>,
    mut snapshot: ResMut<CommandBarTerminalsSnapshot>,
) {
    let changed = pid_map
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    if !changed && !snapshot.terminal_page_url.is_empty() {
        return;
    }
    let mut running = HashMap::new();
    if let Some(pid_map) = pid_map.as_deref() {
        for (pid, entity) in &pid_map.0 {
            running.insert(Pid(*pid).page_url(), *entity);
        }
    }
    snapshot.running = running;
    snapshot.terminal_page_url = TERMINAL_PAGE_URL.to_string();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_url_and_no_running_terminals() {
        let mut app = App::new();
        app.init_resource::<CommandBarTerminalsSnapshot>()
            .add_systems(Update, update_terminals_snapshot);
        app.update();
        let snap = app.world().resource::<CommandBarTerminalsSnapshot>();
        assert_eq!(snap.terminal_page_url, TERMINAL_PAGE_URL);
        assert!(snap.running.is_empty());
    }

    /// The key has to be the url the command bar hands back, spelt out here rather than built with
    /// [`Pid::page_url`], because agreeing with the code under test proves nothing. A key that
    /// drifts from the pane's own `PageMetadata` silently spawns a second terminal.
    #[test]
    fn running_terminals_are_keyed_by_the_url_the_row_carries() {
        let mut app = App::new();
        app.init_resource::<CommandBarTerminalsSnapshot>()
            .add_systems(Update, update_terminals_snapshot);
        let pane = app.world_mut().spawn_empty().id();
        app.world_mut()
            .insert_resource(PidToEntity(HashMap::from([(4321, pane)])));

        app.update();

        let snap = app.world().resource::<CommandBarTerminalsSnapshot>();
        assert_eq!(snap.running.get("vmux://terminal/4321"), Some(&pane));
    }
}
