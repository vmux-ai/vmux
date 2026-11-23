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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectorSetupRoute(pub String);

impl ConnectorSetupRoute {
    pub fn url(&self) -> String {
        format!("vmux://tools/connectors/{}/setup", self.0)
    }

    pub fn parse(value: &str) -> Option<Self> {
        let route = crate::VmuxRoute::parse(value)?;
        if route.host() != "tools" || route.query().is_some() || route.fragment().is_some() {
            return None;
        }
        let mut segments = route.path_segments();
        if segments.next()? != "connectors" {
            return None;
        }
        let id = segments.next()?;
        if id.is_empty()
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return None;
        }
        if segments.next()? != "setup" || segments.next().is_some() {
            return None;
        }
        Some(Self(id.to_string()))
    }
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
    fn connector_setup_urls_select_one_provider_and_reject_other_routes() {
        for id in ["github", "linear"] {
            let route = super::ConnectorSetupRoute(id.into());
            assert_eq!(super::ConnectorSetupRoute::parse(&route.url()), Some(route));
        }
        for url in [
            "https://tools/connectors/github/setup",
            "vmux://tools/connectors",
            "vmux://tools/connectors/github",
            "vmux://tools/connectors/github/setup/other",
            "vmux://tools/connectors/github/setup?other=1",
            "vmux://tools/connectors/github%2Flinear/setup",
        ] {
            assert_eq!(super::ConnectorSetupRoute::parse(url), None);
        }
    }

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
