use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::AgentCommand;

use super::{
    DispatchTarget, ToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolSpawner,
};

pub(super) struct BrowserToolPlugin;

impl Plugin for BrowserToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Browser))
            .add_systems(Update, dispatch.in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
enum BrowserTool {
    #[serde(rename = "browser_navigate")]
    Navigate,
    #[serde(rename = "browser_go_back")]
    GoBack,
    #[serde(rename = "browser_go_forward")]
    GoForward,
    #[serde(rename = "browser_history_search")]
    HistorySearch,
    #[serde(rename = "browser_install_extension")]
    InstallExtension,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserNavigateArgs {
    url: String,
    pane: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserPaneArgs {
    pane: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserHistorySearchArgs {
    query: String,
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserInstallExtensionArgs {
    source: String,
}

impl BrowserTool {
    fn target(self, call: &ToolCall) -> Result<DispatchTarget, String> {
        let command = match self {
            Self::Navigate => {
                let args: BrowserNavigateArgs = call.parse("browser_navigate")?;
                if args.url.trim().is_empty() {
                    return Err("browser_navigate.url is empty".to_string());
                }
                AgentCommand::BrowserNavigate {
                    url: args.url,
                    pane: args.pane,
                }
            }
            Self::GoBack => {
                let args: BrowserPaneArgs = call.parse("browser_go_back")?;
                AgentCommand::BrowserGoBack { pane: args.pane }
            }
            Self::GoForward => {
                let args: BrowserPaneArgs = call.parse("browser_go_forward")?;
                AgentCommand::BrowserGoForward { pane: args.pane }
            }
            Self::HistorySearch => {
                let args: BrowserHistorySearchArgs = call.parse("browser_history_search")?;
                if args.query.trim().is_empty() {
                    return Err("browser_history_search.query is empty".to_string());
                }
                AgentCommand::BrowserHistorySearch {
                    query: args.query,
                    limit: args.limit.unwrap_or(20).min(100),
                }
            }
            Self::InstallExtension => {
                let args: BrowserInstallExtensionArgs = call.parse("browser_install_extension")?;
                if args.source.trim().is_empty() {
                    return Err("browser_install_extension.source is empty".to_string());
                }
                AgentCommand::BrowserInstallExtension {
                    source: args.source,
                }
            }
        };
        Ok(DispatchTarget::Command(command))
    }
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<BrowserTool>::from_ron(include_str!("browser.ron"));
    tools.spawn_manifest(manifest);
}

fn dispatch(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, tool) in calls.iter() {
        call.finish_dispatch(request, &mut commands, tool.target(call));
    }
}
