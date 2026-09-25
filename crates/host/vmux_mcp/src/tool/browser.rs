use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery};

use super::{
    McpToolPlugin, ToolCall, ToolCalls, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolQuery,
    ToolRequestSet,
};

pub(super) struct BrowserToolPlugin;

impl Plugin for BrowserToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<BrowserTool>::new(include_str!(
            "browser.ron"
        )))
        .add_systems(Update, parse.in_set(ToolRequestSet))
        .add_systems(
            Update,
            (
                navigate,
                go_back,
                go_forward,
                history_search,
                install_extension,
                snapshot,
                scroll,
            )
                .in_set(ToolDispatchSet),
        );
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
    #[serde(rename = "browser_snapshot")]
    Snapshot,
    #[serde(rename = "browser_scroll")]
    Scroll,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserNavigateArgs {
    url: String,
    pane: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserBackArgs {
    pane: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserForwardArgs {
    pane: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserHistorySearchArgs {
    query: String,
    limit: Option<u32>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserInstallExtensionArgs {
    source: String,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserSnapshotArgs {
    target: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ScrollTarget {
    Top,
    Bottom,
}

impl From<ScrollTarget> for String {
    fn from(target: ScrollTarget) -> Self {
        match target {
            ScrollTarget::Top => "top".to_string(),
            ScrollTarget::Bottom => "bottom".to_string(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserScrollPositionArgs {
    to: ScrollTarget,
    target: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserScrollDeltaArgs {
    delta: i32,
    target: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(untagged)]
enum BrowserScrollArgs {
    Position(BrowserScrollPositionArgs),
    Delta(BrowserScrollDeltaArgs),
}

struct BrowserPane(Option<String>);

impl From<Option<String>> for BrowserPane {
    fn from(value: Option<String>) -> Self {
        let value = value.and_then(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        });
        Self(value)
    }
}

impl From<BrowserPane> for Option<String> {
    fn from(pane: BrowserPane) -> Self {
        pane.0
    }
}

fn parse(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, tool) in calls.iter() {
        let parsed = match tool {
            BrowserTool::Navigate => call.parse::<BrowserNavigateArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            BrowserTool::GoBack => call.parse::<BrowserBackArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            BrowserTool::GoForward => call.parse::<BrowserForwardArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            BrowserTool::HistorySearch => call.parse::<BrowserHistorySearchArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            BrowserTool::InstallExtension => {
                call.parse::<BrowserInstallExtensionArgs>().map(|args| {
                    commands.entity(request).insert(args);
                })
            }
            BrowserTool::Snapshot => {
                if call
                    .arguments
                    .get("target")
                    .is_some_and(|value| !value.is_null() && !value.is_string())
                {
                    Err("browser_snapshot.target must be a string".to_string())
                } else {
                    call.parse::<BrowserSnapshotArgs>().map(|args| {
                        commands.entity(request).insert(args);
                    })
                }
            }
            BrowserTool::Scroll => call.parse::<BrowserScrollArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
        };
        if let Err(message) = parsed {
            commands.entity(request).insert(ToolDispatchError(message));
        }
    }
}

fn navigate(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserNavigateArgs), (With<ToolCall>, Added<BrowserNavigateArgs>)>,
) {
    for (entity, args) in &requests {
        let command = if args.url.trim().is_empty() {
            Err("browser_navigate.url is empty".to_string())
        } else {
            Ok(AgentCommand::BrowserNavigate {
                url: args.url.clone(),
                pane: args.pane.clone(),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn go_back(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserBackArgs), (With<ToolCall>, Added<BrowserBackArgs>)>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(Ok(AgentCommand::BrowserGoBack {
                pane: args.pane.clone(),
            })));
    }
}

fn go_forward(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserForwardArgs), (With<ToolCall>, Added<BrowserForwardArgs>)>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(Ok(AgentCommand::BrowserGoForward {
                pane: args.pane.clone(),
            })));
    }
}

fn history_search(
    mut commands: Commands,
    requests: Query<
        (Entity, &BrowserHistorySearchArgs),
        (With<ToolCall>, Added<BrowserHistorySearchArgs>),
    >,
) {
    for (entity, args) in &requests {
        let command = if args.query.trim().is_empty() {
            Err("browser_history_search.query is empty".to_string())
        } else {
            Ok(AgentCommand::BrowserHistorySearch {
                query: args.query.clone(),
                limit: args.limit.unwrap_or(20).min(100),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn install_extension(
    mut commands: Commands,
    requests: Query<
        (Entity, &BrowserInstallExtensionArgs),
        (With<ToolCall>, Added<BrowserInstallExtensionArgs>),
    >,
) {
    for (entity, args) in &requests {
        let source = &args.source;
        let command = if source.trim().is_empty() {
            Err("browser_install_extension.source is empty".to_string())
        } else {
            Ok(AgentCommand::BrowserInstallExtension {
                source: source.clone(),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn snapshot(
    mut commands: Commands,
    requests: Query<(Entity, &ToolCall, &BrowserSnapshotArgs), Added<BrowserSnapshotArgs>>,
) {
    for (entity, call, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::BrowserSnapshot {
                pane: BrowserPane::from(args.target.clone()).into(),
                anchor: call.anchor,
            })));
    }
}

fn scroll(
    mut commands: Commands,
    requests: Query<(Entity, &ToolCall, &BrowserScrollArgs), Added<BrowserScrollArgs>>,
) {
    for (entity, call, args) in &requests {
        let (to, delta, target) = match args {
            BrowserScrollArgs::Position(args) => (Some(args.to.into()), None, args.target.clone()),
            BrowserScrollArgs::Delta(args) => (None, Some(args.delta), args.target.clone()),
        };
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::BrowserScroll {
                pane: BrowserPane::from(target).into(),
                to,
                delta,
                anchor: call.anchor,
            })));
    }
}
