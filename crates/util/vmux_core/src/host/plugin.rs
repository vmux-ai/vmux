use bevy::prelude::*;

use crate::archive::{ArchivedPage, ArchivedPagePosition, ArchivedTabPage, PaneStep, SplitAxis};
use crate::component::{
    ActivateRequest, Active, Bookmark, BookmarkOrder, Collapsed, CreatedAt, Folder,
    LastActivatedAt, LastVisitedAt, Order, Pin, TransitionType, Url, Uuid, Visit, VisitCount,
    VisitedUrl,
};
use crate::icon::{BuiltinIcon, PageIcon};
use crate::{PageMetadata, SmartBookmarkFolder};

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PageMetadata>()
            .register_type::<PageIcon>()
            .register_type::<BuiltinIcon>()
            .register_type::<ArchivedPage>()
            .register_type::<ArchivedPagePosition>()
            .register_type::<ArchivedTabPage>()
            .register_type::<PaneStep>()
            .register_type::<SplitAxis>()
            .register_type::<Vec<PaneStep>>()
            .register_type::<CreatedAt>()
            .register_type::<LastActivatedAt>()
            .register_type::<Visit>()
            .register_type::<Url>()
            .register_type::<VisitCount>()
            .register_type::<LastVisitedAt>()
            .register_type::<VisitedUrl>()
            .register_type::<TransitionType>()
            .register_type::<Order>()
            .register_type::<Active>()
            .register_type::<BookmarkOrder>()
            .register_type::<Pin>()
            .register_type::<Bookmark>()
            .register_type::<Folder>()
            .register_type::<SmartBookmarkFolder>()
            .register_type::<Collapsed>()
            .register_type::<Uuid>()
            .register_type::<Children>()
            .register_type::<ChildOf>()
            .add_observer(activate)
            .add_plugins(crate::page::HostHistoryPlugin);
    }
}

fn activate(trigger: On<ActivateRequest>, child_of: Query<&ChildOf>, mut commands: Commands) {
    use bevy::ecs::relationship::Relationship;

    let activated_at = LastActivatedAt::now();
    let mut current = trigger.event_target();
    commands.entity(current).insert(activated_at);
    while let Ok(parent_rel) = child_of.get(current) {
        current = parent_rel.get();
        commands.entity(current).insert(activated_at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_new_history_components() {
        let mut app = App::new();
        app.add_plugins(CorePlugin);

        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(registry.get(std::any::TypeId::of::<Url>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<VisitCount>()).is_some());
        assert!(
            registry
                .get(std::any::TypeId::of::<LastVisitedAt>())
                .is_some()
        );
        assert!(registry.get(std::any::TypeId::of::<VisitedUrl>()).is_some());
        assert!(
            registry
                .get(std::any::TypeId::of::<TransitionType>())
                .is_some()
        );
    }

    #[test]
    fn registers_bookmark_components() {
        let mut app = App::new();
        app.add_plugins(CorePlugin);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(
            registry
                .get(std::any::TypeId::of::<BookmarkOrder>())
                .is_some()
        );
        assert!(registry.get(std::any::TypeId::of::<Pin>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Bookmark>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Folder>()).is_some());
        assert!(
            registry
                .get(std::any::TypeId::of::<SmartBookmarkFolder>())
                .is_some()
        );
        assert!(registry.get(std::any::TypeId::of::<Collapsed>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Uuid>()).is_some());
    }

    #[test]
    fn active_marker_is_registered_and_reflectable() {
        let mut app = App::new();
        app.add_plugins(CorePlugin);

        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(registry.get(std::any::TypeId::of::<Active>()).is_some());
    }

    #[test]
    fn activation_propagates_through_ancestors() {
        let mut app = App::new();
        app.add_plugins(CorePlugin);

        let root = app.world_mut().spawn(LastActivatedAt(1)).id();
        let child = app
            .world_mut()
            .spawn((LastActivatedAt(1), ChildOf(root)))
            .id();
        let leaf = app
            .world_mut()
            .spawn((LastActivatedAt(1), ChildOf(child)))
            .id();

        app.world_mut().trigger(ActivateRequest { entity: leaf });
        app.update();

        assert!(app.world().get::<LastActivatedAt>(root).unwrap().0 > 1);
        assert!(app.world().get::<LastActivatedAt>(child).unwrap().0 > 1);
        assert!(app.world().get::<LastActivatedAt>(leaf).unwrap().0 > 1);
    }
}
