use bevy::prelude::*;
use vmux_command::snapshot::CommandBarTerminalsSnapshot;
use vmux_layout::event::TERMINAL_PAGE_URL;

use crate::pid::PidToEntity;

pub fn update_terminals_snapshot(
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
    snapshot.pid_to_entity = pid_map.as_deref().map(|m| m.0.clone()).unwrap_or_default();
    snapshot.terminal_page_url = TERMINAL_PAGE_URL.to_string();
}

#[cfg(test)]
#[path = "snapshot_updater.test.rs"]
mod tests;
