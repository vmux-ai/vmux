use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentSpaceCommand, JsonValue};

use super::{
    DispatchTarget, ToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolSpawner,
};

pub(super) struct ParamToolPlugin;

impl Plugin for ParamToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Param))
            .add_systems(Update, dispatch.in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ParamTool {
    OpenCommandBar,
    BrowserNavigate,
    TerminalSend,
    RenameProfile,
    SelectTab,
    UpdateSettings,
    BrowserGoBack,
    BrowserGoForward,
    BrowserHistorySearch,
    BrowserInstallExtension,
    CreateSpace,
    RenameSpace,
    DeleteSpace,
    Notify,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenCommandBarArgs {
    mode: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserNavigateArgs {
    url: String,
    pane: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalSendArgs {
    text: String,
    terminal: Option<String>,
    enter: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameProfileArgs {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectTabArgs {
    index: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateSettingsArgs {
    path: String,
    value: serde_json::Value,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateSpaceArgs {
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameSpaceArgs {
    space_id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteSpaceArgs {
    space_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NotifyArgs {
    title: Option<String>,
    body: Option<String>,
}

impl ParamTool {
    fn target(self, call: &ToolCall) -> Result<DispatchTarget, String> {
        let command = match self {
            Self::OpenCommandBar => {
                let args: OpenCommandBarArgs = call.parse("open_command_bar")?;
                let id = match args.mode.as_deref().unwrap_or("default") {
                    "default" => "browser_open_command_bar",
                    "commands" => "browser_open_commands",
                    "path" => "browser_open_path_bar",
                    other => return Err(format!("unknown command bar mode: {other}")),
                };
                AgentCommand::InvokeCommand {
                    id: id.to_string(),
                    args: JsonValue::Object(Vec::new()),
                }
            }
            Self::BrowserNavigate => {
                let args: BrowserNavigateArgs = call.parse("browser_navigate")?;
                if args.url.trim().is_empty() {
                    return Err("browser_navigate.url is empty".to_string());
                }
                AgentCommand::BrowserNavigate {
                    url: args.url,
                    pane: args.pane,
                }
            }
            Self::TerminalSend => {
                let args: TerminalSendArgs = call.parse("terminal_send")?;
                let text = if args.enter.unwrap_or(false) {
                    format!("{}\r", args.text)
                } else {
                    args.text
                };
                if text.is_empty() {
                    return Err("terminal_send.text is empty".to_string());
                }
                AgentCommand::TerminalSend {
                    text,
                    terminal: args.terminal,
                }
            }
            Self::RenameProfile => {
                let args: RenameProfileArgs = call.parse("rename_profile")?;
                if args.name.trim().is_empty() {
                    return Err("rename_profile.name is empty".to_string());
                }
                AgentCommand::RenameProfile { name: args.name }
            }
            Self::SelectTab => {
                let args: SelectTabArgs = call.parse("select_tab")?;
                if !(1..=8).contains(&args.index) {
                    return Err(format!(
                        "select_tab.index must be between 1 and 8, got {}",
                        args.index
                    ));
                }
                AgentCommand::InvokeCommand {
                    id: format!("tab_select_{}", args.index),
                    args: JsonValue::Object(Vec::new()),
                }
            }
            Self::UpdateSettings => {
                let args: UpdateSettingsArgs = call.parse("update_settings")?;
                if args.path.trim().is_empty() {
                    return Err("update_settings.path is empty".to_string());
                }
                AgentCommand::UpdateSettings {
                    path: args.path,
                    value: JsonValue::from(args.value),
                }
            }
            Self::BrowserGoBack => {
                let args: BrowserPaneArgs = call.parse("browser_go_back")?;
                AgentCommand::BrowserGoBack { pane: args.pane }
            }
            Self::BrowserGoForward => {
                let args: BrowserPaneArgs = call.parse("browser_go_forward")?;
                AgentCommand::BrowserGoForward { pane: args.pane }
            }
            Self::BrowserHistorySearch => {
                let args: BrowserHistorySearchArgs = call.parse("browser_history_search")?;
                if args.query.trim().is_empty() {
                    return Err("browser_history_search.query is empty".to_string());
                }
                AgentCommand::BrowserHistorySearch {
                    query: args.query,
                    limit: args.limit.unwrap_or(20).min(100),
                }
            }
            Self::BrowserInstallExtension => {
                let args: BrowserInstallExtensionArgs = call.parse("browser_install_extension")?;
                if args.source.trim().is_empty() {
                    return Err("browser_install_extension.source is empty".to_string());
                }
                AgentCommand::BrowserInstallExtension {
                    source: args.source,
                }
            }
            Self::CreateSpace => {
                let args: CreateSpaceArgs = call.parse("create_space")?;
                AgentCommand::SpaceCommand(AgentSpaceCommand::Create {
                    name: args.name.filter(|name| !name.trim().is_empty()),
                })
            }
            Self::RenameSpace => {
                let args: RenameSpaceArgs = call.parse("rename_space")?;
                if args.space_id.trim().is_empty() {
                    return Err("rename_space.space_id is empty".to_string());
                }
                if args.name.trim().is_empty() {
                    return Err("rename_space.name is empty".to_string());
                }
                AgentCommand::SpaceCommand(AgentSpaceCommand::Rename {
                    space_id: args.space_id,
                    name: args.name,
                })
            }
            Self::DeleteSpace => {
                let args: DeleteSpaceArgs = call.parse("delete_space")?;
                if args.space_id.trim().is_empty() {
                    return Err("delete_space.space_id is empty".to_string());
                }
                AgentCommand::SpaceCommand(AgentSpaceCommand::Delete {
                    space_id: args.space_id,
                })
            }
            Self::Notify => {
                let args: NotifyArgs = call.parse("notify")?;
                AgentCommand::Notify {
                    title: args.title,
                    body: args.body,
                }
            }
        };
        Ok(DispatchTarget::Command(command))
    }
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<ParamTool>::from_ron(include_str!("param.ron"));
    tools.spawn_manifest(manifest);
}

fn dispatch(mut commands: Commands, calls: ToolCalls<ParamTool>) {
    for (request, call, tool) in calls.iter() {
        call.finish_dispatch(request, &mut commands, tool.target(call));
    }
}
