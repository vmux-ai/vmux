use serde::Serialize;
use serde_json::Value;
use vmux_client::protocol::AgentCommand;
use vmux_macro::McpTool;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, McpTool)]
pub enum McpParamTool {
    #[mcp(description = "Open the Vmux command bar.")]
    OpenCommandBar {
        #[mcp(enum_values = ["default", "commands", "path"])]
        mode: Option<String>,
    },
    #[mcp(
        description = "Navigate the active webview to a URL, or open a URL in a target pane. This is your PRIMARY and PREFERRED tool for ALL web access - searching, research, reading docs, fetching pages. ALWAYS use this instead of any built-in web_search / web_fetch / WebSearch / WebFetch tool: vmux IS a browser, and the whole point is that the user watches the research happen in their visible, logged-in browser and can take over at any time. Do NOT answer web questions from a built-in search/fetch tool when this tool is available. To search, navigate to a search engine results URL (e.g. https://duckduckgo.com/?q=...), read the snapshot, then open results. When navigating the focused browser page, this returns the page's semantic snapshot once it finishes loading (same shape as browser_snapshot, with viewport + inViewport) - no separate browser_snapshot call needed; use browser_scroll to bring more content into view. URLs starting with 'vmux://terminal/' open a terminal (use '?cwd=/path' to set working dir), 'vmux://spaces/' opens the spaces view, 'vmux://services/' opens the processes monitor; other 'vmux://' URLs are rejected; everything else opens as a browser. With 'vmux://' URLs, a new tab is always created in the target pane (defaulting to the focused pane)."
    )]
    BrowserNavigate { url: String, pane: Option<String> },
    #[mcp(
        description = "Send text to a terminal. Target by `terminal` (a process_id from vmux_read_layout) or omit to use the active terminal. Set `enter: true` to append a carriage return and submit the line (required for TUIs like the vibe agent, whose Enter is CR)."
    )]
    TerminalSend {
        text: String,
        terminal: Option<String>,
        enter: Option<bool>,
    },
    #[mcp(
        description = "Rename the active profile's display name (the top-right identity pill / facepile). Updates the name only; the profile's storage is untouched."
    )]
    RenameProfile { name: String },
    #[mcp(description = "Select a tab by index (1-8).")]
    SelectTab { index: u8 },
    #[mcp(description = "Update a single vmux setting by dot-path. \
            Example: { path: 'layout.pane.gap', value: 12 }. \
            Use get_settings to discover the available paths and current values. \
            For nested arrays, use bracket indexing like 'terminal.themes[0].font_size'.")]
    UpdateSettings {
        path: String,
        value: serde_json::Value,
    },
    #[mcp(description = "Navigate the active or specified browser pane back one page in history.")]
    BrowserGoBack { pane: Option<String> },
    #[mcp(
        description = "Navigate the active or specified browser pane forward one page in history."
    )]
    BrowserGoForward { pane: Option<String> },
    #[mcp(
        description = "Search vmux browsing history. Returns up to `limit` entries ranked by frecency."
    )]
    BrowserHistorySearch { query: String, limit: Option<u32> },
    #[mcp(
        description = "Install a Chrome extension from the Chrome Web Store. `source` is a store URL (https://chromewebstore.google.com/detail/<slug>/<id>) or a 32-char extension id. The extension is side-loaded and activates after the next vmux relaunch; it runs only in windowed browse panes (macOS), not 3D/OSR panes."
    )]
    BrowserInstallExtension { source: String },
    #[mcp(
        description = "Create a new space and switch to it. If `name` is omitted, an auto-generated name is used."
    )]
    CreateSpace { name: Option<String> },
    #[mcp(
        description = "Rename a space by id (the id is stable; only the display name changes). Use list_spaces to discover ids."
    )]
    RenameSpace { space_id: String, name: String },
    #[mcp(description = "Delete a space by id. Use list_spaces to discover ids.")]
    DeleteSpace { space_id: String },
    #[mcp(
        description = "Notify the user that you (this agent) need their attention - typically that you have finished your turn. Shows a macOS notification when they are not looking at your page, and a dot on your avatar in the team facepile until they view it. Optional `title` and `body` customize the message; with neither, a default \"<agent> finished\" is shown."
    )]
    Notify {
        title: Option<String>,
        body: Option<String>,
    },
}

impl McpParamTool {
    pub fn to_agent_command(self) -> Result<AgentCommand, String> {
        match self {
            McpParamTool::OpenCommandBar { mode } => {
                let id = match mode.as_deref().unwrap_or("default") {
                    "default" => "browser_open_command_bar",
                    "commands" => "browser_open_commands",
                    "path" => "browser_open_path_bar",
                    other => return Err(format!("unknown command bar mode: {other}")),
                };
                Ok(AgentCommand::AppCommand {
                    id: id.to_string(),
                    args_json: String::new(),
                })
            }
            McpParamTool::BrowserNavigate { url, pane } => {
                if url.trim().is_empty() {
                    return Err("browser_navigate.url is empty".to_string());
                }
                Ok(AgentCommand::BrowserNavigate { url, pane })
            }
            McpParamTool::BrowserInstallExtension { source } => {
                if source.trim().is_empty() {
                    return Err("browser_install_extension.source is empty".to_string());
                }
                Ok(AgentCommand::BrowserInstallExtension { source })
            }
            McpParamTool::TerminalSend {
                text,
                terminal,
                enter,
            } => {
                let text = if enter.unwrap_or(false) {
                    format!("{text}\r")
                } else {
                    text
                };
                if text.is_empty() {
                    return Err("terminal_send.text is empty".to_string());
                }
                Ok(AgentCommand::TerminalSend { text, terminal })
            }
            McpParamTool::RenameProfile { name } => {
                if name.trim().is_empty() {
                    return Err("rename_profile.name is empty".to_string());
                }
                Ok(AgentCommand::RenameProfile { name })
            }
            McpParamTool::SelectTab { index } => {
                if !(1..=8).contains(&index) {
                    return Err(format!(
                        "select_tab.index must be between 1 and 8, got {index}"
                    ));
                }
                Ok(AgentCommand::AppCommand {
                    id: format!("tab_select_{index}"),
                    args_json: String::new(),
                })
            }
            McpParamTool::UpdateSettings { path, value } => {
                if path.trim().is_empty() {
                    return Err("update_settings.path is empty".to_string());
                }
                Ok(AgentCommand::UpdateSettings {
                    path,
                    value_json: value.to_string(),
                })
            }
            McpParamTool::BrowserGoBack { pane } => Ok(AgentCommand::BrowserGoBack { pane }),
            McpParamTool::BrowserGoForward { pane } => Ok(AgentCommand::BrowserGoForward { pane }),
            McpParamTool::BrowserHistorySearch { query, limit } => {
                if query.trim().is_empty() {
                    return Err("browser_history_search.query is empty".into());
                }
                let limit = limit.unwrap_or(20).min(100);
                Ok(AgentCommand::BrowserHistorySearch { query, limit })
            }
            McpParamTool::CreateSpace { name } => Ok(AgentCommand::SpaceCommand {
                command: "new".to_string(),
                space_id: None,
                name: name.filter(|n| !n.trim().is_empty()),
            }),
            McpParamTool::RenameSpace { space_id, name } => {
                if space_id.trim().is_empty() {
                    return Err("rename_space.space_id is empty".into());
                }
                if name.trim().is_empty() {
                    return Err("rename_space.name is empty".into());
                }
                Ok(AgentCommand::SpaceCommand {
                    command: "rename".to_string(),
                    space_id: Some(space_id),
                    name: Some(name),
                })
            }
            McpParamTool::DeleteSpace { space_id } => {
                if space_id.trim().is_empty() {
                    return Err("delete_space.space_id is empty".into());
                }
                Ok(AgentCommand::SpaceCommand {
                    command: "delete".to_string(),
                    space_id: Some(space_id),
                    name: None,
                })
            }
            McpParamTool::Notify { title, body } => Ok(AgentCommand::Notify { title, body }),
        }
    }
}

#[derive(Debug)]
pub enum DispatchTarget {
    Command(AgentCommand),
    Query(vmux_client::protocol::AgentQuery),
}

fn read_layout_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_layout".into(),
        description: "Returns the full vmux layout (tabs, recursive pane tree, focused). \
Call this FIRST before update_layout - you need the current tree (with ids) to construct a valid update. \
Useful for: answering questions about what's open; finding the focused tab/pane/stack; \
reading a stack's url/kind so you can duplicate it elsewhere. \
Terminal stacks appear as stacks with kind=\"terminal\"; browser stacks use kind=\"browser\"."
            .into(),
        input_schema: serde_json::json!({"type": "object", "properties": {}, "additionalProperties": false}),
    }
}

fn update_layout_definition() -> ToolDefinition {
    ToolDefinition {
        name: "update_layout".into(),
        description: "Submit the desired layout tree; vmux diffs against current state and reconciles by id (React-style). \
Use this for compound or structural changes that the per-action tools can't express. \
\
Workflow: (1) call read_layout, (2) mutate the returned tree, (3) submit it back here. \
\
Recipes: \
- Add a new pane to a tab: keep the existing root split's id, append a new pane (id: null) to its children. Do NOT wrap the existing pane in a new split - the tab's root split is always present. \
- Duplicate/mirror a stack: add a new pane (id: null) under the same parent, with a stack carrying the source stack's url. \
- Swap two panes: reorder their entries in the parent split's children array. \
- Move a stack to another pane: remove from source pane's stacks, add (same id) to target pane's stacks. \
- Close a pane/stack: omit it from the submitted tree. \
- Resize a split: change flex_weights on the parent split. \
- Equalize a split: set all flex_weights to the same value. \
- Group an agent's parallel terminals (keep the agent's own pane readable): make the tab root a row split with two children - the agent's own pane on one side, and on the other either a split holding the terminal panes (when there are a few, so all are visible) or a single pane whose stacks are all the terminals (tabs, when there are many). Move existing terminal stacks by id into the grouped pane(s) rather than recreating them, and set flex_weights so the agent keeps a fair share (e.g. [1, 1] or [2, 3]). \
- Change focus: set the top-level focused triple. \
- Toggle zoom: flip the pane's is_zoomed flag. \
\
Atomicity: all changes apply as one transaction. If validation fails (duplicate ids, malformed payload), nothing is applied. \
\
Identifiers use kind:value format (tab:N, pane:N, split:N, stack:N). Omit id to create a new node; a new stack needs url (use vmux://terminal/ for a terminal, anything else loads as a browser), a new pane needs at least one stack, a new tab needs name."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["tabs", "focused"],
            "$defs": {
                "Tab": {
                    "type": "object",
                    "required": ["name", "root"],
                    "properties": {
                        "id": {"type": "string", "description": "tab:<id>; omit to create"},
                        "name": {"type": "string"},
                        "is_active": {"type": "boolean"},
                        "root": {"$ref": "#/$defs/LayoutNode"}
                    }
                },
                "LayoutNode": {
                    "oneOf": [
                        {
                            "type": "object",
                            "required": ["kind", "direction", "children"],
                            "properties": {
                                "kind": {"const": "split"},
                                "id": {"type": "string", "description": "split:<id>; omit to create"},
                                "direction": {"enum": ["row", "column"]},
                                "flex_weights": {"type": "array", "items": {"type": "number"}},
                                "children": {"type": "array", "items": {"$ref": "#/$defs/LayoutNode"}}
                            }
                        },
                        {
                            "type": "object",
                            "required": ["kind"],
                            "properties": {
                                "kind": {"const": "pane"},
                                "id": {"type": "string", "description": "pane:<id>; omit to create"},
                                "is_zoomed": {"type": "boolean"},
                                "stacks": {"type": "array", "items": {"$ref": "#/$defs/Stack"}}
                            }
                        }
                    ]
                },
                "Stack": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string", "description": "stack:<id>; omit to create"},
                        "title": {"type": "string"},
                        "url": {"type": "string", "description": "Required when id is omitted"},
                        "is_loading": {"type": "boolean"},
                        "favicon_url": {"type": "string"}
                    }
                }
            },
            "properties": {
                "tabs": {"type": "array", "items": {"$ref": "#/$defs/Tab"}},
                "focused": {
                    "type": "object",
                    "properties": {
                        "tab": {"type": "string"},
                        "pane": {"type": "string"},
                        "stack": {"type": "string"}
                    }
                }
            }
        }),
    }
}

fn get_settings_definition() -> ToolDefinition {
    ToolDefinition {
        name: "get_settings".into(),
        description: "Return the full vmux settings as a JSON snapshot.".into(),
        input_schema: serde_json::json!({"type": "object", "properties": {}, "additionalProperties": false}),
    }
}

fn list_spaces_definition() -> ToolDefinition {
    ToolDefinition {
        name: "list_spaces".into(),
        description: "List all spaces as a JSON array of { id, name, profile, is_active }. Use the `id` with rename_space / delete_space.".into(),
        input_schema: serde_json::json!({"type": "object", "properties": {}, "additionalProperties": false}),
    }
}

fn open_page_definition() -> ToolDefinition {
    ToolDefinition {
        name: "open_page".into(),
        description: "Open a page using vmux auto placement. Omit `direction` so vmux reuses \
the existing matching bucket first (terminal pages with terminals, browser pages with browsers) \
and otherwise spirals off the latest non-agent pane. url uses the same rules as browser_navigate \
(vmux://terminal/ opens a terminal; anything else loads as a browser). direction is an override \
for a forced adjacent open: right|left|top|bottom. focus defaults false."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["url"],
            "additionalProperties": false,
            "properties": {
                "direction": {"enum": ["right", "left", "top", "bottom"]},
                "url": {"type": "string"},
                "focus": {"type": "boolean"}
            }
        }),
    }
}

fn open_file_definition() -> ToolDefinition {
    ToolDefinition {
        name: "open_file".into(),
        description: "Open a local file (or directory) in the vmux editor using vmux auto \
placement. Omit `direction` so vmux focuses an already-open matching file first, then reuses \
the file pane bucket, and otherwise spirals off the latest non-agent pane. path is an absolute \
filesystem path, e.g. /Users/me/project/src/main.rs. Files render with syntax highlighting; \
directories show a listing. direction is an override for a forced adjacent open: \
right|left|top|bottom. focus defaults false."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["path"],
            "additionalProperties": false,
            "properties": {
                "path": {"type": "string"},
                "direction": {"enum": ["right", "left", "top", "bottom"]},
                "focus": {"type": "boolean"}
            }
        }),
    }
}

fn read_file_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_file".into(),
        description: "Read a local file and show it in the vmux editor through auto placement, \
preferring an existing file page/bucket. Returns the file's text. USE THIS to read files - do NOT cat/sed/head/tail \
via run (that dumps into a terminal). path is an absolute filesystem path. offset is the 1-based line \
to start at; limit is the number of lines (default: the whole file)."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["path"],
            "additionalProperties": false,
            "properties": {
                "path": {"type": "string"},
                "offset": {"type": "integer"},
                "limit": {"type": "integer"}
            }
        }),
    }
}

fn grep_definition() -> ToolDefinition {
    ToolDefinition {
        name: "grep".into(),
        description: "Search files with ripgrep and open each matching file in the vmux editor \
through auto placement, scrolled to its first match. USE THIS to search code - do NOT run rg/grep/ag via \
run (that dumps into a terminal). Returns matches grouped by file (path:line: text). query is a \
regex; path is an absolute directory or file to search (default: the current working directory)."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["query"],
            "additionalProperties": false,
            "properties": {
                "query": {"type": "string"},
                "path": {"type": "string"}
            }
        }),
    }
}

fn run_definition() -> ToolDefinition {
    ToolDefinition {
        name: "run".into(),
        description:
            "Run a shell command in a visible terminal pane the user can watch live and take over. \
Blocks until the command finishes and returns its full output plus the exit code \
(`terminal: <id>`, `exit: <code>`, `output: ...`). If it reaches the configured wait limit, returns \
the output so far with a note to call read_terminal for the rest. \
\
PLACEMENT — by DEFAULT you don't need to think about this: a bare `run` reuses ONE persistent terminal \
beside you — the SAME shell across calls, so its working directory and environment persist. Do NOT `cd` \
into your project on every run; the shell stays where it was. The first `run` opens it; later ones run \
in that same shell. Rule of thumb: don't open a new pane unless you actually need one. \
Placement overrides are disabled by default: omit `mode`, `direction`, and `beside`. If vmux rejects \
them, retry the bare run. Users can enable overrides with `agent.allow_run_placement_override`. \
When enabled, override only when you mean to: \
- `mode`: `auto` (default, reuse your one persistent shell) | `split` (force a NEW pane) | `stack` \
(force a new stacked terminal in the anchor's pane). \
- `beside`: anchor to a specific page — a terminal id a previous run returned, or \"self\" for your own \
pane. With `beside` set, `stack` tabs into that page's pane and `split` splits off it. \
- `direction`: only for `split`; Omit `direction` in auto mode so vmux keeps terminal runs in the \
terminal bucket and spirals new panes predictably. \
- `terminal: <id>`: instead of opening anything, run IN that existing terminal (best for dependent / \
sequential steps that share one shell, in order). \
\
`focus` (default false = keep focus on your own pane) applies when opening a new terminal. The command \
is typed into an interactive shell, so the terminal stays usable afterwards."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["command"],
            "additionalProperties": false,
            "properties": {
                "command": {"type": "string"},
                "terminal": {"type": "string"},
                "beside": {"type": "string"},
                "mode": {"enum": ["auto", "split", "stack"]},
                "direction": {"enum": ["right", "left", "top", "bottom"]},
                "focus": {"type": "boolean"}
            }
        }),
    }
}

fn create_worktree_definition() -> ToolDefinition {
    ToolDefinition {
        name: "create_worktree".into(),
        description: "Call immediately before the first edit, write, test, build, or other project mutation, after a Git project is selected. Never call for requests that only read, show, search, or explain existing files. vmux reuses the current linked worktree, accepts a known existing worktree path, automatically uses a single unambiguous existing worktree, or creates a managed worktree when none exists. If multiple existing worktrees are returned as ambiguous, ask the user with request_user_choice to choose an existing path or Create new worktree; call again with path or create=true. Never run git worktree add manually. Returns the absolute worktree path."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "path": {"type": "string"},
                "branch": {"type": "string"},
                "task": {"type": "string"},
                "create": {"type": "boolean"}
            }
        }),
    }
}

fn select_project_definition() -> ToolDefinition {
    ToolDefinition {
        name: "select_project".into(),
        description: "Select an existing project before the first edit, write, test, build, or other project mutation. Never call for requests that only read, show, search, or explain existing files. Pass a known path or omit it to open the native project picker rooted at ~/.vmux/workspace. For a new project, first use request_user_choice to offer a concrete suggested location and Choose existing project; do not ask the user to invent a folder. Use ~/.vmux/workspace/<remote-host>/<organization>/<repository> when a remote is known and ~/.vmux/workspace/local/<project> otherwise. When creation is selected, use run only to create the empty directory, then call this tool with that path. vmux offers Git initialization and uses that new project root directly without a linked worktree. For a previously existing Git project, call create_worktree immediately before the first mutation. The request returns immediately when user selection is needed: stop the current turn and do not call again while pending. Do not search the user's home directory. Do not call for general questions or self-contained terminal demonstrations."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "path": {"type": "string"}
            }
        }),
    }
}

fn request_user_choice_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_user_choice".into(),
        description: "Show a native multiple-choice question in the agent conversation. For a new project without a selected project, use it to offer the concrete suggested ~/.vmux/workspace path or Choose existing project. Also use it for other user-requested options and ambiguous worktree selection. Keep options concise and actionable. The user can choose with arrow keys, Ctrl+N/Ctrl+P, number keys, mouse, or Enter. The request returns immediately: stop the current turn; vmux resumes the same conversation with the selected option."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["question", "options"],
            "additionalProperties": false,
            "properties": {
                "question": {"type": "string"},
                "options": {
                    "type": "array",
                    "minItems": 2,
                    "maxItems": 9,
                    "items": {"type": "string"}
                }
            }
        }),
    }
}

fn vault_status_definition() -> ToolDefinition {
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

fn open_vault_definition() -> ToolDefinition {
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

fn set_conversation_title_definition() -> ToolDefinition {
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

fn write_knowledge_definition() -> ToolDefinition {
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

fn search_knowledge_definition() -> ToolDefinition {
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

fn read_knowledge_definition() -> ToolDefinition {
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

fn resume_in_acp_definition() -> ToolDefinition {
    ToolDefinition {
        name: "resume_in_acp".into(),
        description: "Continue the current CLI conversation in its ACP chat runtime. Replaces this CLI page in place while preserving the session id and working directory. Call only when the user asks to switch or continue in ACP."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {}
        }),
    }
}

fn read_terminal_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_terminal".into(),
        description:
            "Return the current visible scrollback text of a terminal (the same text the user sees). \
Pass `terminal` = a terminal id returned by run, or a terminal stack's process_id from read_layout."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["terminal"],
            "additionalProperties": false,
            "properties": {
                "terminal": {"type": "string"}
            }
        }),
    }
}

fn screenshot_definition() -> ToolDefinition {
    ToolDefinition {
        name: "screenshot".into(),
        description: "Capture the vmux window as a PNG and return it inline so you can SEE the current UI \
(use it to verify your own UI changes). Captures the whole window exactly as it appears on screen - all \
visible panes (browser, terminal, editor) and layout chrome. Optionally pass `pane` (a pane:<id> or \
stack:<id> from read_layout) to crop to just that region. The full-resolution image is saved under \
the active vmux profile's recording directory and a downscaled copy is returned inline. macOS only; the first call may prompt for \
Screen Recording permission - grant it in System Settings > Privacy & Security > Screen Recording, then \
call again."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "pane": {
                    "type": "string",
                    "description": "Optional pane:<id> or stack:<id> to crop to; whole window if omitted."
                }
            }
        }),
    }
}

fn browser_snapshot_definition() -> ToolDefinition {
    ToolDefinition {
        name: "browser_snapshot".into(),
        description:
            "Read the current page's DOM as a compact semantic snapshot. Returns JSON with \
the page url/title and a list of interactive elements, each with a stable `ref`, `role`, `name`, \
`value`, `bbox` ([x,y,w,h] in CSS px), and `state` flags. Use the `ref` values to target later \
interaction tools. Pass `target` = a pane:<id> or stack:<id> from read_layout to pick a \
specific page; defaults to the focused page."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Optional pane:<id> or stack:<id>; if omitted, an agent caller's own browser pane (resolved via anchor), else the focused page."
                }
            }
        }),
    }
}

fn browser_scroll_definition() -> ToolDefinition {
    ToolDefinition {
        name: "browser_scroll".into(),
        description:
            "Scroll the visible browser page so the user can watch, then return the post-scroll \
snapshot (same shape as browser_snapshot, including viewport + inViewport flags). Pass exactly one \
of `to` (\"top\" or \"bottom\") or `delta` (pixels; positive = down, e.g. one screen is about the \
snapshot's viewport.height). Pass `target` = pane:<id> or stack:<id> to pick a page; defaults to \
the focused page. Prefer scrolling to read long pages instead of assuming off-screen content."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "to": {"enum": ["top", "bottom"], "description": "Scroll to page top or bottom. Pass exactly one of `to` or `delta`."},
                "delta": {
                    "type": "integer",
                    "minimum": i32::MIN,
                    "maximum": i32::MAX,
                    "description": "Scroll by pixels; positive = down. Pass exactly one of `to` or `delta`."
                },
                "target": {"type": "string", "description": "Optional pane:<id> or stack:<id>; if omitted, an agent caller's own browser pane (resolved via anchor), else the focused page."}
            }
        }),
    }
}

fn record_start_definition() -> ToolDefinition {
    ToolDefinition {
        name: "record_start".into(),
        description: "Start recording the vmux window to an mp4 video (optionally also a GIF). \
Returns immediately so you can drive the UI with other tools to demonstrate a feature, then call \
record_stop. Record in ONE live take: start, perform the few actions you want to show, then \
stop. Do NOT rehearse, build elaborate layouts, or take screenshots to verify - just capture the \
live interaction in a single pass. Auto-stops after `max_secs` (default 600) as a safety cap. Only \
one recording at a time. macOS only; the first call may prompt for Screen Recording permission - \
grant it in System Settings > Privacy & Security > Screen Recording, then call again."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "gif": {"type": "boolean", "description": "Also emit a GIF next to the mp4 (default false)."},
                "max_secs": {"type": "integer", "description": "Auto-stop cap in seconds (default 600)."},
                "pane": {"type": "string", "description": "Optional pane:<id> or stack:<id> to crop to; whole window if omitted."}
            }
        }),
    }
}

fn record_stop_definition() -> ToolDefinition {
    ToolDefinition {
        name: "record_stop".into(),
        description: "Stop the active recording and write the file(s). Returns the mp4 path, duration, \
and size (plus the GIF path if one was requested). By default saves to the active vmux profile's recording directory; pass `dir` \
(absolute) and `name` (basename, no extension) to save elsewhere - e.g. dir=<repo>/docs/recording, \
name=<feature> to drop a demo straight into the repo."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "dir": {"type": "string", "description": "Absolute output directory (default: active vmux profile recording directory)."},
                "name": {"type": "string", "description": "Output basename without extension (default vmux-<timestamp>)."}
            }
        }),
    }
}

fn bookmark_list_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_list".into(),
        description: "List all pins (favicon quick-access) and bookmarks (saved pages, \
optionally inside folders) for the current profile. Returns JSON: \
{pins:[{uuid,url,title,favicon_url}], roots:[ {kind:\"entry\",...} | \
{kind:\"folder\",uuid,name,collapsed,children:[...]} ]}."
            .into(),
        input_schema: serde_json::json!({"type":"object","properties":{},"additionalProperties":false}),
    }
}

fn bookmark_add_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_add".into(),
        description: "Save a page as a bookmark. Optional folder (a folder uuid from \
bookmark_list) nests it; omit for top level."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["url"],
            "additionalProperties": false,
            "properties": {
                "url": {"type": "string"},
                "title": {"type": "string"},
                "favicon_url": {"type": "string"},
                "folder": {"type": "string"}
            }
        }),
    }
}

fn bookmark_remove_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_remove".into(),
        description: "Remove a bookmark by its uuid (from bookmark_list).".into(),
        input_schema: serde_json::json!({
            "type":"object","required":["uuid"],"additionalProperties":false,
            "properties":{"uuid":{"type":"string"}}
        }),
    }
}

fn bookmark_pin_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_pin".into(),
        description: "Pin a page to the favicon grid. Provide a bookmark uuid to promote an \
existing bookmark, OR a url (+optional title/favicon_url) to pin a page directly."
            .into(),
        input_schema: serde_json::json!({
            "type":"object","additionalProperties":false,
            "properties":{
                "uuid":{"type":"string"},
                "url":{"type":"string"},
                "title":{"type":"string"},
                "favicon_url":{"type":"string"}
            }
        }),
    }
}

fn bookmark_unpin_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_unpin".into(),
        description: "Unpin a pin by its uuid (from bookmark_list).".into(),
        input_schema: serde_json::json!({
            "type":"object","required":["uuid"],"additionalProperties":false,
            "properties":{"uuid":{"type":"string"}}
        }),
    }
}

fn bookmark_folder_create_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_folder_create".into(),
        description: "Create a bookmark folder with the given name.".into(),
        input_schema: serde_json::json!({
            "type":"object","required":["name"],"additionalProperties":false,
            "properties":{"name":{"type":"string"}}
        }),
    }
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    tool_definitions_filtered(false, false)
}

/// Build the MCP tool list. ACP sessions omit the CLI-only runtime switch. When
/// `acp_terminals` is set, `run` + `read_terminal` are also omitted; `terminal_send` stays.
pub fn tool_definitions_filtered(acp_session: bool, acp_terminals: bool) -> Vec<ToolDefinition> {
    let mut defs: Vec<ToolDefinition> = vmux_command_mcp::tool_entries()
        .into_iter()
        .chain(McpParamTool::mcp_tool_entries())
        .map(|(name, description, schema)| ToolDefinition {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: schema,
        })
        .collect();
    defs.push(read_layout_definition());
    defs.push(update_layout_definition());
    defs.push(get_settings_definition());
    defs.push(list_spaces_definition());
    defs.push(open_page_definition());
    defs.push(open_file_definition());
    defs.push(read_file_definition());
    defs.push(grep_definition());
    if !acp_session {
        defs.push(resume_in_acp_definition());
    }
    if !acp_terminals {
        defs.push(run_definition());
    }
    defs.push(request_user_choice_definition());
    defs.push(vault_status_definition());
    defs.push(open_vault_definition());
    defs.push(set_conversation_title_definition());
    defs.push(search_knowledge_definition());
    defs.push(read_knowledge_definition());
    defs.push(write_knowledge_definition());
    defs.push(select_project_definition());
    defs.push(create_worktree_definition());
    if !acp_terminals {
        defs.push(read_terminal_definition());
    }
    defs.push(screenshot_definition());
    defs.push(browser_snapshot_definition());
    defs.push(browser_scroll_definition());
    defs.push(record_start_definition());
    defs.push(record_stop_definition());
    defs.push(bookmark_list_definition());
    defs.push(bookmark_add_definition());
    defs.push(bookmark_remove_definition());
    defs.push(bookmark_pin_definition());
    defs.push(bookmark_unpin_definition());
    defs.push(bookmark_folder_create_definition());
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
    use vmux_client::protocol::AgentPaneDirection;
    let name = name.strip_prefix("vmux_").unwrap_or(name);
    fn parse_direction(arguments: &Value) -> Result<Option<AgentPaneDirection>, String> {
        match arguments.get("direction").and_then(Value::as_str) {
            None => Ok(None),
            Some("right") => Ok(Some(AgentPaneDirection::Right)),
            Some("left") => Ok(Some(AgentPaneDirection::Left)),
            Some("top") => Ok(Some(AgentPaneDirection::Top)),
            Some("bottom") => Ok(Some(AgentPaneDirection::Bottom)),
            Some(other) => Err(format!("unknown direction: {other}")),
        }
    }
    if name == "resume_in_acp" {
        let anchor = anchor
            .ok_or("resume_in_acp requires an agent anchor (not available to this client)")?;
        return Ok(DispatchTarget::Command(AgentCommand::ResumeInAcp {
            anchor,
        }));
    }
    if name == "open_page" {
        let anchor =
            anchor.ok_or("open_page requires an agent anchor (not available to this client)")?;
        let url = arguments
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if url.trim().is_empty() {
            return Err("open_page.url is empty".to_string());
        }
        let direction = parse_direction(&arguments)?;
        let focus = arguments
            .get("focus")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return Ok(DispatchTarget::Command(AgentCommand::OpenBeside {
            anchor,
            direction,
            url,
            focus,
        }));
    }
    if name == "open_file" {
        let anchor =
            anchor.ok_or("open_file requires an agent anchor (not available to this client)")?;
        let path = arguments
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if path.is_empty() {
            return Err("open_file.path is empty".to_string());
        }
        let url = if path.starts_with("file:") {
            path
        } else {
            format!("file://{path}")
        };
        let direction = parse_direction(&arguments)?;
        let focus = arguments
            .get("focus")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return Ok(DispatchTarget::Command(AgentCommand::OpenBeside {
            anchor,
            direction,
            url,
            focus,
        }));
    }
    if name == "run" {
        let anchor = anchor.ok_or("run requires an agent anchor (not available to this client)")?;
        let command = arguments
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if command.trim().is_empty() {
            return Err("run.command is empty".to_string());
        }
        let placement_override = ["mode", "direction", "beside"]
            .iter()
            .any(|key| arguments.get(*key).is_some_and(|value| !value.is_null()));
        let direction = parse_direction(&arguments)?.unwrap_or(AgentPaneDirection::Right);
        let focus = arguments
            .get("focus")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let terminal = match arguments.get("terminal").and_then(Value::as_str) {
            Some(s) if !s.is_empty() => Some(
                s.parse::<vmux_client::protocol::ProcessId>()
                    .map_err(|_| format!("run.terminal is not a valid terminal id: {s}"))?,
            ),
            _ => None,
        };
        let beside = match arguments.get("beside").and_then(Value::as_str) {
            Some(s) if !s.is_empty() && s != "self" => Some(
                s.parse::<vmux_client::protocol::ProcessId>()
                    .map_err(|_| format!("run.beside is not a valid page id: {s}"))?,
            ),
            _ => None,
        };
        let mode = match arguments
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("auto")
        {
            "auto" => vmux_client::protocol::PlacementMode::Auto,
            "split" => vmux_client::protocol::PlacementMode::Split,
            "stack" => vmux_client::protocol::PlacementMode::Stack,
            other => return Err(format!("unknown mode: {other}")),
        };
        let command = if placement_override {
            AgentCommand::RunWithPlacementOverride {
                anchor,
                command,
                direction,
                focus,
                beside,
                mode,
                terminal,
                done_marker: None,
            }
        } else {
            AgentCommand::Run {
                anchor,
                command,
                direction,
                focus,
                beside,
                mode,
                terminal,
                done_marker: None,
            }
        };
        return Ok(DispatchTarget::Command(command));
    }
    if name == "create_worktree" {
        let anchor = anchor
            .ok_or("create_worktree requires an agent anchor (not available to this client)")?;
        let branch = arguments
            .get("branch")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|branch| !branch.is_empty());
        if let Some(branch) = branch {
            return Ok(DispatchTarget::Command(
                AgentCommand::CreateWorktreeOnBranch {
                    anchor,
                    branch: branch.to_string(),
                },
            ));
        }
        let string_arg = |name: &str| {
            arguments
                .get(name)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        return Ok(DispatchTarget::Command(AgentCommand::PrepareWorktree {
            anchor,
            path: string_arg("path"),
            task: string_arg("task"),
            create: arguments
                .get("create")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }));
    }
    if name == "request_user_choice" {
        let anchor = anchor
            .ok_or("request_user_choice requires an agent anchor (not available to this client)")?;
        let question = arguments
            .get("question")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|question| !question.is_empty())
            .ok_or("request_user_choice.question is empty")?;
        let options = arguments
            .get("options")
            .and_then(Value::as_array)
            .ok_or("request_user_choice.options must be an array")?
            .iter()
            .map(|option| {
                option
                    .as_str()
                    .map(str::trim)
                    .filter(|option| !option.is_empty())
                    .map(str::to_string)
                    .ok_or_else(|| {
                        "request_user_choice options must be non-empty strings".to_string()
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !(2..=9).contains(&options.len()) {
            return Err("request_user_choice requires 2 to 9 options".to_string());
        }
        return Ok(DispatchTarget::Command(AgentCommand::RequestUserChoice {
            anchor,
            question: question.to_string(),
            options,
        }));
    }
    if name == "open_vault" {
        let anchor =
            anchor.ok_or("open_vault requires an agent anchor (not available to this client)")?;
        let provider = arguments
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("overview");
        let url = match provider {
            "overview" => "vmux://vault/".to_string(),
            "github" => "vmux://vault/?provider=github".to_string(),
            "cloud_folder" => "vmux://vault/?provider=cloud_folder".to_string(),
            _ => {
                return Err("open_vault.provider must be overview, github, or cloud_folder".into());
            }
        };
        return Ok(DispatchTarget::Command(AgentCommand::OpenBeside {
            anchor,
            direction: None,
            url,
            focus: true,
        }));
    }
    if name == "set_conversation_title" {
        let anchor = anchor.ok_or(
            "set_conversation_title requires an agent anchor (not available to this client)",
        )?;
        let title = arguments
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .ok_or("set_conversation_title.title is empty")?;
        if title.chars().count() > 120 {
            return Err("set_conversation_title.title exceeds 120 characters".to_string());
        }
        return Ok(DispatchTarget::Command(
            AgentCommand::SetConversationTitle {
                anchor,
                title: title.to_string(),
            },
        ));
    }
    if name == "search_knowledge" {
        let anchor = anchor
            .ok_or("search_knowledge requires an agent anchor (not available to this client)")?;
        let query = arguments
            .get("query")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .ok_or("search_knowledge.query is empty")?;
        let limit = arguments.get("limit").and_then(Value::as_u64).unwrap_or(20);
        if !(1..=100).contains(&limit) {
            return Err("search_knowledge.limit must be between 1 and 100".to_string());
        }
        return Ok(DispatchTarget::Command(AgentCommand::SearchKnowledge {
            anchor,
            query: query.to_string(),
            limit: limit as u16,
        }));
    }
    if name == "read_knowledge" {
        let anchor = anchor
            .ok_or("read_knowledge requires an agent anchor (not available to this client)")?;
        let path = arguments
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .ok_or("read_knowledge.path is empty")?;
        let line = arguments.get("line").and_then(Value::as_u64).unwrap_or(1);
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(200);
        if line == 0 || line > u32::MAX as u64 {
            return Err("read_knowledge.line must be at least 1".to_string());
        }
        if !(1..=2_000).contains(&limit) {
            return Err("read_knowledge.limit must be between 1 and 2000".to_string());
        }
        return Ok(DispatchTarget::Command(AgentCommand::ReadKnowledge {
            anchor,
            path: path.to_string(),
            line: line as u32,
            limit: limit as u32,
        }));
    }
    if name == "write_knowledge" {
        let anchor = anchor
            .ok_or("write_knowledge requires an agent anchor (not available to this client)")?;
        let path = arguments
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_string);
        let title = arguments
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .ok_or("write_knowledge.title is empty")?;
        let content = arguments
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|content| !content.is_empty())
            .ok_or("write_knowledge.content is empty")?;
        return Ok(DispatchTarget::Command(AgentCommand::WriteKnowledge {
            anchor,
            path,
            title: title.to_string(),
            content: content.to_string(),
        }));
    }
    if matches!(
        name,
        "select_project" | "select_workspace" | "choose_workspace"
    ) {
        let anchor = anchor
            .ok_or("select_project requires an agent anchor (not available to this client)")?;
        if let Some(path) = arguments
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            return Ok(DispatchTarget::Command(
                AgentCommand::ChooseWorkspaceAtPath {
                    anchor,
                    path: path.to_string(),
                },
            ));
        }
        return Ok(DispatchTarget::Command(AgentCommand::ChooseWorkspace {
            anchor,
        }));
    }
    if name == "read_terminal" {
        let process_id = arguments
            .get("terminal")
            .and_then(Value::as_str)
            .unwrap_or("")
            .parse::<vmux_client::protocol::ProcessId>()
            .map_err(|_| "read_terminal.terminal must be a valid terminal id".to_string())?;
        return Ok(DispatchTarget::Query(
            vmux_client::protocol::AgentQuery::ReadTerminal { process_id },
        ));
    }
    if name == "screenshot" {
        let pane = match arguments.get("pane") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => {
                let s = s.trim();
                (!s.is_empty()).then(|| s.to_string())
            }
            Some(_) => return Err("screenshot.pane must be a string".to_string()),
        };
        return Ok(DispatchTarget::Query(
            vmux_client::protocol::AgentQuery::Screenshot { pane },
        ));
    }
    if name == "browser_snapshot" {
        let pane = match arguments.get("target") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => {
                let s = s.trim();
                (!s.is_empty()).then(|| s.to_string())
            }
            Some(_) => return Err("browser_snapshot.target must be a string".to_string()),
        };
        return Ok(DispatchTarget::Query(
            vmux_client::protocol::AgentQuery::BrowserSnapshot { pane, anchor },
        ));
    }
    if name == "browser_scroll" {
        let pane = match arguments.get("target") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => {
                let s = s.trim();
                (!s.is_empty()).then(|| s.to_string())
            }
            Some(_) => return Err("browser_scroll.target must be a string".to_string()),
        };
        let to = match arguments.get("to").and_then(Value::as_str) {
            None => None,
            Some(value @ ("top" | "bottom")) => Some(value.to_string()),
            Some(other) => {
                return Err(format!(
                    "browser_scroll.to must be 'top' or 'bottom', got {other}"
                ));
            }
        };
        let delta = match arguments.get("delta") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let n = value
                    .as_i64()
                    .ok_or("browser_scroll.delta must be an integer")?;
                let n = i32::try_from(n)
                    .map_err(|_| "browser_scroll.delta is out of range".to_string())?;
                Some(n)
            }
        };
        if to.is_some() == delta.is_some() {
            return Err("browser_scroll requires exactly one of `to` or `delta`".to_string());
        }
        return Ok(DispatchTarget::Query(
            vmux_client::protocol::AgentQuery::BrowserScroll {
                pane,
                to,
                delta,
                anchor,
            },
        ));
    }
    if name == "record_start" {
        let gif = arguments
            .get("gif")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let max_secs = arguments
            .get("max_secs")
            .and_then(Value::as_u64)
            .unwrap_or(600) as u32;
        let pane = match arguments.get("pane") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => {
                let s = s.trim();
                (!s.is_empty()).then(|| s.to_string())
            }
            Some(_) => return Err("record_start.pane must be a string".to_string()),
        };
        return Ok(DispatchTarget::Query(
            vmux_client::protocol::AgentQuery::RecordStart {
                gif,
                max_secs,
                pane,
            },
        ));
    }
    if name == "record_stop" {
        let parse_opt = |key: &str| match arguments.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => {
                let s = s.trim();
                Ok((!s.is_empty()).then(|| s.to_string()))
            }
            Some(_) => Err(format!("record_stop.{key} must be a string")),
        };
        let dir = parse_opt("dir")?;
        let out_name = parse_opt("name")?;
        return Ok(DispatchTarget::Query(
            vmux_client::protocol::AgentQuery::RecordStop {
                dir,
                name: out_name,
            },
        ));
    }
    if name == "read_layout" {
        return Ok(DispatchTarget::Query(
            vmux_client::protocol::AgentQuery::ReadLayout { anchor },
        ));
    }
    if name == "update_layout" {
        let layout: vmux_client::protocol::layout::LayoutSnapshot =
            serde_json::from_value(arguments)
                .map_err(|e| format!("update_layout: invalid layout payload: {e}"))?;
        return Ok(DispatchTarget::Command(AgentCommand::UpdateLayout {
            layout,
        }));
    }
    if name == "get_settings" {
        return Ok(DispatchTarget::Query(
            vmux_client::protocol::AgentQuery::GetSettings,
        ));
    }
    if name == "list_spaces" {
        return Ok(DispatchTarget::Query(
            vmux_client::protocol::AgentQuery::ListSpaces,
        ));
    }
    if name == "bookmark_list" {
        return Ok(DispatchTarget::Query(
            vmux_client::protocol::AgentQuery::BookmarkList,
        ));
    }
    {
        let str_arg = |key: &str| {
            arguments
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let bookmark_cmd = |command: &str| {
            DispatchTarget::Command(AgentCommand::BookmarkCommand {
                command: command.to_string(),
                uuid: str_arg("uuid"),
                name: str_arg("name"),
                url: str_arg("url"),
                title: str_arg("title"),
                favicon_url: str_arg("favicon_url"),
            })
        };
        match name {
            "bookmark_add" => {
                if str_arg("url").unwrap_or_default().is_empty() {
                    return Err("bookmark_add.url is required".to_string());
                }
                return Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand {
                    command: "add".to_string(),
                    uuid: str_arg("folder"),
                    name: None,
                    url: str_arg("url"),
                    title: str_arg("title"),
                    favicon_url: str_arg("favicon_url"),
                }));
            }
            "bookmark_remove" => return Ok(bookmark_cmd("remove")),
            "bookmark_pin" => return Ok(bookmark_cmd("pin")),
            "bookmark_unpin" => return Ok(bookmark_cmd("unpin")),
            "bookmark_folder_create" => {
                if str_arg("name").unwrap_or_default().is_empty() {
                    return Err("bookmark_folder_create.name is required".to_string());
                }
                return Ok(bookmark_cmd("folder_create"));
            }
            _ => {}
        }
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
#[path = "tools.test.rs"]
mod tests;
