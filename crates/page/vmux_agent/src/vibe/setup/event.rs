enum Events {}

impl vmux_api::BinEventFamily for Events {
    const TARGET: vmux_api::BinEventTarget = vmux_api::BinEventTarget::Hosts(&["agent", "agents"]);
}

#[vmux_api::payload(Default)]
#[derive(vmux_api::UiEvent)]
pub struct AgentInstallRunRequest {
    pub agent: String,
}

#[vmux_api::payload(Default)]
#[derive(vmux_api::UiEvent)]
pub struct AgentSetupPrereqRequest {
    pub agent: String,
}

#[vmux_api::payload(Default)]
#[derive(vmux_api::HostEvent)]
pub struct AgentSetupPrereqStatus {
    pub needs_homebrew: bool,
}

#[vmux_api::payload(Default)]
#[derive(vmux_api::HostEvent)]
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
