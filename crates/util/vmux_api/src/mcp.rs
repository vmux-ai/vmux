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
    pub account: Option<String>,
    pub setup_url: Option<String>,
}

#[vmux_api::ui_state(Default, Eq, version = 2)]
pub struct McpServersUiState {
    pub loaded: bool,
    pub loading: bool,
    pub servers: Vec<McpServerEntry>,
    pub pending: Option<McpServerPending>,
    pub result: Option<McpServerResult>,
    pub authorization: Option<McpAuthorization>,
    pub selected: Vec<String>,
}

#[vmux_api::contract(Default, Eq)]
pub struct McpAuthorization {
    pub url: String,
    pub code: Option<String>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ConnectorPopupState {
    pub id: String,
    pub provider: String,
    pub name: String,
    pub origin: String,
    pub code: Option<String>,
    pub open: bool,
}

#[vmux_api::ui_event]
pub struct ConnectorPopupShowRequest {
    pub id: String,
}

#[vmux_api::ui_event]
pub struct ConnectorPopupHideRequest {
    pub id: String,
}

#[vmux_api::ui_event]
pub struct ConnectorPopupBoundsRequest {
    pub id: String,
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

#[vmux_api::ui_event]
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

#[vmux_api::ui_event(Default, Eq)]
pub struct McpServerRequest {
    pub id: String,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct McpServerSelectionRequest {
    pub id: String,
}

#[vmux_api::ui_event]
pub struct McpServerCancelRequest;

#[vmux_api::ui_event]
pub struct McpLinkOpenRequest {
    pub url: String,
    pub external: bool,
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
        let state = McpServersUiState {
            selected: Vec::new(),
            authorization: None,
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
        let decoded = rkyv::from_bytes::<McpServersUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert!(matches!(
            decoded.result,
            Some(McpServerResult { id, success: false, .. }) if id == "linear"
        ));
    }
}
