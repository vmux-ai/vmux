use std::collections::BTreeMap;

use serde::Deserialize;

use crate::AgentKind;

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
    #[serde(default)]
    pub(crate) conversation_title_prompt: String,
    pub(crate) file_touch_matcher: String,
}

#[derive(Deserialize)]
struct AgentFeatureManifest {
    cli: AgentCliManifest,
}

#[derive(Deserialize)]
struct AgentCliManifest {
    providers: BTreeMap<String, CliProviderManifest>,
}

impl CliProviderManifest {
    pub(crate) fn bundled(kind: AgentKind) -> Self {
        ron::from_str::<AgentFeatureManifest>(include_str!("feature.ron"))
            .expect("agent feature manifest contains valid CLI provider metadata")
            .cli
            .providers
            .remove(kind.as_url_segment())
            .expect("agent feature manifest defines the CLI provider")
    }
}
