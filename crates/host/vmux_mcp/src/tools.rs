mod bookmark;
mod files;
mod knowledge;
mod param;
mod state;
mod visual;
mod workspace;

use serde::Serialize;
use serde_json::Value;
use vmux_client::protocol::{AgentCommand, ProcessId};

pub use param::McpParamTool;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug)]
pub enum DispatchTarget {
    Command(AgentCommand),
    Query(vmux_client::protocol::AgentQuery),
}

#[derive(Clone, Copy)]
enum ToolAvailability {
    Always,
    OutsideAcpSession,
    WithoutAcpTerminals,
}

struct ToolSpec {
    name: &'static str,
    aliases: &'static [&'static str],
    definition: fn() -> ToolDefinition,
    route: ToolRoute,
    availability: ToolAvailability,
    shell_note: bool,
}

struct ToolCall<'a> {
    arguments: Value,
    anchor: Option<ProcessId>,
    host_shell: &'a str,
}

impl ToolCall<'_> {
    fn parse<T: serde::de::DeserializeOwned>(&self, name: &str) -> Result<T, String> {
        serde_json::from_value(self.arguments.clone())
            .map_err(|error| format!("{name}: invalid arguments: {error}"))
    }

    fn require_anchor(&self, name: &str) -> Result<ProcessId, String> {
        self.anchor.ok_or_else(|| {
            format!("{name} requires an agent anchor (not available to this client)")
        })
    }
}

type ToolDispatch = for<'a> fn(ToolCall<'a>) -> Result<DispatchTarget, String>;

impl ToolSpec {
    fn find(name: &str) -> Option<&'static Self> {
        TOOL_SPECS
            .iter()
            .find(|spec| spec.name == name || spec.aliases.contains(&name))
    }

    fn available(&self, acp_session: bool, acp_terminals: bool) -> bool {
        match self.availability {
            ToolAvailability::Always => true,
            ToolAvailability::OutsideAcpSession => !acp_session,
            ToolAvailability::WithoutAcpTerminals => !acp_terminals,
        }
    }

    fn definition(&self, shell: &str) -> ToolDefinition {
        let mut definition = (self.definition)();
        definition.name = self.name.to_string();
        if self.shell_note {
            definition
                .description
                .push_str(&ShellNote::for_shell(shell));
        }
        definition
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtocolTool {
    ReadFile,
    Grep,
    VaultStatus,
}

#[derive(Clone, Copy)]
enum ToolRoute {
    Local(ToolDispatch),
    Protocol(ProtocolTool),
}

pub(crate) fn canonical_tool_name(name: &str) -> &str {
    name.strip_prefix("vmux_").unwrap_or(name)
}

pub(crate) fn protocol_tool(name: &str) -> Option<ProtocolTool> {
    match ToolSpec::find(canonical_tool_name(name))?.route {
        ToolRoute::Protocol(tool) => Some(tool),
        ToolRoute::Local(_) => None,
    }
}

pub(crate) fn tool_available(name: &str, acp_session: bool, acp_terminals: bool) -> bool {
    let name = canonical_tool_name(name);
    ToolSpec::find(name).is_none_or(|spec| spec.available(acp_session, acp_terminals))
}

const ALWAYS: ToolAvailability = ToolAvailability::Always;

const TOOL_SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "read_layout",
        aliases: &[],
        definition: state::read_layout_definition,
        route: ToolRoute::Local(state::read_layout),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "update_layout",
        aliases: &[],
        definition: state::update_layout_definition,
        route: ToolRoute::Local(state::update_layout),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "get_settings",
        aliases: &[],
        definition: state::get_settings_definition,
        route: ToolRoute::Local(state::get_settings),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "list_spaces",
        aliases: &[],
        definition: state::list_spaces_definition,
        route: ToolRoute::Local(state::list_spaces),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "open_page",
        aliases: &[],
        definition: workspace::open_page_definition,
        route: ToolRoute::Local(workspace::open_page),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "open_file",
        aliases: &[],
        definition: workspace::open_file_definition,
        route: ToolRoute::Local(workspace::open_file),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "read_file",
        aliases: &[],
        definition: files::read_file_definition,
        route: ToolRoute::Protocol(ProtocolTool::ReadFile),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "grep",
        aliases: &[],
        definition: files::grep_definition,
        route: ToolRoute::Protocol(ProtocolTool::Grep),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "resume_in_acp",
        aliases: &[],
        definition: workspace::resume_in_acp_definition,
        route: ToolRoute::Local(workspace::resume_in_acp),
        availability: ToolAvailability::OutsideAcpSession,
        shell_note: false,
    },
    ToolSpec {
        name: "run",
        aliases: &[],
        definition: workspace::run_definition,
        route: ToolRoute::Local(workspace::run),
        availability: ToolAvailability::WithoutAcpTerminals,
        shell_note: true,
    },
    ToolSpec {
        name: "request_user_choice",
        aliases: &[],
        definition: workspace::request_user_choice_definition,
        route: ToolRoute::Local(workspace::request_user_choice),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "vault_status",
        aliases: &[],
        definition: knowledge::vault_status_definition,
        route: ToolRoute::Protocol(ProtocolTool::VaultStatus),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "open_vault",
        aliases: &[],
        definition: knowledge::open_vault_definition,
        route: ToolRoute::Local(knowledge::open_vault),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "set_conversation_title",
        aliases: &[],
        definition: knowledge::set_conversation_title_definition,
        route: ToolRoute::Local(knowledge::set_conversation_title),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "search_knowledge",
        aliases: &[],
        definition: knowledge::search_knowledge_definition,
        route: ToolRoute::Local(knowledge::search),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "read_knowledge",
        aliases: &[],
        definition: knowledge::read_knowledge_definition,
        route: ToolRoute::Local(knowledge::read),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "write_knowledge",
        aliases: &[],
        definition: knowledge::write_knowledge_definition,
        route: ToolRoute::Local(knowledge::write),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "select_project",
        aliases: &["select_workspace", "choose_workspace"],
        definition: workspace::select_project_definition,
        route: ToolRoute::Local(workspace::select_project),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "create_worktree",
        aliases: &[],
        definition: workspace::create_worktree_definition,
        route: ToolRoute::Local(workspace::create_worktree),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "read_terminal",
        aliases: &[],
        definition: workspace::read_terminal_definition,
        route: ToolRoute::Local(workspace::read_terminal),
        availability: ToolAvailability::WithoutAcpTerminals,
        shell_note: false,
    },
    ToolSpec {
        name: "screenshot",
        aliases: &[],
        definition: visual::screenshot_definition,
        route: ToolRoute::Local(visual::screenshot),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "simulator_screenshot",
        aliases: &[],
        definition: visual::simulator_screenshot_definition,
        route: ToolRoute::Local(visual::simulator_screenshot),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "simulator_tap",
        aliases: &[],
        definition: visual::simulator_tap_definition,
        route: ToolRoute::Local(visual::simulator_tap),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "simulator_swipe",
        aliases: &[],
        definition: visual::simulator_swipe_definition,
        route: ToolRoute::Local(visual::simulator_swipe),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "simulator_type",
        aliases: &[],
        definition: visual::simulator_type_definition,
        route: ToolRoute::Local(visual::simulator_type),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "simulator_key",
        aliases: &[],
        definition: visual::simulator_key_definition,
        route: ToolRoute::Local(visual::simulator_key),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "simulator_button",
        aliases: &[],
        definition: visual::simulator_button_definition,
        route: ToolRoute::Local(visual::simulator_button),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "browser_snapshot",
        aliases: &[],
        definition: visual::browser_snapshot_definition,
        route: ToolRoute::Local(visual::browser_snapshot),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "browser_scroll",
        aliases: &[],
        definition: visual::browser_scroll_definition,
        route: ToolRoute::Local(visual::browser_scroll),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "record_start",
        aliases: &[],
        definition: visual::record_start_definition,
        route: ToolRoute::Local(visual::record_start),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "record_stop",
        aliases: &[],
        definition: visual::record_stop_definition,
        route: ToolRoute::Local(visual::record_stop),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "bookmark_list",
        aliases: &[],
        definition: bookmark::bookmark_list_definition,
        route: ToolRoute::Local(bookmark::list),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "bookmark_add",
        aliases: &[],
        definition: bookmark::bookmark_add_definition,
        route: ToolRoute::Local(bookmark::add),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "bookmark_remove",
        aliases: &[],
        definition: bookmark::bookmark_remove_definition,
        route: ToolRoute::Local(bookmark::remove),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "bookmark_pin",
        aliases: &[],
        definition: bookmark::bookmark_pin_definition,
        route: ToolRoute::Local(bookmark::pin),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "bookmark_unpin",
        aliases: &[],
        definition: bookmark::bookmark_unpin_definition,
        route: ToolRoute::Local(bookmark::unpin),
        availability: ALWAYS,
        shell_note: false,
    },
    ToolSpec {
        name: "bookmark_folder_create",
        aliases: &[],
        definition: bookmark::bookmark_folder_create_definition,
        route: ToolRoute::Local(bookmark::create_folder),
        availability: ALWAYS,
        shell_note: false,
    },
];

pub fn tool_definitions() -> Vec<ToolDefinition> {
    tool_definitions_filtered(false, false, "")
}

pub struct ShellNote;

impl ShellNote {
    pub fn for_shell(shell: &str) -> String {
        let base = shell
            .rsplit('/')
            .next()
            .unwrap_or(shell)
            .trim()
            .to_ascii_lowercase();
        if base.is_empty() {
            return String::new();
        }
        let differences = match base.as_str() {
            "nu" | "nushell" => concat!(
                " Write nushell, not POSIX: redirect both streams with `out+err>` (`2>&1` is a parse error),",
                " substitute with `(cmd)` not `$(cmd)`, set variables with `$env.NAME = \"value\"` not `export`,",
                " and use `| ignore` rather than `> /dev/null` to discard output."
            ),
            "fish" => concat!(
                " Write fish, not POSIX: redirect both streams with `&>`, set variables with `set -x NAME value`",
                " not `export`, and note that `&&` and `||` are `; and` and `; or`."
            ),
            _ => "",
        };
        if differences.is_empty() {
            return format!(" The shell is {base}.");
        }
        format!(
            " The shell is {base}.{differences} To run a POSIX script instead, invoke `bash -c \"...\"` as the command."
        )
    }
}

pub fn tool_definitions_filtered(
    acp_session: bool,
    acp_terminals: bool,
    shell: &str,
) -> Vec<ToolDefinition> {
    let mut defs: Vec<ToolDefinition> = vmux_command_mcp::tool_entries()
        .into_iter()
        .chain(McpParamTool::mcp_tool_entries())
        .map(|(name, description, schema)| ToolDefinition {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: schema,
        })
        .collect();
    for spec in TOOL_SPECS {
        if spec.available(acp_session, acp_terminals) {
            defs.push(spec.definition(shell));
        }
    }
    defs
}

pub fn dispatch_from_tool_call(name: &str, arguments: Value) -> Result<DispatchTarget, String> {
    dispatch_with_anchor(name, arguments, None)
}

pub fn dispatch_with_anchor(
    name: &str,
    arguments: Value,
    anchor: Option<vmux_client::protocol::ProcessId>,
) -> Result<DispatchTarget, String> {
    dispatch_in_shell(name, arguments, anchor, "")
}

pub fn dispatch_in_shell(
    name: &str,
    arguments: Value,
    anchor: Option<ProcessId>,
    host_shell: &str,
) -> Result<DispatchTarget, String> {
    let name = canonical_tool_name(name);
    if let Some(spec) = ToolSpec::find(name) {
        return match spec.route {
            ToolRoute::Local(dispatch) => dispatch(ToolCall {
                arguments,
                anchor,
                host_shell,
            }),
            ToolRoute::Protocol(_) => Err(format!("tool {name} requires MCP protocol context")),
        };
    }
    if let Some(parsed) = McpParamTool::from_mcp_call(name, arguments.clone()) {
        return parsed
            .and_then(McpParamTool::to_agent_command)
            .map(DispatchTarget::Command);
    }
    if vmux_command_mcp::accepts_id(name) {
        return Ok(DispatchTarget::Command(AgentCommand::AppCommand {
            id: name.to_string(),
            args_json: String::new(),
        }));
    }
    if vmux_command_mcp::accepts_call(name, arguments.clone()) {
        let args_json = serde_json::to_string(&arguments).unwrap_or_default();
        return Ok(DispatchTarget::Command(AgentCommand::AppCommand {
            id: name.to_string(),
            args_json,
        }));
    }
    Err(format!("unknown tool: {name}"))
}

#[cfg(test)]
mod tests;
