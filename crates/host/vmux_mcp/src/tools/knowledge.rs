use super::{
    DispatchTarget, ProtocolTool, ToolCall, ToolCalls, ToolDispatchSet, ToolExecution,
    ToolManifest, ToolRegistrationSet, ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::AgentCommand;

pub(super) struct KnowledgeToolsPlugin;

impl Plugin for KnowledgeToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Knowledge))
            .add_systems(
                Update,
                (
                    vault_status,
                    open_vault,
                    set_conversation_title,
                    search,
                    read,
                    write,
                )
                    .in_set(ToolDispatchSet),
            );
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum KnowledgeTool {
    VaultStatus,
    OpenVault,
    SetConversationTitle,
    SearchKnowledge,
    ReadKnowledge,
    WriteKnowledge,
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<KnowledgeTool>::from_ron(include_str!("knowledge.ron"));
    tools.spawn_manifest(manifest);
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum VaultProvider {
    Overview,
    Github,
    CloudFolder,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenVaultArgs {
    provider: Option<VaultProvider>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetConversationTitleArgs {
    title: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchKnowledgeArgs {
    query: String,
    limit: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadKnowledgeArgs {
    path: String,
    line: Option<u64>,
    limit: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteKnowledgeArgs {
    path: Option<String>,
    title: String,
    content: String,
}

fn vault_status(mut commands: Commands, calls: ToolCalls<KnowledgeTool>) {
    for (request, call, _) in calls.matching(KnowledgeTool::VaultStatus) {
        call.finish(
            request,
            &mut commands,
            Ok(ToolExecution::Protocol {
                tool: ProtocolTool::VaultStatus,
                arguments: call.arguments.clone(),
                anchor: call.anchor,
            }),
        );
    }
}

fn open_vault(mut commands: Commands, calls: ToolCalls<KnowledgeTool>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let anchor = call.require_anchor("open_vault")?;
        let args: OpenVaultArgs = call.parse("open_vault")?;
        let url = match args.provider.unwrap_or(VaultProvider::Overview) {
            VaultProvider::Overview => "vmux://vault/",
            VaultProvider::Github => "vmux://vault/?provider=github",
            VaultProvider::CloudFolder => "vmux://vault/?provider=cloud_folder",
        };
        Ok(DispatchTarget::Command(AgentCommand::OpenBeside {
            anchor,
            direction: None,
            url: url.to_string(),
            focus: true,
        }))
    }

    for (request, call, _) in calls.matching(KnowledgeTool::OpenVault) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn set_conversation_title(mut commands: Commands, calls: ToolCalls<KnowledgeTool>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let anchor = call.require_anchor("set_conversation_title")?;
        let args: SetConversationTitleArgs = call.parse("set_conversation_title")?;
        let title = Text::required(args.title, "set_conversation_title.title is empty")?;
        if title.chars().count() > 120 {
            return Err("set_conversation_title.title exceeds 120 characters".to_string());
        }
        Ok(DispatchTarget::Command(
            AgentCommand::SetConversationTitle { anchor, title },
        ))
    }

    for (request, call, _) in calls.matching(KnowledgeTool::SetConversationTitle) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn search(mut commands: Commands, calls: ToolCalls<KnowledgeTool>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let anchor = call.require_anchor("search_knowledge")?;
        let args: SearchKnowledgeArgs = call.parse("search_knowledge")?;
        let query = Text::required(args.query, "search_knowledge.query is empty")?;
        let limit = args.limit.unwrap_or(20);
        if !(1..=100).contains(&limit) {
            return Err("search_knowledge.limit must be between 1 and 100".to_string());
        }
        Ok(DispatchTarget::Command(AgentCommand::SearchKnowledge {
            anchor,
            query,
            limit: limit as u16,
        }))
    }

    for (request, call, _) in calls.matching(KnowledgeTool::SearchKnowledge) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn read(mut commands: Commands, calls: ToolCalls<KnowledgeTool>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let anchor = call.require_anchor("read_knowledge")?;
        let args: ReadKnowledgeArgs = call.parse("read_knowledge")?;
        let path = Text::required(args.path, "read_knowledge.path is empty")?;
        let line = args.line.unwrap_or(1);
        let limit = args.limit.unwrap_or(200);
        if line == 0 || line > u32::MAX as u64 {
            return Err("read_knowledge.line must be at least 1".to_string());
        }
        if !(1..=2_000).contains(&limit) {
            return Err("read_knowledge.limit must be between 1 and 2000".to_string());
        }
        Ok(DispatchTarget::Command(AgentCommand::ReadKnowledge {
            anchor,
            path,
            line: line as u32,
            limit: limit as u32,
        }))
    }

    for (request, call, _) in calls.matching(KnowledgeTool::ReadKnowledge) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn write(mut commands: Commands, calls: ToolCalls<KnowledgeTool>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let anchor = call.require_anchor("write_knowledge")?;
        let args: WriteKnowledgeArgs = call.parse("write_knowledge")?;
        let path = args.path.and_then(Text::trimmed);
        let title = Text::required(args.title, "write_knowledge.title is empty")?;
        let content = Text::required(args.content, "write_knowledge.content is empty")?;
        Ok(DispatchTarget::Command(AgentCommand::WriteKnowledge {
            anchor,
            path,
            title,
            content,
        }))
    }

    for (request, call, _) in calls.matching(KnowledgeTool::WriteKnowledge) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

struct Text;

impl Text {
    fn trimmed(value: String) -> Option<String> {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    }

    fn required(value: String, error: &str) -> Result<String, String> {
        Self::trimmed(value).ok_or_else(|| error.to_string())
    }
}
