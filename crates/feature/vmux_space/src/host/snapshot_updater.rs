use bevy::prelude::*;
use bevy_cef::prelude::HostWindow;
use vmux_command::snapshot::{CommandBarProjection, CommandBarSpacesSnapshot, SpaceSummary};
use vmux_core::Order;
use vmux_layout::space::{Space, SpaceId};

use crate::event::SPACES_PAGE_URL;

pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update_spaces_snapshot.in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
        );
    }
}

fn update_spaces_snapshot(
    spaces: Query<
        (
            Entity,
            &SpaceId,
            &Name,
            Has<vmux_core::Active>,
            Option<&Order>,
        ),
        With<Space>,
    >,
    focused_window: Res<vmux_layout::window::FocusedWindow>,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
    mut state: ResMut<CommandBarProjection>,
) {
    let profile = crate::model::bootstrap_profile_name();
    let mut rows: Vec<(u32, SpaceSummary)> = Vec::new();
    let mut active_space_id = String::new();
    let mut active_space_name = String::new();
    for (entity, id, name, is_active, order) in &spaces {
        let local = vmux_layout::window::host_window_of(entity, &child_of, &host_windows)
            == focused_window.0;
        if local && is_active {
            active_space_id.clone_from(&id.0);
            active_space_name = name.to_string();
        }
        let order = order.map(|order| order.0).unwrap_or(u32::MAX);
        if let Some((existing_order, _)) = rows.iter_mut().find(|(_, summary)| summary.id == id.0) {
            *existing_order = (*existing_order).min(order);
            continue;
        }
        rows.push((
            order,
            SpaceSummary {
                id: id.0.clone(),
                name: name.to_string(),
                profile: profile.clone(),
            },
        ));
    }
    rows.sort_by_key(|(order, _)| *order);

    let next = CommandBarSpacesSnapshot {
        spaces: rows.into_iter().map(|(_, summary)| summary).collect(),
        active_space_id,
        active_space_name,
        spaces_page_url: SPACES_PAGE_URL.to_string(),
    };
    if state.spaces != next {
        state.spaces = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Spaces {
        app: App,
        published_at: u32,
    }

    impl Spaces {
        fn one() -> Self {
            let mut app = App::new();
            app.init_resource::<CommandBarProjection>()
                .add_systems(Update, update_spaces_snapshot);
            let window = app.world_mut().spawn_empty().id();
            app.insert_resource(vmux_layout::window::FocusedWindow(Some(window)));
            let root = app.world_mut().spawn(HostWindow(window)).id();
            let main = app.world_mut().spawn(ChildOf(root)).id();
            app.world_mut().spawn((
                Space,
                SpaceId("space-1".to_string()),
                Name::new("Space 1"),
                vmux_core::Active,
                ChildOf(main),
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
                .get_resource_change_ticks::<CommandBarProjection>()
                .expect("the snapshot")
                .changed
                .get()
        }

        fn snapshot(&self) -> &CommandBarSpacesSnapshot {
            &self.app.world().resource::<CommandBarProjection>().spaces
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
        let mut spaces = Spaces::one();
        spaces.republished();
        let snap = spaces.snapshot();

        assert_eq!(snap.spaces_page_url, SPACES_PAGE_URL);
        assert_eq!(snap.active_space_id, "space-1");
        assert_eq!(snap.active_space_name, "Space 1");
        assert_eq!(snap.spaces.len(), 1);
    }

    #[test]
    fn an_unchanged_space_list_is_not_republished() {
        let mut spaces = Spaces::one();
        assert!(spaces.republished(), "the first list has to reach the bar");

        assert!(
            !spaces.republished(),
            "nothing changed, so nothing should have been published"
        );
    }

    #[test]
    fn a_renamed_space_is_republished() {
        let mut spaces = Spaces::one();
        spaces.republished();
        spaces.rename("Renamed");

        assert!(spaces.republished(), "a rename has to reach the bar");
        assert_eq!(spaces.snapshot().active_space_name, "Renamed");
    }

    #[test]
    fn publishes_global_spaces_with_the_focused_windows_active_space() {
        let mut app = App::new();
        app.init_resource::<CommandBarProjection>()
            .add_systems(Update, update_spaces_snapshot);
        let first_window = app.world_mut().spawn_empty().id();
        let second_window = app.world_mut().spawn_empty().id();
        app.insert_resource(vmux_layout::window::FocusedWindow(Some(first_window)));
        for (window, id) in [(first_window, "first"), (second_window, "second")] {
            let root = app.world_mut().spawn(HostWindow(window)).id();
            let main = app.world_mut().spawn(ChildOf(root)).id();
            app.world_mut().spawn((
                Space,
                SpaceId(id.to_string()),
                Name::new(id.to_string()),
                vmux_core::Active,
                ChildOf(main),
            ));
        }

        app.update();

        let snapshot = &app.world().resource::<CommandBarProjection>().spaces;
        assert_eq!(snapshot.active_space_id, "first");
        assert_eq!(snapshot.spaces.len(), 2);
        assert_eq!(snapshot.spaces[0].id, "first");
        assert_eq!(snapshot.spaces[1].id, "second");

        app.world_mut()
            .resource_mut::<vmux_layout::window::FocusedWindow>()
            .0 = Some(second_window);
        app.update();

        let snapshot = &app.world().resource::<CommandBarProjection>().spaces;
        assert_eq!(snapshot.active_space_id, "second");
        assert_eq!(snapshot.spaces.len(), 2);
    }
}
