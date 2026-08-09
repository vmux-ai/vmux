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

/// Bin-event id for native → page prerequisite status pushes.
pub const AGENT_SETUP_PREREQ_EVENT: &str = "agent_setup_prereq";

/// Bin-event id for native → page install-result pushes.
pub const AGENT_SETUP_RESULT_EVENT: &str = "agent_setup_result";

/// Page → native: asks whether `agent` needs a prerequisite installed first.
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

/// Native → page: whether Homebrew must be installed before the agent.
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

/// Native → page: terminal install finished. `ok == false` drives the Retry state.
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

#[cfg(all(test, not(web)))]
#[path = "event.test.rs"]
mod tests;
