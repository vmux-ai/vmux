use bevy::prelude::*;
use bevy_cef::prelude::HostWindow;
use moonshine_save::prelude::*;
use vmux_flex::prelude::*;

use super::command::LayoutRequestSet;

impl Plugin for SpaceLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Space>()
            .register_type::<SpaceId>()
            .add_systems(
                Update,
                (
                    crate::active::ensure_active_space,
                    bevy::ecs::schedule::ApplyDeferred,
                    sync_current_space.in_set(CurrentSpaceSet),
                )
                    .chain()
                    .after(crate::window::WindowFocusSet),
            )
            .add_systems(
                Update,
                (
                    crate::active::ensure_active_tab,
                    crate::active::ensure_active_stack,
                    crate::active::ensure_active_branch,
                )
                    .after(LayoutRequestSet::Handle)
                    .after(crate::window::spawn_requested_tab_layouts),
            )
            .add_systems(
                PostUpdate,
                sync_space_container_visibility.before(LayoutSystems::Layout),
            );
    }
}

pub struct SpaceLayoutPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CurrentSpaceSet;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space"]
#[require(Save)]
pub struct Space;

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space"]
#[require(Save)]
pub struct SpaceId(pub String);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CurrentSpace;

fn sync_current_space(
    spaces: Query<(Entity, Has<vmux_core::Active>, Has<CurrentSpace>), With<Space>>,
    focused_window: Res<crate::window::FocusedWindow>,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
    mut commands: Commands,
) {
    let current = focused_window
        .0
        .and_then(|focused| {
            spaces
                .iter()
                .filter(|(_, active, _)| *active)
                .map(|(entity, _, _)| entity)
                .find(|space| {
                    crate::window::host_window_of(*space, &child_of, &host_windows) == Some(focused)
                })
        })
        .or_else(|| {
            spaces
                .iter()
                .find(|(_, active, _)| *active)
                .map(|(entity, _, _)| entity)
        });
    for (entity, _, selected) in &spaces {
        if current == Some(entity) && !selected {
            commands.entity(entity).insert(CurrentSpace);
        } else if current != Some(entity) && selected {
            commands.entity(entity).remove::<CurrentSpace>();
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct SpaceOfPane<'w, 's> {
    leaf_panes: Query<'w, 's, Entity, (With<crate::pane::Pane>, Without<crate::pane::PaneSplit>)>,
    child_of: Query<'w, 's, &'static ChildOf>,
    spaces: Query<'w, 's, (), With<Space>>,
}

impl SpaceOfPane<'_, '_> {
    pub fn resolve(&self, pane_id: &str) -> Option<Entity> {
        let bits = pane_id.parse::<u64>().ok()?;
        let pane = self.leaf_panes.iter().find(|pane| pane.to_bits() == bits)?;
        space_of(pane, &self.child_of, &self.spaces)
    }
}

pub fn space_of(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    spaces: &Query<(), With<Space>>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if spaces.get(current).is_ok() {
            return Some(current);
        }
        match child_of.get(current) {
            Ok(parent) => current = parent.parent(),
            Err(_) => return None,
        }
    }
}

pub fn space_id_of(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    spaces: &Query<(), With<Space>>,
    ids: &Query<&SpaceId>,
) -> Option<String> {
    let space = space_of(entity, child_of, spaces)?;
    ids.get(space).ok().map(|id| id.0.clone())
}

pub fn space_container_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        top: Val::Px(0.0),
        bottom: Val::Px(0.0),
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        ..default()
    }
}

pub fn space_view_bundle() -> impl Bundle {
    (
        space_container_node(),
        Transform::default(),
        Visibility::default(),
    )
}

pub fn sync_space_container_visibility(
    mut spaces: Query<(&mut Node, &mut Visibility, Has<vmux_core::Active>), With<Space>>,
) {
    for (mut node, mut vis, active) in &mut spaces {
        let target_display = if active { Display::Flex } else { Display::None };
        if node.display != target_display {
            node.display = target_display;
        }
        let target_vis = if active {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *vis != target_vis {
            *vis = target_vis;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_space_tracks_active_space() {
        let mut app = App::new();
        app.init_resource::<crate::window::FocusedWindow>()
            .add_systems(Update, sync_current_space);
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("default".to_string()), vmux_core::Active))
            .id();
        app.update();
        assert!(app.world().get::<CurrentSpace>(space).is_some());
    }

    #[test]
    fn current_space_clears_when_no_space_is_active() {
        let mut app = App::new();
        app.init_resource::<crate::window::FocusedWindow>()
            .add_systems(Update, sync_current_space);
        let space = app.world_mut().spawn((Space, CurrentSpace)).id();
        app.update();
        assert!(app.world().get::<CurrentSpace>(space).is_none());
    }

    #[test]
    fn current_space_retains_its_typed_id() {
        let mut app = App::new();
        app.init_resource::<crate::window::FocusedWindow>()
            .add_systems(Update, sync_current_space);
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("work".to_string()), vmux_core::Active))
            .id();
        app.update();
        assert_eq!(
            app.world()
                .get::<SpaceId>(space)
                .filter(|_| app.world().get::<CurrentSpace>(space).is_some())
                .map(|id| id.0.as_str()),
            Some("work")
        );
    }

    #[test]
    fn space_of_walks_up_to_nearest_space() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s".to_string())))
            .id();
        let tab = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), ChildOf(space)))
            .id();
        let stack = app.world_mut().spawn(ChildOf(tab)).id();
        let found = app
            .world_mut()
            .run_system_once(
                move |child_of: Query<&ChildOf>, spaces: Query<(), With<Space>>| {
                    space_of(stack, &child_of, &spaces)
                },
            )
            .unwrap();
        assert_eq!(found, Some(space));
    }

    #[test]
    fn space_container_bundle_is_absolute_fill_node() {
        assert_eq!(space_container_node().position_type, PositionType::Absolute);
    }

    #[test]
    fn inactive_space_container_is_hidden_but_alive() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, sync_space_container_visibility);
        let active = app
            .world_mut()
            .spawn((
                Space,
                vmux_core::Active,
                space_container_node(),
                Visibility::default(),
            ))
            .id();
        let bg = app
            .world_mut()
            .spawn((Space, space_container_node(), Visibility::default()))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Node>(active).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            *app.world().get::<Visibility>(active).unwrap(),
            Visibility::Visible
        );
        assert_eq!(app.world().get::<Node>(bg).unwrap().display, Display::None);
        assert_eq!(
            *app.world().get::<Visibility>(bg).unwrap(),
            Visibility::Hidden
        );
        assert!(app.world().get_entity(bg).is_ok());
    }
}
