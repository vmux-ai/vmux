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
                    profile: crate::model::BOOTSTRAP_PROFILE_NAME.to_string(),
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
mod tests {
    use super::*;

    #[test]
    fn writes_active_name_and_url() {
        let mut app = App::new();
        app.init_resource::<CommandBarSpacesSnapshot>()
            .insert_resource(ActiveSpaceId(Some("space-1".to_string())))
            .add_systems(Update, update_spaces_snapshot);
        app.world_mut().spawn((
            Space,
            SpaceId("space-1".to_string()),
            Name::new("Space 1"),
            vmux_core::Active,
        ));
        app.update();
        let snap = app.world().resource::<CommandBarSpacesSnapshot>();
        assert_eq!(snap.spaces_page_url, SPACES_PAGE_URL);
        assert_eq!(snap.active_space_id, "space-1");
        assert_eq!(snap.active_space_name, "Space 1");
        assert_eq!(snap.spaces.len(), 1);
    }
}
