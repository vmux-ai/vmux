use std::collections::BTreeSet;

use bevy_ecs::prelude::Component;
use vmux_core::tool::{
    ToolCategory, ToolItem, ToolOperationKey, ToolOperationKind, ToolProvider, ToolStatus,
};

use crate::{ToolStore, ToolsManifest};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolProviderId(pub ToolProvider);

#[derive(Component, Clone, Copy)]
pub struct ToolScanner(
    fn(&ToolStore, &mut ToolsManifest, bool) -> Result<ToolProviderSnapshot, String>,
);

impl ToolScanner {
    pub fn new(
        scan: fn(&ToolStore, &mut ToolsManifest, bool) -> Result<ToolProviderSnapshot, String>,
    ) -> Self {
        Self(scan)
    }

    pub(crate) fn scan(
        self,
        store: &ToolStore,
        manifest: &mut ToolsManifest,
        refresh: bool,
    ) -> Result<ToolProviderSnapshot, String> {
        (self.0)(store, manifest, refresh)
    }
}

pub struct ToolProviderSnapshot {
    pub category: ToolCategory,
    pub errors: Vec<String>,
}

impl From<ToolCategory> for ToolProviderSnapshot {
    fn from(category: ToolCategory) -> Self {
        Self {
            category,
            errors: Vec::new(),
        }
    }
}

#[derive(Component, Clone, Copy)]
pub struct ToolOperator(fn(&ToolStore, &ToolOperationKey, &str) -> Result<String, String>);

impl ToolOperator {
    pub fn new(run: fn(&ToolStore, &ToolOperationKey, &str) -> Result<String, String>) -> Self {
        Self(run)
    }

    pub(crate) fn run(
        self,
        store: &ToolStore,
        operation: &ToolOperationKey,
        value: &str,
    ) -> Result<String, String> {
        (self.0)(store, operation, value)
    }
}

#[derive(Component, Clone, Copy)]
pub struct ToolApplier(fn(&ToolStore) -> Result<usize, String>);

impl ToolApplier {
    pub fn new(run: fn(&ToolStore) -> Result<usize, String>) -> Self {
        Self(run)
    }

    pub(crate) fn run(self, store: &ToolStore) -> Result<usize, String> {
        (self.0)(store)
    }
}

#[derive(Clone, Debug)]
pub struct ToolInventoryItem {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub version: Option<String>,
    pub detail: String,
    pub status: ToolStatus,
    pub removable: bool,
}

pub struct ToolInventory {
    provider: ToolProvider,
    items: Vec<ToolInventoryItem>,
}

impl ToolInventory {
    pub fn new(provider: ToolProvider, items: Vec<ToolInventoryItem>) -> Self {
        Self { provider, items }
    }

    pub fn reconcile(self, manifest: &mut ToolsManifest) -> ToolCategory {
        for item in self
            .items
            .iter()
            .filter(|item| matches!(item.status, ToolStatus::Installed | ToolStatus::Outdated))
        {
            manifest.set_package(self.provider.id(), &item.id, true);
        }
        let mut items = self
            .items
            .into_iter()
            .map(|item| {
                let managed = manifest.contains(self.provider.id(), &item.id);
                ToolItem {
                    provider: self.provider,
                    operations: package_operations(item.status, managed, item.removable),
                    id: item.id,
                    name: item.name,
                    icon: item.icon,
                    version: item.version,
                    detail: item.detail,
                    status: item.status,
                    managed,
                }
            })
            .collect::<Vec<_>>();
        let existing = items
            .iter()
            .map(|item| item.id.clone())
            .collect::<BTreeSet<_>>();
        for name in manifest.managed_packages(self.provider.id()) {
            if existing.contains(&name) {
                continue;
            }
            items.push(ToolItem {
                provider: self.provider,
                id: name.clone(),
                name,
                icon: None,
                version: None,
                detail: "Declared in tools.toml".to_string(),
                status: ToolStatus::Missing,
                managed: true,
                operations: vec![ToolOperationKind::Install, ToolOperationKind::Forget],
            });
        }
        items.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| left.name.cmp(&right.name))
        });
        ToolCategory {
            provider: self.provider,
            items,
        }
    }
}

fn package_operations(
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
    if status == ToolStatus::Missing {
        operations.push(ToolOperationKind::Install);
    }
    if removable {
        operations.push(ToolOperationKind::Uninstall);
    }
    operations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_reconciliation_imports_installed_items_and_preserves_missing_declarations() {
        let mut manifest = ToolsManifest::default();
        manifest.set_package(ToolProvider::Npm.id(), "missing", true);
        let category = ToolInventory::new(
            ToolProvider::Npm,
            vec![ToolInventoryItem {
                id: "installed".to_string(),
                name: "Installed".to_string(),
                icon: None,
                version: Some("1".to_string()),
                detail: String::new(),
                status: ToolStatus::Installed,
                removable: true,
            }],
        )
        .reconcile(&mut manifest);

        assert!(manifest.contains(ToolProvider::Npm.id(), "installed"));
        assert_eq!(category.items.len(), 2);
        assert_eq!(category.items[0].id, "installed");
        assert_eq!(category.items[1].id, "missing");
    }

    #[test]
    fn reconciled_installed_items_keep_metadata_and_removal() {
        let mut manifest = ToolsManifest::default();
        let category = ToolInventory::new(
            ToolProvider::Npm,
            vec![ToolInventoryItem {
                id: "typescript".to_string(),
                name: "TypeScript".to_string(),
                icon: Some("typescript.svg".to_string()),
                version: Some("5.9.0".to_string()),
                detail: String::new(),
                status: ToolStatus::Installed,
                removable: true,
            }],
        )
        .reconcile(&mut manifest);

        assert_eq!(category.items[0].icon.as_deref(), Some("typescript.svg"));
        assert_eq!(category.items[0].operations, [ToolOperationKind::Uninstall]);
    }

    #[test]
    fn unmanaged_outdated_items_can_be_adopted_or_updated() {
        let operations = package_operations(ToolStatus::Outdated, false, true);

        assert_eq!(
            operations,
            [
                ToolOperationKind::Adopt,
                ToolOperationKind::Update,
                ToolOperationKind::Uninstall,
            ]
        );
    }
}
