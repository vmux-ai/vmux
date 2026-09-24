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
    pub servers: Vec<McpServerEntry>,
}

#[vmux_api::ui_event(Default, Eq, targets = ["command-bar", "layout", "sessions", "agent", "start"])]
pub struct McpServersRequest;

#[vmux_api::contract(Copy, Default, Eq)]
pub enum McpServerAction {
    #[default]
    Connect,
    Disconnect,
}

#[vmux_api::ui_event(Default, Eq, targets = ["command-bar", "layout", "sessions", "agent", "start"])]
pub struct McpServerRequest {
    pub id: String,
    pub action: McpServerAction,
}

#[vmux_api::host_event(Default, Eq, targets = ["command-bar", "layout", "sessions", "agent", "start"])]
pub struct McpServerResult {
    pub id: String,
    pub action: McpServerAction,
    pub success: bool,
    pub message: String,
}
