use super::{DispatchTarget, ProtocolTool, ToolCall, ToolManifest};
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::{Commands, On};
use serde::Deserialize;
use vmux_client::protocol::AgentCommand;

pub(super) struct KnowledgeToolsPlugin;

impl Plugin for KnowledgeToolsPlugin {
    fn build(&self, app: &mut App) {
        let mut tools = ToolManifest::from_ron(include_str!("knowledge.ron"));
        tools.protocol(app, "vault_status", ProtocolTool::VaultStatus);
        tools.observe(app, "open_vault", open_vault);
        tools.observe(app, "set_conversation_title", set_conversation_title);
        tools.observe(app, "search_knowledge", search);
        tools.observe(app, "read_knowledge", read);
        tools.observe(app, "write_knowledge", write);
        tools.finish();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum VaultProvider {
    Overview,
    Github,
    CloudFolder,
}

#[derive(Deserialize)]
struct OpenVaultArgs {
    provider: Option<VaultProvider>,
}

#[derive(Deserialize)]
struct SetConversationTitleArgs {
    title: Option<String>,
}

#[derive(Deserialize)]
struct SearchArgs {
    query: Option<String>,
    limit: Option<u64>,
}

#[derive(Deserialize)]
struct ReadArgs {
    path: Option<String>,
    line: Option<u64>,
    limit: Option<u64>,
}

#[derive(Deserialize)]
struct WriteArgs {
    path: Option<String>,
    title: Option<String>,
    content: Option<String>,
}

fn open_vault(trigger: On<ToolCall>, mut commands: Commands) {
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

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn set_conversation_title(trigger: On<ToolCall>, mut commands: Commands) {
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

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn search(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let anchor = call.require_anchor("search_knowledge")?;
        let args: SearchArgs = call.parse("search_knowledge")?;
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

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn read(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let anchor = call.require_anchor("read_knowledge")?;
        let args: ReadArgs = call.parse("read_knowledge")?;
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

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn write(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let anchor = call.require_anchor("write_knowledge")?;
        let args: WriteArgs = call.parse("write_knowledge")?;
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

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

struct Text;

impl Text {
    fn trimmed(value: String) -> Option<String> {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    }

    fn required(value: Option<String>, error: &str) -> Result<String, String> {
        value
            .and_then(Self::trimmed)
            .ok_or_else(|| error.to_string())
    }
}
