use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery};

use super::{
    DispatchTarget, ToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolSpawner,
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
    fn target(self) -> Result<DispatchTarget, String> {
        if self.url.trim().is_empty() {
            return Err("browser_navigate.url is empty".to_string());
        }
        Ok(DispatchTarget::Command(AgentCommand::BrowserNavigate {
            url: self.url,
            pane: self.pane,
        }))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserPaneArgs {
    pane: Option<String>,
}

impl BrowserPaneArgs {
    fn back(self) -> DispatchTarget {
        DispatchTarget::Command(AgentCommand::BrowserGoBack { pane: self.pane })
    }

    fn forward(self) -> DispatchTarget {
        DispatchTarget::Command(AgentCommand::BrowserGoForward { pane: self.pane })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserHistorySearchArgs {
    query: String,
    limit: Option<u32>,
}

impl BrowserHistorySearchArgs {
    fn target(self) -> Result<DispatchTarget, String> {
        if self.query.trim().is_empty() {
            return Err("browser_history_search.query is empty".to_string());
        }
        Ok(DispatchTarget::Command(
            AgentCommand::BrowserHistorySearch {
                query: self.query,
                limit: self.limit.unwrap_or(20).min(100),
            },
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserInstallExtensionArgs {
    source: String,
}

impl BrowserInstallExtensionArgs {
    fn target(self) -> Result<DispatchTarget, String> {
        if self.source.trim().is_empty() {
            return Err("browser_install_extension.source is empty".to_string());
        }
        Ok(DispatchTarget::Command(
            AgentCommand::BrowserInstallExtension {
                source: self.source,
            },
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserSnapshotArgs {
    target: Option<String>,
}

impl BrowserSnapshotArgs {
    fn target(self, call: &ToolCall) -> DispatchTarget {
        DispatchTarget::Query(AgentQuery::BrowserSnapshot {
            pane: BrowserPane::new(self.target).into_option(),
            anchor: call.anchor,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ScrollTarget {
    Top,
    Bottom,
}

impl ScrollTarget {
    fn into_string(self) -> String {
        match self {
            Self::Top => "top".to_string(),
            Self::Bottom => "bottom".to_string(),
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
    fn target(self, call: &ToolCall) -> DispatchTarget {
        let (to, delta, target) = match self {
            Self::Position(args) => (Some(args.to.into_string()), None, args.target),
            Self::Delta(args) => (None, Some(args.delta), args.target),
        };
        DispatchTarget::Query(AgentQuery::BrowserScroll {
            pane: BrowserPane::new(target).into_option(),
            to,
            delta,
            anchor: call.anchor,
        })
    }
}

struct BrowserPane(Option<String>);

impl BrowserPane {
    fn new(value: Option<String>) -> Self {
        let value = value.and_then(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        });
        Self(value)
    }

    fn into_option(self) -> Option<String> {
        self.0
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
            .and_then(BrowserNavigateArgs::target);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn go_back(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::GoBack) {
        let target = call
            .parse::<BrowserPaneArgs>("browser_go_back")
            .map(BrowserPaneArgs::back);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn go_forward(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::GoForward) {
        let target = call
            .parse::<BrowserPaneArgs>("browser_go_forward")
            .map(BrowserPaneArgs::forward);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn history_search(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::HistorySearch) {
        let target = call
            .parse::<BrowserHistorySearchArgs>("browser_history_search")
            .and_then(BrowserHistorySearchArgs::target);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn install_extension(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::InstallExtension) {
        let target = call
            .parse::<BrowserInstallExtensionArgs>("browser_install_extension")
            .and_then(BrowserInstallExtensionArgs::target);
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
                .map(|args| args.target(call))
        };
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn scroll(mut commands: Commands, calls: ToolCalls<BrowserTool>) {
    for (request, call, _) in calls.matching(BrowserTool::Scroll) {
        let target = call
            .parse::<BrowserScrollArgs>("browser_scroll")
            .map(|args| args.target(call));
        call.finish_dispatch(request, &mut commands, target);
    }
}
