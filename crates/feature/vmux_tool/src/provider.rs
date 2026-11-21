use crate::state::{ToolOperationKey, ToolProvider};
use bevy_ecs::prelude::Component;

use crate::ToolStore;
use vmux_feature::manifest::{FeatureId, FeatureManifestSource};

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ToolProviderId(pub ToolProvider);

#[derive(Component, Clone, Copy)]
pub struct ToolProviderBinding {
    pub(crate) feature: FeatureId,
    pub(crate) index: usize,
}

impl ToolProviderBinding {
    pub fn new<M: FeatureManifestSource>(index: usize) -> Self {
        Self {
            feature: FeatureId::of::<M>(),
            index,
        }
    }

    pub fn feature(self) -> FeatureId {
        self.feature
    }

    pub fn index(self) -> usize {
        self.index
    }
}

#[derive(Component, Clone, Copy)]
pub struct ToolOperator(fn(&ToolStore, &ToolOperationKey, &str) -> Result<String, String>);

impl ToolOperator {
    pub fn new(run: fn(&ToolStore, &ToolOperationKey, &str) -> Result<String, String>) -> Self {
        Self(run)
    }

    pub fn run(
        self,
        store: &ToolStore,
        operation: &ToolOperationKey,
        value: &str,
    ) -> Result<String, String> {
        (self.0)(store, operation, value)
    }
}
