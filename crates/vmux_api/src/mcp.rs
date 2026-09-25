#[vmux_api::contract(Copy, Default, Eq)]
pub enum McpServerStatus {
    #[default]
    Available,
    Configured,
    Connected,
    AuthenticationRequired,
    Failed,
}

#[vmux_api::contract(Default, Eq)]
pub struct McpServerEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: McpServerStatus,
}

#[vmux_api::ui_state(Default, Eq, targets = ["command-bar", "layout", "sessions", "agent", "start"])]
pub struct McpServers {
    pub loaded: bool,
    pub loading: bool,
    pub servers: Vec<McpServerEntry>,
    pub pending: Option<McpServerPending>,
    pub result: Option<McpServerResult>,
}

#[vmux_api::ui_event(Default, Eq, targets = ["command-bar", "layout", "sessions", "agent", "start"])]
pub struct McpServersRequest;

#[vmux_api::contract(Copy, Default, Eq)]
pub enum McpServerOperation {
    #[default]
    Connect,
    Disconnect,
}

#[vmux_api::contract(Default, Eq)]
pub struct McpServerPending {
    pub id: String,
    pub operation: McpServerOperation,
}

#[vmux_api::ui_event(Default, Eq, targets = ["command-bar", "layout", "sessions", "agent", "start"])]
pub struct McpServerRequest {
    pub id: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct McpServerResult {
    pub id: String,
    pub operation: McpServerOperation,
    pub success: bool,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_result_round_trips_inside_state() {
        let state = McpServers {
            loaded: true,
            loading: false,
            servers: Vec::new(),
            pending: None,
            result: Some(McpServerResult {
                id: "linear".to_string(),
                operation: McpServerOperation::Connect,
                success: false,
                message: "denied".to_string(),
            }),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&state).unwrap();
        let decoded = rkyv::from_bytes::<McpServers, rkyv::rancor::Error>(&bytes).unwrap();

        assert!(matches!(
            decoded.result,
            Some(McpServerResult { id, success: false, .. }) if id == "linear"
        ));
    }
}
