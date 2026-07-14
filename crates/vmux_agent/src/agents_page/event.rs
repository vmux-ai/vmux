//! Shared bin-ipc payloads for the `vmux://agents` manager page (browse the ACP registry).
//! Compiled for both native (Bevy host) and wasm (Dioxus page); rkyv on the wire.

/// Bin-event id: native → page, the registry catalog to render.
pub const AGENTS_CATALOG_EVENT: &str = "agents_catalog";

/// Native → page: the browsable agent catalog.
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
pub struct AgentsCatalog {
    pub agents: Vec<AgentEntry>,
}

/// One catalog row.
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
pub struct AgentEntry {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    /// `acp` | `cli`.
    pub source: String,
    /// URL opened after installation or from the installed row.
    pub launch_url: String,
    /// Whether vmux owns enough state to remove this installation safely.
    pub uninstallable: bool,
    /// `native` | `node` | `python` | `cli`.
    pub runtime: String,
    /// `available` | `installing` | `installed` | `update` | `error`.
    pub status: String,
    /// Progress text (while installing) or error message.
    pub detail: String,
}

/// Page → native: the page mounted and wants the catalog pushed to it.
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
pub struct AgentsCatalogRequest {}

/// Page → native: install (or update) the named agent's runtime + package.
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
pub struct AgentsInstall {
    pub id: String,
}

/// Page → native: remove an installed native-binary agent.
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
pub struct AgentsUninstall {
    pub id: String,
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
pub struct AgentsOpen {
    pub url: String,
}
