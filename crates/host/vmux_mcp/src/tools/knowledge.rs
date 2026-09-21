use super::{DispatchTarget, ToolCall, ToolDefinition};
use serde::Deserialize;
use vmux_client::protocol::AgentCommand;

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

pub(super) fn open_vault(call: ToolCall<'_>) -> Result<DispatchTarget, String> {
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

pub(super) fn set_conversation_title(call: ToolCall<'_>) -> Result<DispatchTarget, String> {
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

pub(super) fn search(call: ToolCall<'_>) -> Result<DispatchTarget, String> {
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

pub(super) fn read(call: ToolCall<'_>) -> Result<DispatchTarget, String> {
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

pub(super) fn write(call: ToolCall<'_>) -> Result<DispatchTarget, String> {
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

pub(super) fn vault_status_definition() -> ToolDefinition {
    ToolDefinition {
        name: "vault_status".into(),
        description: "Read the local Vault sync state without connecting, uploading, or discovering remote repositories. Use this first when the user asks to back up, upload, sync, or migrate vmux. If Vault is not connected and the user did not already choose a provider, call request_user_choice with GitHub and Cloud folder. If changes need upload, ask the user to confirm syncing before opening Vault. Never claim data was uploaded until a later status reports no local changes and no commits ahead."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    }
}

pub(super) fn open_vault_definition() -> ToolDefinition {
    ToolDefinition {
        name: "open_vault".into(),
        description: "Open the user-facing Vault page for the final connection or sync confirmation. This tool never uploads by itself. First call vault_status. If the user did not already specify the provider or sync action, call request_user_choice and stop the turn; call open_vault only after the user selects GitHub, Cloud folder, or confirms Sync. The user completes the final repository/folder choice and clicks Create, Use, or Sync in Vault."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "provider": {"enum": ["overview", "github", "cloud_folder"]}
            }
        }),
    }
}

pub(super) fn set_conversation_title_definition() -> ToolDefinition {
    ToolDefinition {
        name: "set_conversation_title".into(),
        description: "Set the agent conversation header to a concise model-written summary without asking permission. Always call first after the first user message to replace the provisional raw-prompt title. On later messages, call first only when the topic materially changes. Use 3 to 7 words, correct spelling and grammar, and never copy the user's prompt verbatim."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["title"],
            "additionalProperties": false,
            "properties": {
                "title": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 120
                }
            }
        }),
    }
}

pub(super) fn write_knowledge_definition() -> ToolDefinition {
    ToolDefinition {
        name: "write_knowledge".into(),
        description: "Create or replace a Markdown note in the user's vmux Knowledge base, then open it beside the conversation. Use this when the user asks to save, copy, or organize information in Knowledge. Provide a relative path under skills/, memories/, projects/, meetings/, or handbook/; omit path to create projects/<title-slug>.md. Never write directly to ~/.vmux/knowledge with shell commands."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["title", "content"],
            "additionalProperties": false,
            "properties": {
                "path": {"type": "string"},
                "title": {"type": "string"},
                "content": {"type": "string"}
            }
        }),
    }
}

pub(super) fn search_knowledge_definition() -> ToolDefinition {
    ToolDefinition {
        name: "search_knowledge".into(),
        description: "Search every Markdown note in the user's vmux Knowledge base. Returns ranked source references as path:line with titles and matching previews. Use this before read_knowledge when the relevant note is unknown. No permission is required."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["query"],
            "additionalProperties": false,
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }
        }),
    }
}

pub(super) fn read_knowledge_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_knowledge".into(),
        description: "Read a Markdown note from the user's vmux Knowledge base by relative path, title, or alias. line is 1-based and defaults to 1; limit defaults to 200 lines. Use source references returned by search_knowledge. No permission is required."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["path"],
            "additionalProperties": false,
            "properties": {
                "path": {"type": "string"},
                "line": {"type": "integer", "minimum": 1},
                "limit": {"type": "integer", "minimum": 1, "maximum": 2000}
            }
        }),
    }
}
