use crate::prompt_media::ChatAttachment;

#[vmux_api::contract(Eq)]
pub enum ChatBlock {
    Text(String),
    Thinking(String),
    ToolUse {
        call_id: String,
        name: String,
        args: String,
        parent_call_id: Option<String>,
    },
    Subagent(Box<ChatSubagent>),
    Diff {
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    Plan {
        steps: Vec<ChatPlanStep>,
    },
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
    Reconnect {
        attempt: u32,
        total: u32,
    },
}

#[vmux_api::contract(Eq)]
pub struct ChatSubagent {
    pub call_id: String,
    pub provider: String,
    pub title: String,
    pub status: String,
    pub activity: String,
    pub agent_name: Option<String>,
    pub thread_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub child_thread_ids: Vec<String>,
    pub parent_call_id: Option<String>,
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub raw_input: String,
}

#[vmux_api::contract(Eq)]
pub struct ChatPlanStep {
    pub content: String,
    pub status: String,
}

#[vmux_api::contract(Eq)]
pub enum ChatTurnRow {
    Text {
        index: u32,
        text: String,
    },
    Thinking {
        index: u32,
        text: String,
        latest: bool,
    },
    Tool(ChatToolCall),
    FinishedTools {
        index: u32,
        calls: Vec<ChatToolCall>,
    },
    Subagent(ChatSubagentState),
    Plan {
        index: u32,
        steps: Vec<ChatPlanItem>,
    },
    Diff(ChatDiff),
    ToolResult {
        index: u32,
        content: String,
        is_error: bool,
    },
    Reconnect {
        index: u32,
        attempt: u32,
        total: u32,
    },
}

impl ChatTurnRow {
    pub fn index(&self) -> u32 {
        match self {
            Self::Text { index, .. }
            | Self::Thinking { index, .. }
            | Self::FinishedTools { index, .. }
            | Self::Plan { index, .. }
            | Self::ToolResult { index, .. }
            | Self::Reconnect { index, .. } => *index,
            Self::Tool(call) => call.index,
            Self::Subagent(subagent) => subagent.index,
            Self::Diff(diff) => diff.index,
        }
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatToolCall {
    pub index: u32,
    pub name: String,
    pub kind: ChatToolKind,
    pub activity: ChatActivityKind,
    pub fallback_label: String,
    pub file_path: Option<String>,
    pub arguments: ChatToolArguments,
    pub children: Vec<ChatToolChild>,
    pub live: bool,
}

impl ChatToolCall {
    pub fn command(&self) -> Option<ChatCommand> {
        ChatCommand::from_arguments(self.kind, &self.arguments)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatCommand {
    pub source: String,
    pub preview: String,
    pub metadata: ChatToolArguments,
}

impl ChatCommand {
    fn from_arguments(kind: ChatToolKind, arguments: &ChatToolArguments) -> Option<Self> {
        if kind != ChatToolKind::Command {
            return None;
        }
        let (source, metadata) = match arguments {
            ChatToolArguments::None => return None,
            ChatToolArguments::Fields(arguments) => {
                let source = arguments
                    .iter()
                    .find(|argument| matches!(argument.name.as_str(), "cmd" | "command" | "script"))
                    .and_then(|argument| argument.value.text())?
                    .to_string();
                let metadata = arguments
                    .iter()
                    .filter(|argument| {
                        !matches!(argument.name.as_str(), "cmd" | "command" | "script")
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let metadata = if metadata.is_empty() {
                    ChatToolArguments::None
                } else {
                    ChatToolArguments::Fields(metadata)
                };
                (source, metadata)
            }
            ChatToolArguments::Value(value) => (value.text()?.to_string(), ChatToolArguments::None),
            ChatToolArguments::Raw(raw) => (CommandSource::from_raw(raw)?, ChatToolArguments::None),
        };
        let preview = CommandPreview::from(&source)?;
        Some(Self {
            source,
            preview,
            metadata,
        })
    }
}

#[vmux_api::contract(Eq)]
pub enum ChatToolChild {
    Tool(ChatToolChildCall),
    Subagent(ChatSubagentSummary),
    Result {
        index: u32,
        content: String,
        is_error: bool,
    },
}

impl ChatToolChild {
    pub fn index(&self) -> u32 {
        match self {
            Self::Tool(call) => call.index,
            Self::Subagent(subagent) => subagent.index,
            Self::Result { index, .. } => *index,
        }
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatToolChildCall {
    pub index: u32,
    pub name: String,
    pub kind: ChatToolKind,
    pub activity: ChatActivityKind,
    pub fallback_label: String,
    pub file_path: Option<String>,
    pub arguments: ChatToolArguments,
}

impl ChatToolChildCall {
    pub fn command(&self) -> Option<ChatCommand> {
        ChatCommand::from_arguments(self.kind, &self.arguments)
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatSubagentState {
    pub index: u32,
    pub call_id: String,
    pub provider: String,
    pub title: String,
    pub status: ChatSubagentStatus,
    pub activity: String,
    pub agent_name: Option<String>,
    pub thread_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub child_threads: String,
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub raw_input: String,
    pub children: Vec<ChatToolChild>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatSubagentSummary {
    pub index: u32,
    pub title: String,
    pub status: ChatSubagentStatus,
    pub provider: String,
    pub agent_name: Option<String>,
    pub prompt: Option<String>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatDiff {
    pub index: u32,
    pub path: String,
    pub name: String,
    pub lines: Vec<ChatDiffLine>,
}

#[vmux_api::contract(Eq)]
pub struct ChatDiffLine {
    pub kind: ChatDiffLineKind,
    pub text: String,
}

#[vmux_api::contract(Copy, Eq)]
pub enum ChatDiffLineKind {
    Removed,
    Added,
}

#[vmux_api::contract(Eq)]
pub struct ChatPlanItem {
    pub content: String,
    pub status: ChatPlanStatus,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum ChatPlanStatus {
    Complete,
    Active,
    #[default]
    Pending,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum ChatSubagentStatus {
    Running,
    Complete,
    Failed,
    #[default]
    Pending,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum ChatToolKind {
    Guardian,
    ReadFile,
    ReadSkill,
    WriteFile,
    Layout,
    Worktree,
    Image,
    Screenshot,
    OpenPage,
    Browser,
    Search,
    Command,
    #[default]
    Other,
}

#[vmux_api::contract(Default, Eq)]
pub enum ChatActivityKind {
    #[default]
    None,
    Thinking,
    Writing,
    Installing,
    Awaiting,
    Language(String),
    ReadFile,
    WriteFile,
    Layout,
    Worktree,
    Search,
    Image,
    Screenshot,
    OpenPage,
    Command,
    Browser,
    Guardian,
    Subagent,
    Tool,
    Output,
    Error,
    Plan,
    Diff,
    Reconnect,
}

#[vmux_api::contract(Default, Eq)]
pub enum ChatToolArguments {
    #[default]
    None,
    Fields(Vec<ChatToolArgument>),
    Value(ChatToolArgumentValue),
    Raw(String),
}

struct CommandPreview;

struct CommandSource;

impl CommandSource {
    fn from_raw(raw: &str) -> Option<String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        if let Some(value) = Self::structured(raw) {
            return Some(value);
        }
        Some(raw.to_string())
    }

    fn structured(raw: &str) -> Option<String> {
        let candidate = if raw.starts_with('{') {
            raw
        } else {
            raw.split_once("tools.exec_command(")?.1.trim_start()
        };
        let value = serde_json::Deserializer::from_str(candidate)
            .into_iter::<serde_json::Value>()
            .next()?
            .ok()?;
        let object = value.as_object()?;
        for name in ["cmd", "command", "script"] {
            if let Some(value) = object.get(name).and_then(serde_json::Value::as_str) {
                return Some(value.to_string());
            }
        }
        None
    }
}

impl CommandPreview {
    fn from(command: &str) -> Option<String> {
        let command = command.lines().next()?.trim();
        if command.is_empty() {
            return None;
        }
        let mut preview = String::new();
        let mut redact_rest = false;
        for token in command.split_whitespace() {
            let rendered = Self::token(token, &mut redact_rest);
            if !preview.is_empty() {
                preview.push(' ');
            }
            preview.push_str(&rendered);
            if redact_rest {
                preview.push_str(" •••");
                break;
            }
            if preview.chars().count() > 160 {
                preview = preview.chars().take(159).collect();
                preview.push('…');
                break;
            }
        }
        Some(preview)
    }

    fn token(token: &str, redact_rest: &mut bool) -> String {
        if Self::redacts_following(token) {
            *redact_rest = true;
            return token.to_string();
        }
        if let Some(url) = Self::url(token) {
            return url;
        }
        if let Some((name, _)) = token.split_once('=')
            && (Self::sensitive(name) || Self::redacts_value(name))
        {
            return format!("{name}=•••");
        }
        if Self::sensitive(token) {
            *redact_rest = true;
            return token.to_string();
        }
        if matches!(token.to_ascii_lowercase().as_str(), "bearer" | "basic") {
            *redact_rest = true;
            return token.to_string();
        }
        if Self::opaque(token) {
            return "•••".to_string();
        }
        token.to_string()
    }

    fn redacts_following(value: &str) -> bool {
        value == "-H"
            || matches!(
                value.to_ascii_lowercase().as_str(),
                "-u" | "--user"
                    | "--proxy-user"
                    | "--header"
                    | "--cookie"
                    | "--cookie-jar"
                    | "--cert"
                    | "--key"
            )
    }

    fn redacts_value(value: &str) -> bool {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "--user"
                | "--proxy-user"
                | "--header"
                | "--cookie"
                | "--cookie-jar"
                | "--cert"
                | "--key"
        )
    }

    fn url(value: &str) -> Option<String> {
        let quote = value
            .chars()
            .next()
            .filter(|character| matches!(character, '\'' | '"'));
        let unquoted = quote.map_or(value, |quote| value.trim_matches(quote));
        let scheme_end = unquoted
            .strip_prefix("http://")
            .map(|_| "http://".len())
            .or_else(|| unquoted.strip_prefix("https://").map(|_| "https://".len()))?;
        let authority_end = unquoted[scheme_end..]
            .find(['/', '?', '#'])
            .map(|offset| scheme_end + offset)
            .unwrap_or(unquoted.len());
        let authority = &unquoted[scheme_end..authority_end];
        let mut rendered = if let Some((_, host)) = authority.rsplit_once('@') {
            format!(
                "{}•••@{}{}",
                &unquoted[..scheme_end],
                host,
                &unquoted[authority_end..]
            )
        } else {
            unquoted.to_string()
        };
        if let Some(query) = rendered.find('?') {
            rendered.truncate(query + 1);
            rendered.push('…');
        }
        if let Some(quote) = quote {
            rendered.insert(0, quote);
            rendered.push(quote);
        }
        Some(rendered)
    }

    fn sensitive(value: &str) -> bool {
        let value = value
            .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .to_ascii_lowercase();
        [
            "token",
            "password",
            "passwd",
            "secret",
            "credential",
            "authorization",
            "cookie",
            "api_key",
            "apikey",
            "api-key",
            "auth-token",
            "access-token",
            "access_key",
            "private_key",
            "signature",
        ]
        .iter()
        .any(|needle| value.contains(needle))
    }

    fn opaque(value: &str) -> bool {
        let value = value.trim_matches(['\'', '"']);
        value.len() >= 32
            && !value.contains('/')
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric()
                    || matches!(character, '-' | '_' | '.' | '+' | '=' | '/')
            })
    }
}

#[vmux_api::contract(recursive, Eq)]
pub struct ChatToolArgument {
    pub name: String,
    pub label: String,
    pub value: ChatToolArgumentValue,
}

#[vmux_api::contract(recursive, Eq)]
pub enum ChatToolArgumentValue {
    Path(String),
    Code(String),
    Text(String),
    Bool(bool),
    Number(String),
    List(Vec<ChatToolArgument>),
    Object(Vec<ChatToolArgument>),
    Null,
}

impl ChatToolArgumentValue {
    fn text(&self) -> Option<&str> {
        match self {
            Self::Path(value) | Self::Code(value) | Self::Text(value) => Some(value),
            Self::Bool(_) | Self::Number(_) | Self::List(_) | Self::Object(_) | Self::Null => None,
        }
    }
}

#[cfg(test)]
mod command_preview_tests {
    use super::*;

    #[test]
    fn command_call_accepts_path_arguments() {
        let call = ChatToolCall {
            kind: ChatToolKind::Command,
            arguments: ChatToolArguments::Value(ChatToolArgumentValue::Path(
                "/usr/bin/git status".into(),
            )),
            ..Default::default()
        };

        let command = call.command().unwrap();
        assert_eq!(command.preview, "/usr/bin/git status");
    }

    #[test]
    fn command_call_redacts_secrets_and_ignores_later_lines() {
        let call = ChatToolCall {
            kind: ChatToolKind::Command,
            arguments: ChatToolArguments::Raw(
                "token=secret curl https://example.com/file?signature=secret --password hunter2\necho leaked"
                    .into(),
            ),
            ..Default::default()
        };

        let command = call.command().unwrap();
        assert_eq!(
            command.preview,
            "token=••• curl https://example.com/file?… --password •••"
        );
    }

    #[test]
    fn command_call_does_not_expose_authorization_credentials() {
        let call = ChatToolCall {
            kind: ChatToolKind::Command,
            arguments: ChatToolArguments::Raw(
                "curl -H 'Authorization: Bearer exposed' https://example.com".into(),
            ),
            ..Default::default()
        };

        let command = call.command().unwrap();
        assert_eq!(command.preview, "curl -H •••");
    }

    #[test]
    fn command_call_extracts_the_shell_command_from_a_codex_wrapper() {
        let call = ChatToolCall {
            kind: ChatToolKind::Command,
            arguments: ChatToolArguments::Raw(
                r#"const result = await tools.exec_command({"cmd":"git status && cargo test","workdir":"/work"});"#
                    .into(),
            ),
            ..Default::default()
        };

        let command = call.command().unwrap();
        assert_eq!(command.source, "git status && cargo test");
        assert_eq!(command.preview, "git status && cargo test");
        assert_eq!(command.metadata, ChatToolArguments::None);
    }

    #[test]
    fn command_call_keeps_embedded_command_json_in_a_shell_command() {
        let call = ChatToolCall {
            kind: ChatToolKind::Command,
            arguments: ChatToolArguments::Raw(
                r#"curl -d '{"cmd":"status"}' https://example.com"#.into(),
            ),
            ..Default::default()
        };

        let command = call.command().unwrap();
        assert_eq!(
            command.source,
            r#"curl -d '{"cmd":"status"}' https://example.com"#
        );
    }

    #[test]
    fn command_call_redacts_credentials_in_flags_headers_and_urls() {
        let user = ChatToolCall {
            kind: ChatToolKind::Command,
            arguments: ChatToolArguments::Raw("curl -u user:password https://example.com".into()),
            ..Default::default()
        };
        let header = ChatToolCall {
            kind: ChatToolKind::Command,
            arguments: ChatToolArguments::Raw(
                "curl -H 'X-Api-Key: exposed' https://example.com".into(),
            ),
            ..Default::default()
        };
        let url = ChatToolCall {
            kind: ChatToolKind::Command,
            arguments: ChatToolArguments::Raw(
                "curl https://user:password@example.com/path?token=exposed".into(),
            ),
            ..Default::default()
        };

        assert_eq!(user.command().unwrap().preview, "curl -u •••");
        assert_eq!(header.command().unwrap().preview, "curl -H •••");
        assert_eq!(
            url.command().unwrap().preview,
            "curl https://•••@example.com/path?…"
        );
    }

    #[test]
    fn child_command_calls_share_command_extraction() {
        let call = ChatToolChildCall {
            kind: ChatToolKind::Command,
            arguments: ChatToolArguments::Fields(vec![ChatToolArgument {
                name: "cmd".into(),
                label: "Command".into(),
                value: ChatToolArgumentValue::Code("cargo test".into()),
            }]),
            ..Default::default()
        };

        assert_eq!(call.command().unwrap().source, "cargo test");
    }

    #[test]
    fn non_command_call_has_no_command_preview() {
        let call = ChatToolCall {
            kind: ChatToolKind::Other,
            arguments: ChatToolArguments::Raw("echo visible".into()),
            ..Default::default()
        };

        assert_eq!(call.command(), None);
    }
}

#[vmux_api::contract(Eq)]
pub enum ChatItem {
    User {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<ChatAttachment>,
        #[serde(default, skip_serializing_if = "is_zero")]
        created_at_ms: u64,
    },
    Turn(ChatTurn),
}

impl ChatItem {
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            text: text.into(),
            context: None,
            attachments: Vec::new(),
            created_at_ms: 0,
        }
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ChatTurn {
    pub blocks: Vec<ChatBlock>,
    #[serde(default)]
    pub rows: Vec<ChatTurnRow>,
    #[serde(default)]
    pub copy_text: String,
    #[serde(default)]
    pub activity: ChatActivityKind,
    #[serde(default)]
    pub active_subagents: u32,
    #[serde(default)]
    pub active_tasks: u32,
    pub running: bool,
    pub duration_secs: Option<u32>,
    pub step_count: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub created_at_ms: u64,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

pub const WORKING_VERB_IDS: &[&str] = &[
    "agent-working-working",
    "agent-working-thinking",
    "agent-working-pondering",
    "agent-working-noodling",
    "agent-working-percolating",
    "agent-working-conjuring",
    "agent-working-cooking",
    "agent-working-brewing",
    "agent-working-musing",
    "agent-working-ruminating",
    "agent-working-scheming",
    "agent-working-synthesizing",
    "agent-working-tinkering",
    "agent-working-churning",
    "agent-working-vibing",
    "agent-working-simmering",
    "agent-working-crafting",
    "agent-working-divining",
    "agent-working-mulling",
    "agent-working-spelunking",
];

#[vmux_api::ui_event(Default, Eq)]
pub struct PromptHistoryRequest {
    pub agent: String,
    pub cwd: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct PromptHistory {
    pub prompts: Vec<String>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ResumableSessionEntry {
    pub kind: String,
    pub sid: String,
    pub cwd: String,
    pub url: String,
    pub title: String,
    pub latest: String,
    pub subtitle: String,
    pub age_seconds: u64,
    #[serde(default)]
    pub updated_at: String,
    pub agent_name: String,
    pub project: String,
    pub branch: String,
    pub cross_runtime: bool,
}
#[vmux_api::contract(Default)]
pub struct ResumableSessions {
    pub request_id: u64,
    pub query: String,
    pub sessions: Vec<ResumableSessionEntry>,
    pub offset: u32,
    pub total: u32,
}

impl ResumableSessions {
    pub const PAGE: u32 = 50;
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum SlashCommand {
    #[default]
    Upload,
    Resume,
    Mcp,
    Model,
}

impl SlashCommand {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Upload => "upload",
            Self::Resume => "resume",
            Self::Mcp => "mcp",
            Self::Model => "model",
        }
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct SlashCommandEntry {
    pub command: SlashCommand,
    pub description: String,
    pub delegated: bool,
}
#[vmux_api::contract(Default)]
pub struct SlashCommands {
    pub commands: Vec<SlashCommandEntry>,
}
#[vmux_api::ui_event(Default)]
pub struct ResumeListRequest {
    pub request_id: u64,
    pub query: String,
    pub offset: u32,
}
#[vmux_api::ui_event(Default)]
pub struct ResumeSession {
    pub kind: String,
    pub sid: String,
    pub cwd: String,
    pub open: Option<crate::open_target::OpenTarget>,
}
