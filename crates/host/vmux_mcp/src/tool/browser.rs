use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery};

use super::{
    DispatchTarget, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet, ToolSpawner,
};

pub(super) struct BrowserToolPlugin;

impl Plugin for BrowserToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Browser))
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserNavigateArgs {
    url: String,
    pane: Option<String>,
}

impl BrowserNavigateArgs {
    fn command(self) -> Result<AgentCommand, String> {
        if self.url.trim().is_empty() {
            return Err("browser_navigate.url is empty".to_string());
        }
        Ok(AgentCommand::BrowserNavigate {
            url: self.url,
            pane: self.pane,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserPaneArgs {
    pane: Option<String>,
}

impl BrowserPaneArgs {
    fn back_command(self) -> AgentCommand {
        AgentCommand::BrowserGoBack { pane: self.pane }
    }

    fn forward_command(self) -> AgentCommand {
        AgentCommand::BrowserGoForward { pane: self.pane }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserHistorySearchArgs {
    query: String,
    limit: Option<u32>,
}

impl BrowserHistorySearchArgs {
    fn command(self) -> Result<AgentCommand, String> {
        if self.query.trim().is_empty() {
            return Err("browser_history_search.query is empty".to_string());
        }
        Ok(AgentCommand::BrowserHistorySearch {
            query: self.query,
            limit: self.limit.unwrap_or(20).min(100),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserInstallExtensionArgs {
    source: String,
}

impl BrowserInstallExtensionArgs {
    fn command(self) -> Result<AgentCommand, String> {
        if self.source.trim().is_empty() {
            return Err("browser_install_extension.source is empty".to_string());
        }
        Ok(AgentCommand::BrowserInstallExtension {
            source: self.source,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserSnapshotArgs {
    target: Option<String>,
}

impl BrowserSnapshotArgs {
    fn query(self, anchor: Option<vmux_client::protocol::ProcessId>) -> AgentQuery {
        AgentQuery::BrowserSnapshot {
            pane: BrowserPane::from(self.target).into(),
            anchor,
        }
    }
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
#[serde(untagged)]
enum BrowserScrollArgs {
    Position(BrowserScrollPositionArgs),
    Delta(BrowserScrollDeltaArgs),
}

impl BrowserScrollArgs {
    fn query(self, anchor: Option<vmux_client::protocol::ProcessId>) -> AgentQuery {
        let (to, delta, target) = match self {
            Self::Position(args) => (Some(args.to.into()), None, args.target),
            Self::Delta(args) => (None, Some(args.delta), args.target),
        };
        AgentQuery::BrowserScroll {
            pane: BrowserPane::from(target).into(),
            to,
            delta,
            anchor,
        }
    }
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

fn navigate(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::Navigate) {
        let target = call
            .parse::<BrowserNavigateArgs>("browser_navigate")
            .and_then(BrowserNavigateArgs::command)
            .map(DispatchTarget::Command);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn go_back(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::GoBack) {
        let target = call
            .parse::<BrowserPaneArgs>("browser_go_back")
            .map(BrowserPaneArgs::back_command)
            .map(DispatchTarget::Command);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn go_forward(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::GoForward) {
        let target = call
            .parse::<BrowserPaneArgs>("browser_go_forward")
            .map(BrowserPaneArgs::forward_command)
            .map(DispatchTarget::Command);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn history_search(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::HistorySearch) {
        let target = call
            .parse::<BrowserHistorySearchArgs>("browser_history_search")
            .and_then(BrowserHistorySearchArgs::command)
            .map(DispatchTarget::Command);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn install_extension(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::InstallExtension) {
        let target = call
            .parse::<BrowserInstallExtensionArgs>("browser_install_extension")
            .and_then(BrowserInstallExtensionArgs::command)
            .map(DispatchTarget::Command);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn snapshot(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::Snapshot) {
        let target = if call
            .arguments
            .get("target")
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            Err("browser_snapshot.target must be a string".to_string())
        } else {
            call.parse::<BrowserSnapshotArgs>("browser_snapshot")
                .map(|args| DispatchTarget::Query(args.query(call.anchor)))
        };
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn scroll(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::Scroll) {
        let target = call
            .parse::<BrowserScrollArgs>("browser_scroll")
            .map(|args| DispatchTarget::Query(args.query(call.anchor)));
        call.finish_dispatch(request, &mut commands, target);
    }
}
