use bevy::prelude::*;
use moonshine_save::prelude::*;

pub struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Space>()
            .register_type::<SpaceId>()
            .register_type::<ActiveSpaceTag>()
            .init_resource::<ActiveSpaceEntity>()
            .init_resource::<ActiveSpaceId>()
            .add_systems(
                Update,
                (
                    ensure_active_space_tagged,
                    sync_active_space_entity,
                    sync_active_space_id,
                    assign_orphan_tabs_to_active_space,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    crate::active::ensure_active_tab,
                    crate::active::ensure_active_stack,
                    crate::active::ensure_active_branch,
                ),
            );
    }
}

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

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space"]
#[require(Save)]
pub struct ActiveSpaceTag;

#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct ActiveSpaceEntity(pub Option<Entity>);

#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct ActiveSpaceId(pub Option<String>);

pub fn sync_active_space_entity(
    tagged: Query<Entity, With<ActiveSpaceTag>>,
    mut active: ResMut<ActiveSpaceEntity>,
) {
    let current = tagged.iter().next();
    if active.0 != current {
        active.0 = current;
    }
}

pub fn ensure_active_space_tagged(
    tagged: Query<(), With<ActiveSpaceTag>>,
    spaces: Query<Entity, With<Space>>,
    mut commands: Commands,
) {
    if !tagged.is_empty() {
        return;
    }
    if let Some(entity) = spaces.iter().next() {
        commands.entity(entity).insert(ActiveSpaceTag);
    }
}

pub fn sync_active_space_id(
    active: Res<ActiveSpaceEntity>,
    ids: Query<&SpaceId>,
    mut active_id: ResMut<ActiveSpaceId>,
) {
    let current = active
        .0
        .and_then(|entity| ids.get(entity).ok())
        .map(|id| id.0.clone());
    if active_id.0 != current {
        active_id.0 = current;
    }
}

pub fn assign_orphan_tabs_to_active_space(
    active_id: Res<ActiveSpaceId>,
    orphans: Query<Entity, (With<crate::tab::Tab>, Without<SpaceId>)>,
    mut commands: Commands,
) {
    let Some(id) = active_id.0.as_deref() else {
        return;
    };
    for tab in &orphans {
        commands.entity(tab).insert(SpaceId(id.to_string()));
    }
}

pub fn same_space(candidate: Option<&SpaceId>, active: Option<&SpaceId>) -> bool {
    match (candidate, active) {
        (Some(candidate), Some(active)) => candidate == active,
        _ => true,
    }
}

/// Whether a tab/entity carrying `candidate` belongs to the active space id.
/// Unknown ids (no `SpaceId`, or no active space) are treated as in-scope so
/// callers degrade to global behavior instead of hiding everything.
pub fn in_active_space(candidate: Option<&SpaceId>, active: Option<&str>) -> bool {
    match (candidate, active) {
        (Some(candidate), Some(active)) => candidate.0 == active,
        _ => true,
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
        GlobalTransform::default(),
        Visibility::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_space_entity_tracks_tagged_space() {
        let mut app = App::new();
        app.init_resource::<ActiveSpaceEntity>()
            .add_systems(Update, sync_active_space_entity);
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("default".to_string()), ActiveSpaceTag))
            .id();
        app.update();
        assert_eq!(app.world().resource::<ActiveSpaceEntity>().0, Some(space));
    }

    #[test]
    fn active_space_entity_clears_when_no_tag() {
        let mut app = App::new();
        app.init_resource::<ActiveSpaceEntity>()
            .add_systems(Update, sync_active_space_entity);
        app.insert_resource(ActiveSpaceEntity(Some(Entity::from_bits(42))));
        app.update();
        assert_eq!(app.world().resource::<ActiveSpaceEntity>().0, None);
    }

    #[test]
    fn ensure_active_space_tagged_tags_sole_untagged_space() {
        let mut app = App::new();
        app.init_resource::<ActiveSpaceEntity>().add_systems(
            Update,
            (ensure_active_space_tagged, sync_active_space_entity).chain(),
        );
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("default".to_string())))
            .id();
        app.update();
        assert!(app.world().entity(space).contains::<ActiveSpaceTag>());
        assert_eq!(app.world().resource::<ActiveSpaceEntity>().0, Some(space));
    }

    #[test]
    fn active_space_id_tracks_active_entity() {
        let mut app = App::new();
        app.init_resource::<ActiveSpaceEntity>()
            .init_resource::<ActiveSpaceId>()
            .add_systems(
                Update,
                (sync_active_space_entity, sync_active_space_id).chain(),
            );
        app.world_mut()
            .spawn((Space, SpaceId("work".to_string()), ActiveSpaceTag));
        app.update();
        assert_eq!(
            app.world().resource::<ActiveSpaceId>().0.as_deref(),
            Some("work")
        );
    }

    #[test]
    fn orphan_tabs_get_active_space_id() {
        let mut app = App::new();
        app.insert_resource(ActiveSpaceId(Some("work".to_string())))
            .add_systems(Update, assign_orphan_tabs_to_active_space);
        let tab = app.world_mut().spawn(crate::tab::Tab::default()).id();
        app.update();
        assert_eq!(
            app.world().entity(tab).get::<SpaceId>(),
            Some(&SpaceId("work".to_string()))
        );
    }

    #[test]
    fn ensure_active_space_tagged_is_noop_when_already_tagged() {
        let mut app = App::new();
        app.add_systems(Update, ensure_active_space_tagged);
        let a = app
            .world_mut()
            .spawn((Space, SpaceId("a".to_string()), ActiveSpaceTag))
            .id();
        let b = app
            .world_mut()
            .spawn((Space, SpaceId("b".to_string())))
            .id();
        app.update();
        assert!(app.world().entity(a).contains::<ActiveSpaceTag>());
        assert!(!app.world().entity(b).contains::<ActiveSpaceTag>());
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
}
