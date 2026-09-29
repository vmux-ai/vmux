use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::{
    AgentBrowserGoBack, AgentBrowserGoForward, AgentBrowserHistorySearch,
    AgentBrowserInstallExtension, AgentBrowserNavigate, AgentBrowserScroll, AgentBrowserSnapshot,
    AgentRequest,
};
use vmux_core::ProcessAnchor;

use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
};

pub struct BrowserToolPlugin;

impl Plugin for BrowserToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::from_feature(
            include_str!("feature.ron"),
            "default",
        ))
        .add_systems(Startup, register_agent_policy)
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

fn register_agent_policy(mut commands: Commands) {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/"));
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    commands.spawn((
        Name::new("Browser agent policy"),
        vmux_core::agent::AgentPromptContribution(
            "Use the available vmux browser tools for web access and follow their descriptions. Do not look for a built-in web search or connector discovery. For website visuals, use code-native design or available project assets when no image tool is available."
                .to_string(),
        ),
        vmux_core::agent::AgentDisabledSkillRoot(
            codex_home.join("plugins/cache/openai-bundled/browser"),
        ),
    ));
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
            AgentRequest::encode(&AgentBrowserNavigate {
                url: args.url.clone(),
                pane: args.pane.clone(),
            })
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
            .insert(ToolCommand(AgentRequest::encode(&AgentBrowserGoBack {
                pane: args.pane.clone(),
            })));
    }
}

fn go_forward(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserForwardArgs), AddedTool<BrowserForwardArgs>>,
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
    requests: Query<(Entity, &BrowserHistorySearchArgs), AddedTool<BrowserHistorySearchArgs>>,
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

fn install_extension(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserInstallExtensionArgs), AddedTool<BrowserInstallExtensionArgs>>,
) {
    for (entity, args) in &requests {
        let source = &args.source;
        let command = if source.trim().is_empty() {
            Err("browser_install_extension.source is empty".to_string())
        } else {
            AgentRequest::encode(&AgentBrowserInstallExtension {
                source: source.clone(),
            })
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
            .insert(ToolQuery(AgentRequest::encode(&AgentBrowserSnapshot {
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
            .insert(ToolQuery(AgentRequest::encode(&AgentBrowserScroll {
                pane: BrowserPane::from(target).into(),
                to,
                delta,
                anchor: anchor.map(|anchor| anchor.0),
            })));
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
            &vmux_core::agent::AgentPromptContribution,
            &vmux_core::agent::AgentDisabledSkillRoot,
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
