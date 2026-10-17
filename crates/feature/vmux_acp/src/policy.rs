use bevy::prelude::*;
use serde::Deserialize;
use vmux_ecs::manifest::{FeatureManifest, FeatureManifestOwner};
#[cfg(test)]
use vmux_ecs::manifest::{FeatureManifestSource, FeaturePlugin};

pub(crate) fn add(app: &mut App) {
    app.configure_sets(Startup, PolicyLoaded)
        .add_systems(Startup, load.in_set(PolicyLoaded));
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PolicyLoaded;

#[derive(Deserialize)]
pub(super) struct AgentPolicies {
    pub(super) acp_registry: AcpRegistryPolicy,
    pub(super) acp_workspace: AcpWorkspacePolicy,
}

#[derive(Component, Clone, Deserialize)]
pub(crate) struct AcpRegistryPolicy {
    pub(crate) url: String,
}

#[derive(Component, Clone, Deserialize)]
pub(crate) struct AcpWorkspacePolicy {
    pub(crate) unbound: String,
    pub(crate) pending_worktree: String,
    pub(crate) repository_needs_worktree: String,
}

fn load(
    manifests: Query<
        (Entity, &FeatureManifest),
        (
            Added<FeatureManifest>,
            With<FeatureManifestOwner<crate::Feature>>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, manifest) in &manifests {
        let policies = manifest
            .policies::<AgentPolicies>()
            .expect("agent feature manifest contains valid policies");
        let mut entity = commands.entity(entity);
        if let Some(policies) = policies {
            entity.insert((policies.acp_registry, policies.acp_workspace));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ForeignFeature;

    impl FeatureManifestSource for ForeignFeature {
        const SOURCE: &'static str = "(policies: Some((unrelated: true)))";
    }

    #[test]
    fn acp_feature_manifest_attaches_typed_sections_to_one_entity() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            FeaturePlugin::<crate::Feature>::default(),
            FeaturePlugin::<ForeignFeature>::default(),
        ));
        add(&mut app);
        app.update();

        let entries = app
            .world_mut()
            .query::<(&FeatureManifest, &AcpRegistryPolicy, &AcpWorkspacePolicy)>()
            .iter(app.world())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].1.url.starts_with("https://"));
        assert!(entries[0].2.unbound.contains("select_project"));
    }
}
