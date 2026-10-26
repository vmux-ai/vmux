use bevy::prelude::*;

use crate::model::SpaceRecord;

impl SpaceRecord {
    pub fn bundle(&self) -> impl Bundle {
        (
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId(self.id.clone()),
            Name::new(self.name.clone()),
        )
    }

    pub fn profile_bundle(&self, owner: Entity) -> impl Bundle {
        vmux_layout::profile::Profile::owned_by(self.profile.clone(), owner)
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
    use crate::model::BOOTSTRAP_SPACE_ID;

    #[test]
    fn space_profile_bundle_spawns_space_name_profile_and_id() {
        let mut app = App::new();
        let record = SpaceRecord::bootstrap();
        let space = app.world_mut().spawn(record.bundle()).id();
        app.world_mut().spawn(record.profile_bundle(space));

        let mut query = app
            .world_mut()
            .query_filtered::<(&Name, &vmux_layout::space::SpaceId), With<vmux_layout::space::Space>>();
        let (name, space_id) = query.single(app.world()).unwrap();
        let profile = app
            .world()
            .get::<Children>(space)
            .unwrap()
            .iter()
            .find_map(|child| {
                app.world()
                    .get::<vmux_layout::profile::Profile>(child)
                    .and_then(|_| app.world().get::<Name>(child))
            })
            .unwrap();

        assert!(SpaceRecord::is_planet_name(name.as_str()));
        assert_eq!(profile.as_str(), "Personal");
        assert_eq!(space_id.0, BOOTSTRAP_SPACE_ID);
    }
}
