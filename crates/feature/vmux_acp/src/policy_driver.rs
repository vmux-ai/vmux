use vmux_ecs::manifest::FeatureManifest;

#[cfg(test)]
use crate::policy::AcpWorkspacePolicy;
use crate::policy::{AcpRegistryPolicy, AgentPolicies};

pub(crate) struct PolicyDriver;

impl PolicyDriver {
    pub(crate) fn registry() -> AcpRegistryPolicy {
        Self::policies().acp_registry
    }

    #[cfg(test)]
    pub(crate) fn bundled() -> AcpWorkspacePolicy {
        Self::policies().acp_workspace
    }

    fn policies() -> AgentPolicies {
        FeatureManifest::of::<crate::Feature>()
            .policies::<AgentPolicies>()
            .unwrap()
            .unwrap()
    }
}
