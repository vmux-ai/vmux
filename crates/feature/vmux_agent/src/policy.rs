use bevy::prelude::*;
use serde::Deserialize;
use vmux_core::host::manifest::FeatureManifest;

pub(crate) struct AgentPolicyPlugin;

impl Plugin for AgentPolicyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load);
    }
}

#[derive(Deserialize)]
struct AgentPolicies {
    acp_workspace: AcpWorkspacePolicy,
}

#[cfg(test)]
impl AgentPolicies {
    fn bundled() -> Self {
        FeatureManifest::of::<crate::Feature>()
            .policies::<Self>()
            .unwrap()
            .unwrap()
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
        AgentPolicies::bundled().acp_workspace
    }
}

fn load(
    manifests: Query<(Entity, &FeatureManifest), Added<FeatureManifest>>,
    mut commands: Commands,
) {
    for (entity, manifest) in &manifests {
        let policies = manifest
            .policies::<AgentPolicies>()
            .expect("agent feature manifest contains valid policies");
        let mut entity = commands.entity(entity);
        if let Some(policies) = policies {
            entity.insert(policies.acp_workspace);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_feature_manifest_attaches_typed_sections_to_one_entity() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_core::host::manifest::FeaturePlugin::<crate::Feature>::default(),
            AgentPolicyPlugin,
        ));
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
