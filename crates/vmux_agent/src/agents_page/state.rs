//! The agent catalog the page renders, and the search over it.

use dioxus::prelude::*;
use vmux_ui::hooks::{send, use_listener, use_theme};
use vmux_ui::i18n::translate;

use crate::agents_page::event::{
    AGENTS_CATALOG_EVENT, AgentEntry, AgentsCatalog, AgentsCatalogRequest,
};
use crate::vibe::setup::event::{AGENT_SETUP_RESULT_EVENT, AgentSetupResult};

/// Every installed agent, the search narrowing them, and whether the first fetch has landed.
#[derive(Clone, Copy, PartialEq)]
pub struct Catalog {
    agents: Signal<Vec<AgentEntry>>,
    pub query: Signal<String>,
    pub loaded: Signal<bool>,
}

/// Fetch the catalog, keep it in step with install results, and title the document.
pub fn use_catalog() -> Catalog {
    let locale = use_theme();
    let catalog = Catalog {
        agents: use_signal(Vec::new),
        query: use_signal(String::new),
        loaded: use_signal(|| false),
    };

    let mut agents = catalog.agents;
    let mut loaded = catalog.loaded;
    let _fetched = use_listener::<AgentsCatalog, _>(AGENTS_CATALOG_EVENT, move |incoming| {
        agents.set(incoming.agents);
        loaded.set(true);
    });

    // An install reports its own result, so the row is moved before the refetch lands rather than
    // sitting on "installing" for a round trip.
    let _installed = use_listener::<AgentSetupResult, _>(AGENT_SETUP_RESULT_EVENT, move |result| {
        let id = format!("cli:{}", result.agent);
        if result.ok {
            catalog.set_status(&id, "installed", "");
            Catalog::request();
        } else {
            catalog.set_status(&id, "error", &translate("agents-install-failed"));
        }
    });

    use_effect(move || {
        locale();
        set_document_title(&translate("agents-title"));
        Catalog::request();
    });

    catalog
}

impl Catalog {
    /// Ask the host to send the catalog.
    pub fn request() {
        let _ = send(&AgentsCatalogRequest {});
    }

    pub fn all(&self) -> Vec<AgentEntry> {
        (self.agents)()
    }

    /// The entries the current search leaves visible.
    pub fn matching(&self) -> Vec<AgentEntry> {
        let query = (self.query)();
        (self.agents)()
            .into_iter()
            .filter(|agent| agent.matches(&query))
            .collect()
    }

    /// Move one row to a new state, without waiting for a refetch to confirm it.
    pub fn set_status(&self, id: &str, status: &str, detail: &str) {
        let mut agents = self.agents;
        agents.with_mut(|list| {
            if let Some(agent) = list.iter_mut().find(|agent| agent.id == id) {
                agent.status = status.to_string();
                agent.detail = detail.to_string();
            }
        });
    }

    pub fn set_pinned_version(&self, id: &str, version: &str) {
        let mut agents = self.agents;
        agents.with_mut(|list| {
            if let Some(agent) = list.iter_mut().find(|agent| agent.id == id) {
                agent.pinned_version = version.to_string();
            }
        });
    }
}

/// The browser tab's title. A phone has no tab, so this is where that difference stops.
#[cfg(web)]
fn set_document_title(title: &str) {
    if let Some(doc) = web_sys::window().and_then(|window| window.document()) {
        doc.set_title(title);
    }
}

#[cfg(not(web))]
fn set_document_title(_title: &str) {}
