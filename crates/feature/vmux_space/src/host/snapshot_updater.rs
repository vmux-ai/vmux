use bevy::prelude::*;
#[cfg(test)]
use bevy_cef::prelude::HostWindow;
use vmux_command::CommandBarContextSnapshot;
use vmux_layout::space::Space;
#[cfg(test)]
use vmux_layout::space::SpaceId;

pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(
            Update,
            update.in_set(vmux_command::WriteCommandBarSnapshots),
        );
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Command bar context"),
        CommandBarContextSnapshot::default(),
    ));
}

fn update(
    spaces: Query<(Entity, &Name, Has<vmux_ecs::Active>), With<Space>>,
    focused_window: vmux_layout::window::FocusedWindow,
    hierarchy: vmux_layout::window::WindowHierarchy,
    mut state: Single<&mut CommandBarContextSnapshot>,
) {
    let mut label = String::new();
    for (entity, name, is_active) in &spaces {
        let local = hierarchy.get(entity) == focused_window.entity();
        if local && is_active {
            label = name.to_string();
            break;
        }
    }
    let next = CommandBarContextSnapshot { label };
    if **state != next {
        **state = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Spaces {
        app: App,
        published: CommandBarContextSnapshot,
    }

    impl Spaces {
        fn one() -> Self {
            let mut app = App::new();
            app.add_systems(Update, update);
            app.world_mut().spawn(CommandBarContextSnapshot::default());
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
                published: CommandBarContextSnapshot::default(),
            }
        }

        fn republished(&mut self) -> bool {
            self.app.update();
            let current = self.snapshot().clone();
            let changed = current != self.published;
            self.published = current;
            changed
        }

        fn snapshot(&self) -> &CommandBarContextSnapshot {
            self.app
                .world()
                .iter_entities()
                .find_map(|entity| entity.get::<CommandBarContextSnapshot>())
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

        assert_eq!(snap.label, "Space 1");
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
        assert_eq!(spaces.snapshot().label, "Renamed");
    }

    #[test]
    fn publishes_global_spaces_with_the_focused_windows_active_space() {
        let mut app = App::new();
        app.add_systems(Update, update);
        app.world_mut().spawn(CommandBarContextSnapshot::default());
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
            .find_map(|entity| entity.get::<CommandBarContextSnapshot>())
            .unwrap();
        assert_eq!(snapshot.label, "first");

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
            .find_map(|entity| entity.get::<CommandBarContextSnapshot>())
            .unwrap();
        assert_eq!(snapshot.label, "second");
    }
}
