use bevy::prelude::*;
use vmux_command::snapshot::{CommandBarSpacesSnapshot, SpaceSummary};
use vmux_core::Order;
use vmux_layout::space::{ActiveSpaceId, Space, SpaceId};

use crate::event::SPACES_PAGE_URL;

pub fn update_spaces_snapshot(
    spaces: Query<(&SpaceId, &Name, Option<&Order>), With<Space>>,
    active_id: Res<ActiveSpaceId>,
    active_name: Query<&Name, (With<Space>, With<vmux_core::Active>)>,
    mut snapshot: ResMut<CommandBarSpacesSnapshot>,
) {
    let mut rows: Vec<(u32, SpaceSummary)> = spaces
        .iter()
        .map(|(id, name, order)| {
            (
                order.map(|o| o.0).unwrap_or(u32::MAX),
                SpaceSummary {
                    id: id.0.clone(),
                    name: name.to_string(),
                    profile: crate::model::bootstrap_profile_name(),
                },
            )
        })
        .collect();
    rows.sort_by_key(|(order, _)| *order);

    snapshot.spaces = rows.into_iter().map(|(_, summary)| summary).collect();
    snapshot.active_space_id = active_id.0.clone().unwrap_or_default();
    snapshot.active_space_name = active_name
        .iter()
        .next()
        .map(|name| name.to_string())
        .unwrap_or_default();
    snapshot.spaces_page_url = SPACES_PAGE_URL.to_string();
}

#[cfg(test)]
#[path = "snapshot_updater.test.rs"]
mod tests;
