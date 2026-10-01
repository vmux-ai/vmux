use bevy::prelude::*;
use moonshine_save::prelude::*;
use vmux_core::host::persistence::PersistenceAppExt;
use vmux_core::{Active, EffectiveStartupUrl};
use vmux_flex::prelude::*;

impl Plugin for SpaceLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::active::ActivePlugin)
            .register_persisted::<Space>()
            .register_persisted::<SpaceId>()
            .add_systems(
                Update,
                (
                    bevy::ecs::schedule::ApplyDeferred,
                    sync_current.in_set(CurrentSpaceSet),
                )
                    .chain()
                    .after(crate::active::ActiveSystemSet::Space),
            )
            .add_systems(
                PostUpdate,
                sync_container_visibility.before(LayoutSystems::Layout),
            );
    }
}

pub struct SpaceLayoutPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CurrentSpaceSet;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space"]
#[require(
    Save,
    SpaceId,
    EffectiveStartupDir,
    EffectiveStartupUrl,
    crate::profile::Profile
)]
pub struct Space;

impl Space {
    pub fn bundle() -> impl Bundle {
        (
            Self,
            Self::container_node(),
            Transform::default(),
            Visibility::default(),
        )
    }

    fn container_node() -> Node {
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
}

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space"]
#[require(Save)]
pub struct SpaceId(pub String);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CurrentSpace;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectiveStartupDir(pub Option<std::path::PathBuf>);

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct EffectiveStartupSet;

#[derive(bevy::ecs::system::SystemParam)]
pub struct FocusedSpace<'w, 's> {
    spaces: Query<
        'w,
        's,
        (
            Entity,
            &'static SpaceId,
            &'static EffectiveStartupDir,
            &'static EffectiveStartupUrl,
            &'static crate::profile::Profile,
            Has<CurrentSpace>,
            Has<Active>,
        ),
        With<Space>,
    >,
}

impl FocusedSpace<'_, '_> {
    fn selected(
        &self,
    ) -> Option<(
        Entity,
        &SpaceId,
        &EffectiveStartupDir,
        &EffectiveStartupUrl,
        &crate::profile::Profile,
    )> {
        self.spaces
            .iter()
            .find(|(_, _, _, _, _, current, _)| *current)
            .or_else(|| {
                self.spaces
                    .iter()
                    .find(|(_, _, _, _, _, _, active)| *active)
            })
            .or_else(|| self.spaces.iter().next())
            .map(|(entity, id, startup_dir, startup_url, profile, _, _)| {
                (entity, id, startup_dir, startup_url, profile)
            })
    }

    pub fn get(&self) -> Option<(Entity, Option<std::path::PathBuf>)> {
        self.selected()
            .map(|(entity, _, startup_dir, _, _)| (entity, startup_dir.0.clone()))
    }

    pub fn entity(&self) -> Option<Entity> {
        self.selected().map(|(entity, _, _, _, _)| entity)
    }

    pub fn id(&self) -> Option<&str> {
        self.selected().map(|(_, id, _, _, _)| id.0.as_str())
    }

    pub fn profile(&self) -> Option<&str> {
        self.selected()
            .map(|(_, _, _, _, profile)| profile.name.as_str())
    }

    pub fn startup_dir(&self) -> Option<&std::path::Path> {
        self.selected()
            .and_then(|(_, _, startup_dir, _, _)| startup_dir.0.as_deref())
    }

    pub fn startup_url(&self) -> Option<&str> {
        self.selected()
            .map(|(_, _, _, startup_url, _)| startup_url.0.as_str())
            .filter(|url| !url.is_empty())
    }

    pub fn resolved_startup_url(&self) -> String {
        self.startup_url().unwrap_or_default().to_string()
    }
}

fn sync_current(
    spaces: Query<(Entity, Has<Active>, Has<CurrentSpace>), With<Space>>,
    focused_window: crate::window::FocusedWindow,
    hierarchy: crate::window::WindowHierarchy,
    mut commands: Commands,
) {
    let current = focused_window
        .entity()
        .and_then(|focused| {
            spaces
                .iter()
                .filter(|(_, active, _)| *active)
                .map(|(entity, _, _)| entity)
                .find(|space| hierarchy.get(*space) == Some(focused))
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
    hierarchy: SpaceHierarchy<'w, 's>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct SpaceHierarchy<'w, 's> {
    child_of: Query<'w, 's, &'static ChildOf>,
    spaces: Query<'w, 's, (), With<Space>>,
    ids: Query<'w, 's, &'static SpaceId>,
}

impl SpaceOfPane<'_, '_> {
    pub fn resolve(&self, pane_id: &str) -> Option<Entity> {
        let bits = pane_id.parse::<u64>().ok()?;
        let pane = self.leaf_panes.iter().find(|pane| pane.to_bits() == bits)?;
        self.hierarchy.get(pane)
    }
}

impl SpaceHierarchy<'_, '_> {
    pub fn get(&self, entity: Entity) -> Option<Entity> {
        let mut current = entity;
        loop {
            if self.spaces.get(current).is_ok() {
                return Some(current);
            }
            match self.child_of.get(current) {
                Ok(parent) => current = parent.parent(),
                Err(_) => return None,
            }
        }
    }

    pub fn id(&self, entity: Entity) -> Option<String> {
        let space = self.get(entity)?;
        self.ids.get(space).ok().map(|id| id.0.clone())
    }
}

fn sync_container_visibility(
    mut spaces: Query<(&mut Node, &mut Visibility, Has<Active>), With<Space>>,
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
        app.add_systems(Update, sync_current);
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
        app.add_systems(Update, sync_current);
        let space = app.world_mut().spawn((Space, CurrentSpace)).id();
        app.update();
        assert!(app.world().get::<CurrentSpace>(space).is_none());
    }

    #[test]
    fn current_space_retains_its_typed_id() {
        let mut app = App::new();
        app.add_systems(Update, sync_current);
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
            .run_system_once(move |hierarchy: SpaceHierarchy| hierarchy.get(stack))
            .unwrap();
        assert_eq!(found, Some(space));
    }

    #[test]
    fn space_container_bundle_is_absolute_fill_node() {
        assert_eq!(
            Space::container_node().position_type,
            PositionType::Absolute
        );
    }

    #[test]
    fn inactive_space_container_is_hidden_but_alive() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, sync_container_visibility);
        let active = app
            .world_mut()
            .spawn((
                Space,
                vmux_core::Active,
                Space::container_node(),
                Visibility::default(),
            ))
            .id();
        let bg = app.world_mut().spawn(Space::bundle()).id();
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
