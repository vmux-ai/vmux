use crate::state::ToolProvider;
use bevy_ecs::prelude::Component;

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
