//! What the page and the plugin say to each other.
//!
//! Shared bin-ipc payloads for the `vmux://agents` manager page (browse the ACP registry).
//! Ungated: it is the one part both halves compile, and the reason neither has to know how the
//! other is built. Compiled for both native (Bevy host) and wasm (Dioxus page); rkyv on the wire.
//! The chat page's own vocabulary lives in [`vmux_chat::event`].

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
    PartialEq,
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
    /// Pinned package version for npx/uvx agents (`""` = latest). Reflects the dropdown selection;
    /// edited in place on the page.
    pub pinned_version: String,
    /// The applied/installed version (`""` = latest) at push time — the immutable baseline the
    /// dropdown starts from. The "Apply" button shows only while `pinned_version` differs from it.
    pub installed_version: String,
    /// Published versions (newest-first) offered in the version selector. Empty = not (yet)
    /// fetched or unavailable; the page falls back to free-text entry.
    pub available_versions: Vec<String>,
}

impl AgentEntry {
    /// Whether the search leaves this entry visible. An empty search leaves all of them.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.name.to_lowercase().contains(&query)
            || self.id.to_lowercase().contains(&query)
            || self.description.to_lowercase().contains(&query)
            || self.runtime.to_lowercase().contains(&query)
            || self.source.to_lowercase().contains(&query)
    }
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
    /// Requested package version (`""` = latest). Persisted to the agent's settings entry.
    pub version: String,
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
