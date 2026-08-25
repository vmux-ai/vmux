#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentInstallRunRequest {
    pub agent: String,
}

pub const AGENT_SETUP_PREREQ_EVENT: &str = "agent_setup_prereq";

pub const AGENT_SETUP_RESULT_EVENT: &str = "agent_setup_result";

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentSetupPrereqRequest {
    pub agent: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentSetupPrereqStatus {
    pub needs_homebrew: bool,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentSetupResult {
    pub agent: String,
    pub ok: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prereq_status_rkyv_roundtrip() {
        let v = AgentSetupPrereqStatus {
            needs_homebrew: true,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
        let back = rkyv::from_bytes::<AgentSetupPrereqStatus, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(back.needs_homebrew);
    }

    #[test]
    fn result_rkyv_roundtrip() {
        let v = AgentSetupResult {
            agent: "codex".to_string(),
            ok: false,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
        let back = rkyv::from_bytes::<AgentSetupResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.agent, "codex");
        assert!(!back.ok);
    }
}
