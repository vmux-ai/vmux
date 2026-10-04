use bevy::prelude::*;
#[cfg(test)]
use bevy_cef::prelude::HostWindow;
use vmux_command::{CommandBarSpacesSnapshot, SpaceSummary};
use vmux_ecs::Order;
use vmux_layout::space::{Space, SpaceId};

use crate::host::SpacePlugin;
use crate::model::SpaceRecord;

pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(
            Update,
            update_spaces_snapshot.in_set(vmux_command::WriteCommandBarSnapshots),
        );
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Command bar spaces"),
        CommandBarSpacesSnapshot::default(),
    ));
}

fn update_spaces_snapshot(
    spaces: Query<
        (
            Entity,
            &SpaceId,
            &Name,
            Has<vmux_ecs::Active>,
            Option<&Order>,
        ),
        With<Space>,
    >,
    focused_window: vmux_layout::window::FocusedWindow,
    hierarchy: vmux_layout::window::WindowHierarchy,
    mut state: Single<&mut CommandBarSpacesSnapshot>,
) {
    let profile = SpaceRecord::current_profile_name();
    let mut rows: Vec<(u32, SpaceSummary)> = Vec::new();
    let mut active_space_id = String::new();
    let mut active_space_name = String::new();
    for (entity, id, name, is_active, order) in &spaces {
        let local = hierarchy.get(entity) == focused_window.entity();
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
        spaces_page_url: SpacePlugin::MANIFEST.url.to_string(),
    };
    if **state != next {
        **state = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Spaces {
        app: App,
        published: CommandBarSpacesSnapshot,
    }

    impl Spaces {
        fn one() -> Self {
            let mut app = App::new();
            app.add_systems(Update, update_spaces_snapshot);
            app.world_mut().spawn(CommandBarSpacesSnapshot::default());
            let window = app
                .world_mut()
                .spawn((Window::default(), vmux_ecs::Active))
                .id();
            let root = app.world_mut().spawn(HostWindow(window)).id();
            let main = app.world_mut().spawn(ChildOf(root)).id();
            app.world_mut().spawn((
                Space,
                SpaceId("space-1".to_string()),
                Name::new("Space 1"),
                vmux_ecs::Active,
                ChildOf(main),
            ));
            Self {
                app,
                published: CommandBarSpacesSnapshot::default(),
            }
        }

        fn republished(&mut self) -> bool {
            self.app.update();
            let current = self.snapshot().clone();
            let changed = current != self.published;
            self.published = current;
            changed
        }

        fn snapshot(&self) -> &CommandBarSpacesSnapshot {
            self.app
                .world()
                .iter_entities()
                .find_map(|entity| entity.get::<CommandBarSpacesSnapshot>())
                .unwrap()
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

        assert_eq!(snap.spaces_page_url, SpacePlugin::MANIFEST.url);
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
        app.add_systems(Update, update_spaces_snapshot);
        app.world_mut().spawn(CommandBarSpacesSnapshot::default());
        let first_window = app
            .world_mut()
            .spawn((Window::default(), vmux_ecs::Active))
            .id();
        let second_window = app.world_mut().spawn(Window::default()).id();
        for (window, id) in [(first_window, "first"), (second_window, "second")] {
            let root = app.world_mut().spawn(HostWindow(window)).id();
            let main = app.world_mut().spawn(ChildOf(root)).id();
            app.world_mut().spawn((
                Space,
                SpaceId(id.to_string()),
                Name::new(id.to_string()),
                vmux_ecs::Active,
                ChildOf(main),
            ));
        }

        app.update();

        let snapshot = app
            .world()
            .iter_entities()
            .find_map(|entity| entity.get::<CommandBarSpacesSnapshot>())
            .unwrap();
        assert_eq!(snapshot.active_space_id, "first");
        assert_eq!(snapshot.spaces.len(), 2);
        assert_eq!(snapshot.spaces[0].id, "first");
        assert_eq!(snapshot.spaces[1].id, "second");

        app.world_mut()
            .entity_mut(first_window)
            .remove::<vmux_ecs::Active>();
        app.world_mut()
            .entity_mut(second_window)
            .insert(vmux_ecs::Active);
        app.update();

        let snapshot = app
            .world()
            .iter_entities()
            .find_map(|entity| entity.get::<CommandBarSpacesSnapshot>())
            .unwrap();
        assert_eq!(snapshot.active_space_id, "second");
        assert_eq!(snapshot.spaces.len(), 2);
    }
}
