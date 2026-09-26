use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::{
    AgentBrowserHistorySearch, AgentBrowserHistoryStep, AgentBrowserInstallExtension,
    AgentBrowserNavigate, AgentCommand, AgentQuery,
};
use vmux_core::ProcessAnchor;

use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
};

pub struct BrowserToolPlugin;

impl Plugin for BrowserToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::new(include_str!("tool.ron")))
            .register_tool::<BrowserNavigateArgs>("browser_navigate")
            .register_tool::<BrowserBackArgs>("browser_go_back")
            .register_tool::<BrowserForwardArgs>("browser_go_forward")
            .register_tool::<BrowserHistorySearchArgs>("browser_history_search")
            .register_tool::<BrowserInstallExtensionArgs>("browser_install_extension")
            .register_tool::<BrowserSnapshotArgs>("browser_snapshot")
            .register_tool::<BrowserScrollArgs>("browser_scroll")
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

fn navigate(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserNavigateArgs), AddedTool<BrowserNavigateArgs>>,
) {
    for (entity, args) in &requests {
        let command = if args.url.trim().is_empty() {
            Err("browser_navigate.url is empty".to_string())
        } else {
            Ok(AgentCommand::BrowserNavigate(AgentBrowserNavigate {
                url: args.url.clone(),
                pane: args.pane.clone(),
            }))
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn go_back(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserBackArgs), AddedTool<BrowserBackArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(Ok(AgentCommand::BrowserGoBack(
                AgentBrowserHistoryStep {
                    pane: args.pane.clone(),
                },
            ))));
    }
}

fn go_forward(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserForwardArgs), AddedTool<BrowserForwardArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(Ok(AgentCommand::BrowserGoForward(
                AgentBrowserHistoryStep {
                    pane: args.pane.clone(),
                },
            ))));
    }
}

fn history_search(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserHistorySearchArgs), AddedTool<BrowserHistorySearchArgs>>,
) {
    for (entity, args) in &requests {
        let command = if args.query.trim().is_empty() {
            Err("browser_history_search.query is empty".to_string())
        } else {
            Ok(AgentCommand::BrowserHistorySearch(
                AgentBrowserHistorySearch {
                    query: args.query.clone(),
                    limit: args.limit.unwrap_or(20).min(100),
                },
            ))
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn install_extension(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserInstallExtensionArgs), AddedTool<BrowserInstallExtensionArgs>>,
) {
    for (entity, args) in &requests {
        let source = &args.source;
        let command = if source.trim().is_empty() {
            Err("browser_install_extension.source is empty".to_string())
        } else {
            Ok(AgentCommand::BrowserInstallExtension(
                AgentBrowserInstallExtension {
                    source: source.clone(),
                },
            ))
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn snapshot(
    mut commands: Commands,
    requests: Query<
        (Entity, Option<&ProcessAnchor>, &BrowserSnapshotArgs),
        Added<BrowserSnapshotArgs>,
    >,
) {
    for (entity, anchor, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::BrowserSnapshot {
                pane: BrowserPane::from(args.target.clone()).into(),
                anchor: anchor.map(|anchor| anchor.0),
            })));
    }
}

fn scroll(
    mut commands: Commands,
    requests: Query<(Entity, Option<&ProcessAnchor>, &BrowserScrollArgs), Added<BrowserScrollArgs>>,
) {
    for (entity, anchor, args) in &requests {
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
                anchor: anchor.map(|anchor| anchor.0),
            })));
    }
}
