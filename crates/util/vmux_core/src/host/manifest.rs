use bevy::prelude::*;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct FeatureManifestSource(&'static str);

impl FeatureManifestSource {
    pub const fn new(source: &'static str) -> Self {
        Self(source)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

pub struct FeatureManifestPlugin {
    source: FeatureManifestSource,
}

impl FeatureManifestPlugin {
    pub const fn new(source: &'static str) -> Self {
        Self {
            source: FeatureManifestSource::new(source),
        }
    }
}

impl Plugin for FeatureManifestPlugin {
    fn build(&self, app: &mut App) {
        let source = self.source;
        app.add_systems(PreStartup, move |mut commands: Commands| {
            commands.spawn((Name::new("Feature manifest"), source));
        });
    }

    fn is_unique(&self) -> bool {
        false
    }
}
