use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};

pub struct RenamePlugin;

impl Plugin for RenamePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Icon>().add_observer(apply);
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct Icon(pub String);

impl Icon {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(EntityEvent, Clone, Debug, PartialEq, Eq)]
pub struct RenameRequest {
    #[event_target]
    pub entity: Entity,
    pub name: String,
}

impl RenameRequest {
    pub fn new(entity: Entity, name: impl Into<String>) -> Self {
        Self {
            entity,
            name: name.into(),
        }
    }

    pub fn normalize(raw: &str) -> Option<&str> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.contains(['\0', '\n', '\r']) {
            return None;
        }
        Some(trimmed)
    }
}

#[derive(EntityEvent, Clone, Debug, PartialEq, Eq)]
pub struct Renamed {
    #[event_target]
    pub entity: Entity,
    pub previous: Option<String>,
    pub name: String,
}

fn apply(trigger: On<RenameRequest>, entities: Query<Option<&Name>>, mut commands: Commands) {
    let request = trigger.event();
    let Some(trimmed) = RenameRequest::normalize(&request.name) else {
        return;
    };
    let Ok(current) = entities.get(request.entity) else {
        return;
    };
    let previous = current.map(|name| name.as_str().to_string());
    if previous.as_deref() == Some(trimmed) {
        return;
    }
    commands
        .entity(request.entity)
        .insert(Name::new(trimmed.to_string()));
    commands.trigger(Renamed {
        entity: request.entity,
        previous,
        name: trimmed.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Observed(Vec<(Entity, Option<String>, String)>);

    fn record(trigger: On<Renamed>, mut observed: ResMut<Observed>) {
        let event = trigger.event();
        observed
            .0
            .push((event.entity, event.previous.clone(), event.name.clone()));
    }

    #[test]
    fn rename_trims_valid_names_and_emits_notification() {
        let mut app = App::new();
        app.add_plugins(RenamePlugin)
            .init_resource::<Observed>()
            .add_observer(record);
        let entity = app.world_mut().spawn(Name::new("Original")).id();

        app.world_mut()
            .trigger(RenameRequest::new(entity, "  Updated Name  "));
        app.world_mut().flush();

        assert_eq!(
            app.world().get::<Name>(entity).map(Name::as_str),
            Some("Updated Name")
        );
        assert_eq!(
            app.world().resource::<Observed>().0,
            vec![(entity, Some("Original".into()), "Updated Name".into())]
        );
    }

    #[test]
    fn rename_rejects_blank_control_noop_and_missing_targets() {
        let mut app = App::new();
        app.add_plugins(RenamePlugin)
            .init_resource::<Observed>()
            .add_observer(record);
        let entity = app.world_mut().spawn(Name::new("Stable")).id();
        let removed = app.world_mut().spawn_empty().id();
        app.world_mut().despawn(removed);

        for invalid in ["", "   \t ", "line1\nline2", "null\0byte", "  Stable  "] {
            app.world_mut().trigger(RenameRequest::new(entity, invalid));
        }
        app.world_mut()
            .trigger(RenameRequest::new(removed, "Ghost"));
        app.world_mut().flush();

        assert_eq!(
            app.world().get::<Name>(entity).map(Name::as_str),
            Some("Stable")
        );
        assert!(app.world().resource::<Observed>().0.is_empty());
    }

    #[test]
    fn rename_attaches_name_when_previously_absent() {
        let mut app = App::new();
        app.add_plugins(RenamePlugin)
            .init_resource::<Observed>()
            .add_observer(record);
        let entity = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(RenameRequest::new(entity, "First"));
        app.world_mut().flush();

        assert_eq!(
            app.world().get::<Name>(entity).map(Name::as_str),
            Some("First")
        );
        assert_eq!(
            app.world().resource::<Observed>().0,
            vec![(entity, None, "First".into())]
        );
    }
}
