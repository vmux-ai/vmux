use bevy::prelude::*;
use vmux_layout::native_open::HostedPage;

use crate::event::SPACES_PAGE_URL;
use crate::model::SpaceRecord;

#[derive(Resource, Clone, Debug, Default)]
pub struct ActiveSpace {
    pub record: SpaceRecord,
}

pub fn space_profile_bundle(record: &SpaceRecord) -> impl Bundle {
    (
        vmux_layout::space::Space,
        vmux_layout::space::SpaceId(record.id.clone()),
        vmux_layout::profile::Profile {
            name: record.profile.clone(),
        },
        Name::new(record.name.clone()),
    )
}

#[derive(Component, Default)]
pub struct Spaces;

impl Spaces {}

impl HostedPage for Spaces {
    const HOST: &'static str = "spaces";
    const URL: &'static str = SPACES_PAGE_URL;
    const TITLE: &'static str = "Spaces";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        BOOTSTRAP_SPACE_ID, BOOTSTRAP_SPACE_NAME, bootstrap_profile_name, bootstrap_space_record,
    };

    #[test]
    fn space_profile_bundle_spawns_space_name_profile_and_id() {
        let mut app = App::new();
        app.world_mut()
            .spawn(space_profile_bundle(&bootstrap_space_record()));

        let mut query = app.world_mut().query_filtered::<(
            &Name,
            &vmux_layout::profile::Profile,
            &vmux_layout::space::SpaceId,
        ), With<vmux_layout::space::Space>>();
        let (name, profile, space_id) = query.single(app.world()).unwrap();

        assert_eq!(name.as_str(), BOOTSTRAP_SPACE_NAME);
        assert_eq!(profile.name, bootstrap_profile_name());
        assert_eq!(space_id.0, BOOTSTRAP_SPACE_ID);
    }
}
