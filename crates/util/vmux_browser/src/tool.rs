use bevy::ecs::system::Commands;
use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::AgentRequest;
use vmux_ecs::ProcessAnchor;
use vmux_feature::manifest::FeaturePlugin;

use crate::host::{
    AgentBrowserGoBack, AgentBrowserGoForward, AgentBrowserHistorySearch, AgentBrowserNavigate,
    AgentBrowserScroll, AgentBrowserSnapshot,
};

use vmux_tool::{AddedTool, ToolCommand, ToolDispatchSet, ToolQuery};

pub struct BrowserToolPlugin;

impl Plugin for BrowserToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_systems(Startup, register_agent_policy)
            .add_systems(
                Update,
                (
                    navigate,
                    go_back,
                    go_forward,
                    history_search,
                    snapshot,
                    scroll,
                )
                    .in_set(ToolDispatchSet),
            );
    }
}

fn register_agent_policy(mut commands: Commands) {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/"));
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    commands.spawn((
        Name::new("Browser agent policy"),
        vmux_ecs::agent::AgentPromptContribution(
            "Use the available vmux browser tools for web access and follow their descriptions. Do not look for a built-in web search or connector discovery. For website visuals, use code-native design or available project assets when no image tool is available."
                .to_string(),
        ),
        vmux_ecs::agent::AgentDisabledSkillRoot(
            codex_home.join("plugins/cache/openai-bundled/browser"),
        ),
    ));
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct NavigateTool {
    url: String,
    pane: Option<String>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoBackTool {
    pane: Option<String>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoForwardTool {
    pane: Option<String>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistorySearchTool {
    query: String,
    limit: Option<u32>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotTool {
    target: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ScrollTarget {
    Top,
    Bottom,
}

impl ScrollTarget {
    fn as_str(self) -> &'static str {
        match self {
            ScrollTarget::Top => "top",
            ScrollTarget::Bottom => "bottom",
        }
    }
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScrollTool {
    to: Option<ScrollTarget>,
    delta: Option<i32>,
    target: Option<String>,
}

impl SnapshotTool {
    fn normalize_pane(value: Option<String>) -> Option<String> {
        value.and_then(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        })
    }
}

fn navigate(
    mut commands: Commands,
    requests: Query<(Entity, &NavigateTool), AddedTool<NavigateTool>>,
) {
    for (entity, args) in &requests {
        let command = if args.url.trim().is_empty() {
            Err("browser_navigate.url is empty".to_string())
        } else {
            AgentRequest::encode(&AgentBrowserNavigate {
                url: args.url.clone(),
                pane: args.pane.clone(),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn go_back(mut commands: Commands, requests: Query<(Entity, &GoBackTool), AddedTool<GoBackTool>>) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(AgentRequest::encode(&AgentBrowserGoBack {
                pane: args.pane.clone(),
            })));
    }
}

fn go_forward(
    mut commands: Commands,
    requests: Query<(Entity, &GoForwardTool), AddedTool<GoForwardTool>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(AgentRequest::encode(&AgentBrowserGoForward {
                pane: args.pane.clone(),
            })));
    }
}

fn history_search(
    mut commands: Commands,
    requests: Query<(Entity, &HistorySearchTool), AddedTool<HistorySearchTool>>,
) {
    for (entity, args) in &requests {
        let command = if args.query.trim().is_empty() {
            Err("browser_history_search.query is empty".to_string())
        } else {
            AgentRequest::encode(&AgentBrowserHistorySearch {
                query: args.query.clone(),
                limit: args.limit.unwrap_or(20).min(100),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn snapshot(
    mut commands: Commands,
    requests: Query<(Entity, Option<&ProcessAnchor>, &SnapshotTool), Added<SnapshotTool>>,
) {
    for (entity, anchor, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(AgentRequest::encode(&AgentBrowserSnapshot {
                pane: SnapshotTool::normalize_pane(args.target.clone()),
                anchor: anchor.map(|anchor| anchor.0),
            })));
    }
}

fn scroll(
    mut commands: Commands,
    requests: Query<(Entity, Option<&ProcessAnchor>, &ScrollTool), Added<ScrollTool>>,
) {
    for (entity, anchor, args) in &requests {
        let query = if args.to.is_some() == args.delta.is_some() {
            Err("browser_scroll requires exactly one of `to` or `delta`".to_string())
        } else {
            AgentRequest::encode(&AgentBrowserScroll {
                pane: SnapshotTool::normalize_pane(args.target.clone()),
                to: args.to.map(|target| target.as_str().to_string()),
                delta: args.delta,
                anchor: anchor.map(|anchor| anchor.0),
            })
        };
        commands.entity(entity).insert(ToolQuery(query));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_registers_its_agent_policy() {
        let mut app = App::new();
        app.add_plugins(BrowserToolPlugin);
        app.update();

        let mut policies = app.world_mut().query::<(
            &vmux_ecs::agent::AgentPromptContribution,
            &vmux_ecs::agent::AgentDisabledSkillRoot,
        )>();
        let policies = policies.iter(app.world()).collect::<Vec<_>>();
        assert_eq!(policies.len(), 1);
        assert!(policies[0].0.0.contains("vmux browser tools"));
        assert!(
            policies[0]
                .1
                .0
                .ends_with("plugins/cache/openai-bundled/browser")
        );
    }
}
