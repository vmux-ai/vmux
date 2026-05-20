use bevy::prelude::*;
use vmux_command::snapshot::{CommandBarSpacesSnapshot, SpaceSummary};

use crate::event::SPACES_PAGE_URL;
use crate::spaces::{ActiveSpace, registry_space_summaries};

pub fn update_spaces_snapshot(
    active: Res<ActiveSpace>,
    mut snapshot: ResMut<CommandBarSpacesSnapshot>,
) {
    let active_changed = active.is_changed() || active.is_added();
    if !active_changed && !snapshot.spaces_page_url.is_empty() {
        return;
    }

    snapshot.spaces = registry_space_summaries()
        .into_iter()
        .map(|(id, name, profile)| SpaceSummary { id, name, profile })
        .collect();
    snapshot.active_space_id = active.record.id.clone();
    snapshot.active_space_name = active.record.name.clone();
    snapshot.spaces_page_url = SPACES_PAGE_URL.to_string();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_active_name_and_url() {
        let mut app = App::new();
        app.init_resource::<CommandBarSpacesSnapshot>();
        app.init_resource::<ActiveSpace>();
        app.add_systems(Update, update_spaces_snapshot);
        app.update();
        let snap = app.world().resource::<CommandBarSpacesSnapshot>();
        assert_eq!(snap.spaces_page_url, SPACES_PAGE_URL);
        assert!(!snap.active_space_id.is_empty());
    }
}
