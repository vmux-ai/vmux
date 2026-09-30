use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::Deserialize;
use vmux_core::host::manifest::{FeatureManifest, FeatureManifestPlugin};

pub(crate) struct AgentManifestPlugin;

impl Plugin for AgentManifestPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeatureManifestPlugin::<crate::Feature>::new(
            crate::FEATURE_MANIFEST,
        ))
        .add_systems(Startup, load);
    }
}

#[derive(Component, Clone, Deserialize)]
pub(crate) struct AcpWorkspacePolicy {
    pub(crate) unbound: String,
    pub(crate) pending_worktree: String,
    pub(crate) repository_needs_worktree: String,
}

#[cfg(test)]
impl AcpWorkspacePolicy {
    pub(crate) fn bundled() -> Self {
        let manifest = FeatureManifest::parse(crate::FEATURE_MANIFEST);
        manifest
            .policies::<BTreeMap<String, Self>>()
            .unwrap()
            .unwrap()
            .remove("acp_workspace")
            .unwrap()
    }
}

fn load(
    manifests: Query<(Entity, &FeatureManifest), Added<FeatureManifest>>,
    mut commands: Commands,
) {
    for (entity, manifest) in &manifests {
        let policies = manifest
            .policies::<BTreeMap<String, AcpWorkspacePolicy>>()
            .expect("agent feature manifest contains valid policies");
        let mut entity = commands.entity(entity);
        if let Some(mut policies) = policies {
            let policy = policies
                .remove("acp_workspace")
                .expect("agent feature manifest defines acp_workspace policy");
            entity.insert(policy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_feature_manifest_attaches_typed_sections_to_one_entity() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AgentManifestPlugin));
        app.update();

        let entries = app
            .world_mut()
            .query::<(&FeatureManifest, &AcpWorkspacePolicy)>()
            .iter(app.world())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].1.unbound.contains("select_project"));
    }
}
