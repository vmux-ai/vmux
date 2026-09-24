use super::{
    DispatchTarget, McpToolPlugin, ProtocolTool, ToolCall, ToolCalls, ToolDispatchResult,
    ToolDispatchSet, ToolExecution, ToolOutcome, ToolRequestSet,
};
use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::AgentCommand;

pub(super) struct KnowledgeToolPlugin;

impl Plugin for KnowledgeToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<KnowledgeTool>::new(include_str!(
            "knowledge.ron"
        )))
        .add_systems(Update, parse.in_set(ToolRequestSet))
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

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum VaultProvider {
    Overview,
    Github,
    CloudFolder,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenVaultArgs {
    provider: Option<VaultProvider>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetConversationTitleArgs {
    title: String,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchKnowledgeArgs {
    query: String,
    limit: Option<u64>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadKnowledgeArgs {
    path: String,
    line: Option<u64>,
    limit: Option<u64>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteKnowledgeArgs {
    path: Option<String>,
    title: String,
    content: String,
}

fn vault_status(mut commands: Commands, calls: ToolCalls<KnowledgeTool>) {
    for (request, call, _) in calls.matching(KnowledgeTool::VaultStatus) {
        commands
            .entity(request)
            .insert(ToolOutcome(Ok(ToolExecution::Protocol {
                tool: ProtocolTool::VaultStatus,
                arguments: call.arguments.clone(),
                anchor: call.anchor,
            })));
    }
}

fn parse(mut commands: Commands, calls: ToolCalls<KnowledgeTool>) {
    for (request, call, tool) in calls.iter() {
        let parsed = match tool {
            KnowledgeTool::VaultStatus => continue,
            KnowledgeTool::OpenVault => call.parse::<OpenVaultArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            KnowledgeTool::SetConversationTitle => {
                call.parse::<SetConversationTitleArgs>().map(|args| {
                    commands.entity(request).insert(args);
                })
            }
            KnowledgeTool::SearchKnowledge => call.parse::<SearchKnowledgeArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            KnowledgeTool::ReadKnowledge => call.parse::<ReadKnowledgeArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            KnowledgeTool::WriteKnowledge => call.parse::<WriteKnowledgeArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
        };
        if let Err(message) = parsed {
            commands
                .entity(request)
                .insert(ToolDispatchResult(Err(message)));
        }
    }
}

fn open_vault(
    mut commands: Commands,
    requests: Query<(Entity, &ToolCall, &OpenVaultArgs), Added<OpenVaultArgs>>,
) {
    for (entity, call, args) in &requests {
        let target = call.require_anchor().map(|anchor| {
            let url = match args.provider.as_ref().unwrap_or(&VaultProvider::Overview) {
                VaultProvider::Overview => "vmux://vault/",
                VaultProvider::Github => "vmux://vault/?provider=github",
                VaultProvider::CloudFolder => "vmux://vault/?provider=cloud_folder",
            };
            DispatchTarget::Command(AgentCommand::OpenBeside {
                anchor,
                direction: None,
                url: url.to_string(),
                focus: true,
            })
        });
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}

fn set_conversation_title(
    mut commands: Commands,
    requests: Query<
        (Entity, &ToolCall, &SetConversationTitleArgs),
        Added<SetConversationTitleArgs>,
    >,
) {
    for (entity, call, args) in &requests {
        let target = call.require_anchor().and_then(|anchor| {
            let title =
                Text::required(args.title.clone(), "set_conversation_title.title is empty")?;
            if title.chars().count() > 120 {
                return Err("set_conversation_title.title exceeds 120 characters".to_string());
            }
            Ok(DispatchTarget::Command(
                AgentCommand::SetConversationTitle { anchor, title },
            ))
        });
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}

fn search(
    mut commands: Commands,
    requests: Query<(Entity, &ToolCall, &SearchKnowledgeArgs), Added<SearchKnowledgeArgs>>,
) {
    for (entity, call, args) in &requests {
        let target = call.require_anchor().and_then(|anchor| {
            let query = Text::required(args.query.clone(), "search_knowledge.query is empty")?;
            let limit = args.limit.unwrap_or(20);
            if !(1..=100).contains(&limit) {
                return Err("search_knowledge.limit must be between 1 and 100".to_string());
            }
            Ok(DispatchTarget::Command(AgentCommand::SearchKnowledge {
                anchor,
                query,
                limit: limit as u16,
            }))
        });
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}

fn read(
    mut commands: Commands,
    requests: Query<(Entity, &ToolCall, &ReadKnowledgeArgs), Added<ReadKnowledgeArgs>>,
) {
    for (entity, call, args) in &requests {
        let target = call.require_anchor().and_then(|anchor| {
            let path = Text::required(args.path.clone(), "read_knowledge.path is empty")?;
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
        });
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}

fn write(
    mut commands: Commands,
    requests: Query<(Entity, &ToolCall, &WriteKnowledgeArgs), Added<WriteKnowledgeArgs>>,
) {
    for (entity, call, args) in &requests {
        let target = call.require_anchor().and_then(|anchor| {
            let path = args.path.clone().and_then(Text::trimmed);
            let title = Text::required(args.title.clone(), "write_knowledge.title is empty")?;
            let content = Text::required(args.content.clone(), "write_knowledge.content is empty")?;
            Ok(DispatchTarget::Command(AgentCommand::WriteKnowledge {
                anchor,
                path,
                title,
                content,
            }))
        });
        commands.entity(entity).insert(ToolDispatchResult(target));
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
