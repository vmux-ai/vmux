use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::Deserialize;
use vmux_core::host::manifest::{FeatureManifest, FeatureManifestPlugin};

use crate::AgentKind;

pub(crate) struct AgentManifestPlugin;

impl Plugin for AgentManifestPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeatureManifestPlugin::new(crate::FEATURE_MANIFEST))
            .add_systems(Startup, load);
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CliProviderManifest {
    #[serde(default)]
    pub(crate) disabled_features: Vec<String>,
    #[serde(default)]
    pub(crate) disallowed_tools: Vec<String>,
    #[serde(default)]
    pub(crate) host_allowed_tools: Vec<String>,
    #[serde(default)]
    pub(crate) direct_only_namespace: String,
    pub(crate) run_prompt: String,
    pub(crate) file_touch_matcher: String,
}

#[derive(Component)]
pub(crate) struct CliProviderManifests(BTreeMap<String, CliProviderManifest>);

impl CliProviderManifests {
    pub(crate) fn get(&self, kind: AgentKind) -> &CliProviderManifest {
        self.0
            .get(kind.as_url_segment())
            .expect("agent feature manifest defines the CLI provider")
    }
}

#[cfg(test)]
impl CliProviderManifest {
    pub(crate) fn bundled(kind: AgentKind) -> Self {
        let manifest = FeatureManifest::parse(crate::FEATURE_MANIFEST);
        let mut providers = manifest
            .cli
            .unwrap()
            .providers::<BTreeMap<String, Self>>()
            .unwrap()
            .unwrap();
        providers.remove(kind.as_url_segment()).unwrap()
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
        let providers = match &manifest.cli {
            Some(cli) => cli
                .providers::<BTreeMap<String, CliProviderManifest>>()
                .expect("agent feature manifest contains valid CLI provider metadata"),
            None => None,
        };
        let policies = manifest
            .policies::<BTreeMap<String, AcpWorkspacePolicy>>()
            .expect("agent feature manifest contains valid policies");
        let mut entity = commands.entity(entity);
        if let Some(providers) = providers {
            entity.insert(CliProviderManifests(providers));
        }
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
            .query::<(&FeatureManifest, &CliProviderManifests, &AcpWorkspacePolicy)>()
            .iter(app.world())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].1.get(AgentKind::Codex).direct_only_namespace,
            "mcp__vmux"
        );
        assert!(entries[0].2.unbound.contains("select_project"));
    }
}
