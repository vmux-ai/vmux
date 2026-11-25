use std::path::PathBuf;

use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryFilter;
use bevy_reflect::{GetTypeRegistration, Reflect, TypePath};
use vmux_store::{Save, StoreDomain};

use crate::Uuid;

pub enum WorkspaceStore {}

pub type WorkspaceStored = StoreDomain<WorkspaceStore>;

#[derive(Event, Clone, Copy, Debug, Default)]
pub struct StoreDirty;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PageRestore;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct WorkspaceRestore {
    pub store_present: bool,
    pub complete: bool,
}

#[derive(Message, Clone)]
pub struct WorkspaceSaveRequest {
    pub path: PathBuf,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceCommitRequest {
    pub request_id: Uuid,
    pub requester: Entity,
    pub profile: Entity,
    pub path: PathBuf,
    pub required: Entity,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceCommitCompleted {
    pub request_id: Uuid,
    pub requester: Entity,
    pub profile: Entity,
    pub store: Entity,
    pub path: PathBuf,
    pub revision: u64,
    pub result: Result<(), String>,
}

pub type WorkspaceStoreValidator = vmux_store::StoreValidator;

pub type WorkspaceRestoreValidator = vmux_store::RestoreValidator;

pub type WorkspaceStoreValidators<'w, 's> = vmux_store::StoreValidators<'w, 's>;

pub trait StoreAppExt {
    fn register_stored<T>(&mut self) -> &mut Self
    where
        T: Component + Reflect + TypePath + GetTypeRegistration;

    fn track_store<T>(&mut self) -> &mut Self
    where
        T: Component;

    fn track_domain<D, F, T>(&mut self) -> &mut Self
    where
        D: Send + Sync + 'static,
        F: QueryFilter + 'static,
        T: Component;
}

impl StoreAppExt for App {
    fn register_stored<T>(&mut self) -> &mut Self
    where
        T: Component + Reflect + TypePath + GetTypeRegistration,
    {
        <Self as vmux_store::StoreAppExt>::register_domain::<WorkspaceStore, T>(self)
    }

    fn track_store<T>(&mut self) -> &mut Self
    where
        T: Component,
    {
        <Self as vmux_store::StoreAppExt>::track_domain::<WorkspaceStore, With<Save>, T>(self)
    }

    fn track_domain<D, F, T>(&mut self) -> &mut Self
    where
        D: Send + Sync + 'static,
        F: QueryFilter + 'static,
        T: Component,
    {
        <Self as vmux_store::StoreAppExt>::track_domain::<D, F, T>(self)
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::reflect::AppTypeRegistry;
    use vmux_store::{Dirty, SchemaPlugin};

    use super::*;

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Saved;

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Transient;

    #[derive(Resource, Default)]
    struct DirtyCount(u32);

    impl DirtyCount {
        fn record(_: On<Dirty<WorkspaceStore>>, mut count: ResMut<Self>) {
            count.0 += 1;
        }
    }

    #[crate::store(domain = WorkspaceStore, track; Saved)]
    struct WorkspaceSchema;

    #[test]
    fn workspace_store_facade_registers_and_tracks_under_workspace_domain() {
        let mut app = App::new();
        app.add_plugins(SchemaPlugin::<WorkspaceSchema>::default())
            .register_type::<Transient>()
            .init_resource::<DirtyCount>()
            .add_observer(DirtyCount::record);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let workspace = WorkspaceStored::filter(&registry);
        assert!(workspace.is_allowed::<Saved>());
        assert!(!workspace.is_allowed::<Transient>());
        drop(registry);

        let entity = app.world_mut().spawn((Save, Saved)).id();
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 1);

        app.register_stored::<Saved>();
        app.world_mut().entity_mut(entity).insert(Saved);
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 2);
    }
}
