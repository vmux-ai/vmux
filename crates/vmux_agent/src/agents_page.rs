//! The `vmux://agents` manager page: browse the ACP registry catalog (all agents, with icons,
//! descriptions, and the runtime each needs). Install/spawn happens by opening an agent from the
//! launcher; this page is discovery.

pub mod event;

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};
#[cfg(not(target_arch = "wasm32"))]
use crossbeam_channel::{Receiver, Sender};
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
use crate::acp_registry::Runtime;
#[cfg(not(target_arch = "wasm32"))]
use crate::agents_page::event::{
    AGENTS_CATALOG_EVENT, AgentEntry, AgentsCatalog, AgentsCatalogRequest, AgentsInstall,
    AgentsOpen, AgentsUninstall,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::client::acp::{AcpCatalog, AcpInstallGeneration};
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::agent::AgentKind;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "agents",
    title: "Agents",
    keywords: &["acp", "agent", "install", "registry"],
    icon: Some(vmux_core::BuiltinIcon::Sparkles),
    command_bar: true,
};

/// The most recent `vmux://agents` webview to push the catalog to.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
struct AgentsPageWebview(Option<Entity>);

/// Session install status per agent id (`status`, `detail`), overlaid on the disk-derived state.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
struct AgentsStatus(HashMap<String, (String, String)>);

/// Background install progress/result for the manager page.
#[cfg(not(target_arch = "wasm32"))]
enum AgentMsg {
    Progress {
        id: String,
        pct: Option<u8>,
        message: String,
    },
    Done {
        id: String,
    },
    Failed {
        id: String,
        message: String,
    },
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource)]
struct AgentsInstallChannel {
    tx: Sender<AgentMsg>,
    rx: Receiver<AgentMsg>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for AgentsInstallChannel {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct AgentsManagerPlugin;

#[cfg(not(target_arch = "wasm32"))]
impl Plugin for AgentsManagerPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.init_resource::<AgentsPageWebview>()
            .init_resource::<AgentsStatus>()
            .init_resource::<AgentsInstallChannel>()
            .init_resource::<AcpInstallGeneration>()
            .add_plugins(BinEventEmitterPlugin::<(
                AgentsCatalogRequest,
                AgentsInstall,
                AgentsUninstall,
                AgentsOpen,
            )>::for_hosts(&["agents"]))
            .add_observer(on_catalog_request)
            .add_observer(on_install_request)
            .add_observer(on_uninstall_request)
            .add_observer(on_open_request)
            .add_systems(Update, (push_agents, drain_agent_installs));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_open_request(
    trigger: On<BinReceive<AgentsOpen>>,
    mut commands: MessageWriter<vmux_command::AppCommand>,
) {
    commands.write(vmux_command::AppCommand::Browser(
        vmux_command::BrowserCommand::Open(vmux_command::open::OpenCommand::InNewStack {
            url: Some(trigger.event().payload.url.clone()),
        }),
    ));
}

#[cfg(not(target_arch = "wasm32"))]
fn catalog_snapshot(catalog: &AcpCatalog, status: &AgentsStatus) -> AgentsCatalog {
    let mut agents: Vec<AgentEntry> = catalog
        .agents
        .iter()
        .map(|a| {
            let (st, detail) = status.0.get(&a.id).cloned().unwrap_or_else(|| {
                if crate::acp_install::is_update_available(a) {
                    ("update".to_string(), String::new())
                } else if crate::acp_install::is_agent_installed(a) {
                    ("installed".to_string(), String::new())
                } else {
                    ("available".to_string(), String::new())
                }
            });
            AgentEntry {
                id: a.id.clone(),
                name: a.name.clone(),
                icon: a.icon.clone().unwrap_or_default(),
                description: a.description.clone().unwrap_or_default(),
                source: "acp".to_string(),
                launch_url: format!("vmux://agent/{}", a.id),
                uninstallable: true,
                runtime: match a.preferred_runtime() {
                    Runtime::None => "native",
                    Runtime::Node => "node",
                    Runtime::Uv => "python",
                }
                .to_string(),
                status: st,
                detail,
            }
        })
        .collect();
    agents.extend(cli_agent_entries(|kind| {
        crate::exec::find_executable(kind.executable()).is_some()
    }));
    agents.sort_by_key(|a| a.name.to_lowercase());
    AgentsCatalog { agents }
}

#[cfg(not(target_arch = "wasm32"))]
fn cli_agent_entries(mut is_installed: impl FnMut(AgentKind) -> bool) -> Vec<AgentEntry> {
    AgentKind::all()
        .into_iter()
        .map(|kind| {
            let segment = kind.as_url_segment();
            AgentEntry {
                id: format!("cli:{segment}"),
                name: format!("{} CLI", kind.display_name()),
                icon: String::new(),
                description: "Terminal-based coding agent".to_string(),
                source: "cli".to_string(),
                launch_url: format!("{}cli", kind.cli_url_prefix()),
                uninstallable: false,
                runtime: "cli".to_string(),
                status: if is_installed(kind) {
                    "installed".to_string()
                } else {
                    "available".to_string()
                },
                detail: String::new(),
            }
        })
        .collect()
}

/// Remember which webview asked for the catalog; the push system delivers it.
#[cfg(not(target_arch = "wasm32"))]
fn on_catalog_request(
    trigger: On<BinReceive<AgentsCatalogRequest>>,
    mut webview_res: ResMut<AgentsPageWebview>,
) {
    webview_res.0 = Some(trigger.event().webview);
}

/// Kick a background install (or update) for the requested agent.
#[cfg(not(target_arch = "wasm32"))]
fn on_install_request(
    trigger: On<BinReceive<AgentsInstall>>,
    catalog: Res<AcpCatalog>,
    installs: Res<AgentsInstallChannel>,
    mut status: ResMut<AgentsStatus>,
) {
    let id = trigger.event().payload.id.clone();
    let Some(agent) = catalog.agents.iter().find(|a| a.id == id).cloned() else {
        return;
    };
    status.0.insert(
        id.clone(),
        ("installing".to_string(), "Preparing…".to_string()),
    );
    let tx = installs.tx.clone();
    std::thread::spawn(move || {
        let result = crate::acp_install::ensure_installed(&agent, |_phase, pct, msg| {
            let _ = tx.send(AgentMsg::Progress {
                id: id.clone(),
                pct,
                message: msg.to_string(),
            });
        });
        let _ = match result {
            Ok(_) => tx.send(AgentMsg::Done { id }),
            Err(message) => tx.send(AgentMsg::Failed { id, message }),
        };
    });
}

/// Remove an installed agent, then let its status re-derive from disk.
#[cfg(not(target_arch = "wasm32"))]
fn on_uninstall_request(
    trigger: On<BinReceive<AgentsUninstall>>,
    mut status: ResMut<AgentsStatus>,
    mut install_generation: ResMut<AcpInstallGeneration>,
) {
    let id = trigger.event().payload.id.clone();
    let _ = crate::acp_install::uninstall(&id);
    status.0.remove(&id);
    install_generation.bump();
}

/// Fold background-install updates into the session status map.
#[cfg(not(target_arch = "wasm32"))]
fn drain_agent_installs(
    installs: Res<AgentsInstallChannel>,
    mut status: ResMut<AgentsStatus>,
    mut install_generation: ResMut<AcpInstallGeneration>,
) {
    while let Ok(msg) = installs.rx.try_recv() {
        match msg {
            AgentMsg::Progress { id, pct, message } => {
                let text = match pct {
                    Some(p) => format!("{message} ({p}%)"),
                    None => message,
                };
                status.0.insert(id, ("installing".to_string(), text));
            }
            AgentMsg::Done { id } => {
                status.0.remove(&id);
                install_generation.bump();
            }
            AgentMsg::Failed { id, message } => {
                status.0.insert(id, ("error".to_string(), message));
            }
        }
    }
}

/// Push the catalog (with per-agent status) whenever it (re)loads, status changes, or a page
/// requests it.
#[cfg(not(target_arch = "wasm32"))]
fn push_agents(
    catalog: Res<AcpCatalog>,
    status: Res<AgentsStatus>,
    webview_res: Res<AgentsPageWebview>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    if !catalog.is_changed() && !status.is_changed() && !webview_res.is_changed() {
        return;
    }
    let Some(webview) = webview_res.0 else {
        return;
    };
    if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        AGENTS_CATALOG_EVENT,
        &catalog_snapshot(&catalog, &status),
    ));
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn cli_catalog_rows_report_install_state() {
        let rows = cli_agent_entries(|kind| kind == AgentKind::Codex);

        assert_eq!(rows.len(), 3);
        let codex = rows.iter().find(|row| row.id == "cli:codex").unwrap();
        assert_eq!(codex.source, "cli");
        assert_eq!(codex.launch_url, "vmux://agent/codex/cli");
        assert_eq!(codex.status, "installed");
        assert!(!codex.uninstallable);
        assert!(
            rows.iter()
                .filter(|row| row.id != "cli:codex")
                .all(|row| row.status == "available")
        );
    }
}
