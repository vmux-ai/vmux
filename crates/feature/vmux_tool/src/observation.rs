use bevy_ecs::prelude::*;

pub use crate::catalog::{Detail, Icon, Id, Operations, Status, Version};
use crate::state::{ToolOperationKind, ToolStatus};

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObservationSet;

#[derive(Component)]
pub struct Handler;

#[derive(Component, Default)]
pub(crate) struct ManifestProvider;

#[derive(Component)]
#[require(ManifestProvider)]
pub struct PackageProvider;

#[derive(Component)]
pub struct Accepted;

#[derive(EntityEvent)]
pub struct ObserveProviderRequest {
    #[event_target]
    pub entity: Entity,
    pub observation: Entity,
    pub refresh_catalogs: bool,
}

#[derive(Component)]
pub struct Observation;

#[derive(Component)]
pub struct Finished {
    pub error: Option<String>,
}

#[derive(Component, Default)]
pub struct Diagnostics(pub Vec<String>);

#[derive(Component)]
pub struct Item;

#[derive(Component)]
pub struct Package;

#[derive(Component)]
pub struct Removable(pub bool);

#[derive(Component)]
pub struct Dotfile;

#[derive(Component)]
pub struct Discovered;

#[derive(Bundle)]
pub struct ObservedTool {
    pub id: Id,
    pub name: Name,
    pub version: Version,
    pub icon: Icon,
    pub detail: Detail,
    pub status: Status,
    pub operations: Operations,
    pub item: Item,
}

impl ObservedTool {
    pub fn new(
        id: String,
        name: String,
        version: Option<String>,
        icon: Option<String>,
        detail: String,
        status: ToolStatus,
        operations: Vec<ToolOperationKind>,
    ) -> Self {
        Self {
            id: Id(id),
            name: Name::new(name),
            version: Version(version),
            icon: Icon(icon),
            detail: Detail(detail),
            status: Status(status),
            operations: Operations(operations),
            item: Item,
        }
    }
}

#[derive(Bundle)]
pub struct ObservedPackage {
    pub tool: ObservedTool,
    pub removable: Removable,
    pub package: Package,
}

impl ObservedPackage {
    pub fn new(tool: ObservedTool, removable: bool) -> Self {
        Self {
            tool,
            removable: Removable(removable),
            package: Package,
        }
    }
}

#[derive(bevy_ecs::system::SystemParam)]
pub struct InFlight<'w, 's> {
    native: Query<'w, 's, &'static vmux_ecs::OperationTarget>,
    owners: Query<'w, 's, &'static vmux_ecs::EntityTarget<Observation>>,
}

impl InFlight<'_, '_> {
    pub fn pending(&self, observation: Entity) -> bool {
        for target in &self.native {
            let owner = target.entity();
            if owner == observation
                || self
                    .owners
                    .get(owner)
                    .is_ok_and(|owner| owner.entity() == observation)
            {
                return true;
            }
        }
        false
    }
}

impl Package {
    pub(crate) fn operations(
        status: ToolStatus,
        managed: bool,
        removable: bool,
    ) -> Vec<ToolOperationKind> {
        let mut operations = Vec::new();
        if !managed && matches!(status, ToolStatus::Installed | ToolStatus::Outdated) {
            operations.push(ToolOperationKind::Adopt);
        }
        if status == ToolStatus::Outdated {
            operations.push(ToolOperationKind::Update);
        }
        if matches!(status, ToolStatus::Available | ToolStatus::Missing) {
            operations.push(ToolOperationKind::Install);
        }
        if removable {
            operations.push(ToolOperationKind::Uninstall);
        }
        operations
    }
}
