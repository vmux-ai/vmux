use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery};

use super::{
    DispatchTarget, ParsedToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolRequestSet, ToolSpawner,
};

pub(super) struct BrowserToolPlugin;

impl Plugin for BrowserToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Browser))
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

fn register(mut tools: ToolSpawner) {
    tools.spawn_manifest(ToolManifest::<BrowserTool>::from_ron(include_str!(
        "browser.ron"
    )));
}

fn parse(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, tool) in calls.iter() {
        match tool {
            BrowserTool::Navigate => call.parse_into::<BrowserNavigateArgs>(request, &mut commands),
            BrowserTool::GoBack => call.parse_into::<BrowserBackArgs>(request, &mut commands),
            BrowserTool::GoForward => call.parse_into::<BrowserForwardArgs>(request, &mut commands),
            BrowserTool::HistorySearch => {
                call.parse_into::<BrowserHistorySearchArgs>(request, &mut commands)
            }
            BrowserTool::InstallExtension => {
                call.parse_into::<BrowserInstallExtensionArgs>(request, &mut commands)
            }
            BrowserTool::Snapshot => {
                if call
                    .arguments
                    .get("target")
                    .is_some_and(|value| !value.is_null() && !value.is_string())
                {
                    call.finish_dispatch(
                        request,
                        &mut commands,
                        Err("browser_snapshot.target must be a string".to_string()),
                    );
                } else {
                    call.parse_into::<BrowserSnapshotArgs>(request, &mut commands);
                }
            }
            BrowserTool::Scroll => call.parse_into::<BrowserScrollArgs>(request, &mut commands),
        }
    }
}

fn navigate(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BrowserNavigateArgs>),
        Added<ParsedToolCall<BrowserNavigateArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let args = request.args();
        let target = if args.url.trim().is_empty() {
            Err("browser_navigate.url is empty".to_string())
        } else {
            Ok(DispatchTarget::Command(AgentCommand::BrowserNavigate {
                url: args.url.clone(),
                pane: args.pane.clone(),
            }))
        };
        request.finish(entity, &mut commands, target);
    }
}

fn go_back(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BrowserBackArgs>),
        Added<ParsedToolCall<BrowserBackArgs>>,
    >,
) {
    for (entity, request) in &requests {
        request.finish(
            entity,
            &mut commands,
            Ok(DispatchTarget::Command(AgentCommand::BrowserGoBack {
                pane: request.args().pane.clone(),
            })),
        );
    }
}

fn go_forward(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BrowserForwardArgs>),
        Added<ParsedToolCall<BrowserForwardArgs>>,
    >,
) {
    for (entity, request) in &requests {
        request.finish(
            entity,
            &mut commands,
            Ok(DispatchTarget::Command(AgentCommand::BrowserGoForward {
                pane: request.args().pane.clone(),
            })),
        );
    }
}

fn history_search(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BrowserHistorySearchArgs>),
        Added<ParsedToolCall<BrowserHistorySearchArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let args = request.args();
        let target = if args.query.trim().is_empty() {
            Err("browser_history_search.query is empty".to_string())
        } else {
            Ok(DispatchTarget::Command(
                AgentCommand::BrowserHistorySearch {
                    query: args.query.clone(),
                    limit: args.limit.unwrap_or(20).min(100),
                },
            ))
        };
        request.finish(entity, &mut commands, target);
    }
}

fn install_extension(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BrowserInstallExtensionArgs>),
        Added<ParsedToolCall<BrowserInstallExtensionArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let source = &request.args().source;
        let target = if source.trim().is_empty() {
            Err("browser_install_extension.source is empty".to_string())
        } else {
            Ok(DispatchTarget::Command(
                AgentCommand::BrowserInstallExtension {
                    source: source.clone(),
                },
            ))
        };
        request.finish(entity, &mut commands, target);
    }
}

fn snapshot(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BrowserSnapshotArgs>),
        Added<ParsedToolCall<BrowserSnapshotArgs>>,
    >,
) {
    for (entity, request) in &requests {
        request.finish(
            entity,
            &mut commands,
            Ok(DispatchTarget::Query(AgentQuery::BrowserSnapshot {
                pane: BrowserPane::from(request.args().target.clone()).into(),
                anchor: request.anchor(),
            })),
        );
    }
}

fn scroll(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BrowserScrollArgs>),
        Added<ParsedToolCall<BrowserScrollArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let (to, delta, target) = match request.args() {
            BrowserScrollArgs::Position(args) => (Some(args.to.into()), None, args.target.clone()),
            BrowserScrollArgs::Delta(args) => (None, Some(args.delta), args.target.clone()),
        };
        request.finish(
            entity,
            &mut commands,
            Ok(DispatchTarget::Query(AgentQuery::BrowserScroll {
                pane: BrowserPane::from(target).into(),
                to,
                delta,
                anchor: request.anchor(),
            })),
        );
    }
}
