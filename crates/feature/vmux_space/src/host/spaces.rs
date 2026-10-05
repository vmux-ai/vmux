use bevy::prelude::*;

use crate::model::SpaceRecord;

impl SpaceRecord {
    pub fn bundle(&self) -> impl Bundle {
        (
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId(self.id.clone()),
            vmux_layout::profile::Profile {
                name: self.profile.clone(),
            },
            Name::new(self.name.clone()),
        )
    }
}

#[derive(Component, Default)]
#[require(super::SpacesUiStateUpdates, SpaceSelection, SpacesPageSnapshot)]
pub struct Spaces;

#[derive(Component, Default)]
pub(crate) struct SpaceSelection(pub(crate) usize);

#[derive(Component, Default)]
pub(crate) struct SpacesPageSnapshot(pub(crate) vmux_api::space::SpacesListEvent);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BOOTSTRAP_SPACE_ID, BOOTSTRAP_SPACE_NAME};

    #[test]
    fn space_profile_bundle_spawns_space_name_profile_and_id() {
        let mut app = App::new();
        app.world_mut().spawn(SpaceRecord::bootstrap().bundle());

        let mut query = app.world_mut().query_filtered::<(
            &Name,
            &vmux_layout::profile::Profile,
            &vmux_layout::space::SpaceId,
        ), With<vmux_layout::space::Space>>();
        let (name, profile, space_id) = query.single(app.world()).unwrap();

        assert_eq!(name.as_str(), BOOTSTRAP_SPACE_NAME);
        assert_eq!(profile.name, "Personal");
        assert_eq!(space_id.0, BOOTSTRAP_SPACE_ID);
    }
}
