//! Reflection registration for the shared component vocabulary.

use bevy::prelude::*;

use crate::PageMetadata;
use crate::archive::{ArchivedPage, ArchivedPagePosition, ArchivedTabPage, PaneStep, SplitAxis};
use crate::component::{
    Active, Bookmark, BookmarkOrder, Collapsed, CreatedAt, Folder, LastActivatedAt, LastVisitedAt,
    Order, Pin, TransitionType, Url, Uuid, Visit, VisitCount, VisitedUrl,
};
use crate::icon::{BuiltinIcon, PageIcon};

/// Registers reflection for the shared component types so they can be saved and loaded.
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
            .register_type::<Collapsed>()
            .register_type::<Uuid>()
            .register_type::<Children>()
            .register_type::<ChildOf>();
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
}
