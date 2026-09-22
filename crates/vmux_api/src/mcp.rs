#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum McpServerStatus {
    #[default]
    Available,
    Configured,
    Connected,
    AuthenticationRequired,
    Failed,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct McpServerEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: McpServerStatus,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(
    targets = ["command-bar", "layout", "sessions", "agent", "start"]
)]
pub struct McpServers {
    pub servers: Vec<McpServerEntry>,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(
    targets = ["command-bar", "layout", "sessions", "agent", "start"]
)]
pub struct McpServersRequest;

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum McpServerAction {
    #[default]
    Connect,
    Disconnect,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(
    targets = ["command-bar", "layout", "sessions", "agent", "start"]
)]
pub struct McpServerRequest {
    pub id: String,
    pub action: McpServerAction,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(
    targets = ["command-bar", "layout", "sessions", "agent", "start"]
)]
pub struct McpServerResult {
    pub id: String,
    pub action: McpServerAction,
    pub success: bool,
    pub message: String,
}
