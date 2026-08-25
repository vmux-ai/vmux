use bevy::prelude::*;
use vmux_command::snapshot::{CommandBarSpacesSnapshot, SpaceSummary};
use vmux_core::Order;
use vmux_layout::space::{ActiveSpaceId, Space, SpaceId};

use crate::event::SPACES_PAGE_URL;

pub struct SpaceSnapshotPlugin;

impl Plugin for SpaceSnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update_spaces_snapshot.in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
        );
    }
}

fn update_spaces_snapshot(
    spaces: Query<(&SpaceId, &Name, Option<&Order>), With<Space>>,
    active_id: Res<ActiveSpaceId>,
    active_name: Query<&Name, (With<Space>, With<vmux_core::Active>)>,
    mut snapshot: ResMut<CommandBarSpacesSnapshot>,
) {
    let profile = crate::model::bootstrap_profile_name();
    let mut rows: Vec<(u32, SpaceSummary)> = Vec::new();
    for (id, name, order) in &spaces {
        rows.push((
            order.map(|o| o.0).unwrap_or(u32::MAX),
            SpaceSummary {
                id: id.0.clone(),
                name: name.to_string(),
                profile: profile.clone(),
            },
        ));
    }
    rows.sort_by_key(|(order, _)| *order);

    snapshot.set_if_neq(CommandBarSpacesSnapshot {
        spaces: rows.into_iter().map(|(_, summary)| summary).collect(),
        active_space_id: active_id.0.clone().unwrap_or_default(),
        active_space_name: active_name
            .iter()
            .next()
            .map(|name| name.to_string())
            .unwrap_or_default(),
        spaces_page_url: SPACES_PAGE_URL.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Spaces {
        app: App,
        published_at: u32,
    }

    impl Spaces {
        fn of_one() -> Self {
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
            let published_at = Self::changed_tick(&app);
            Self { app, published_at }
        }

        fn republished(&mut self) -> bool {
            self.app.update();
            let now = Self::changed_tick(&self.app);
            let moved = now != self.published_at;
            self.published_at = now;
            moved
        }

        fn changed_tick(app: &App) -> u32 {
            app.world()
                .get_resource_change_ticks::<CommandBarSpacesSnapshot>()
                .expect("the snapshot")
                .changed
                .get()
        }

        fn snapshot(&self) -> &CommandBarSpacesSnapshot {
            self.app.world().resource::<CommandBarSpacesSnapshot>()
        }

        fn rename(&mut self, to: &str) {
            let world = self.app.world_mut();
            let entity = world
                .query_filtered::<Entity, With<Space>>()
                .iter(world)
                .next()
                .expect("the space");
            world.entity_mut(entity).insert(Name::new(to.to_string()));
        }
    }

    #[test]
    fn writes_active_name_and_url() {
        let mut spaces = Spaces::of_one();
        spaces.republished();
        let snap = spaces.snapshot();

        assert_eq!(snap.spaces_page_url, SPACES_PAGE_URL);
        assert_eq!(snap.active_space_id, "space-1");
        assert_eq!(snap.active_space_name, "Space 1");
        assert_eq!(snap.spaces.len(), 1);
    }

    #[test]
    fn an_unchanged_space_list_is_not_republished() {
        let mut spaces = Spaces::of_one();
        assert!(spaces.republished(), "the first list has to reach the bar");

        assert!(
            !spaces.republished(),
            "nothing changed, so nothing should have been published"
        );
    }

    #[test]
    fn a_renamed_space_is_republished() {
        let mut spaces = Spaces::of_one();
        spaces.republished();
        spaces.rename("Renamed");

        assert!(spaces.republished(), "a rename has to reach the bar");
        assert_eq!(spaces.snapshot().active_space_name, "Renamed");
    }
}
