use serde::Serialize;
use serde_json::Value;
use vmux_command::command::AppCommand;
use vmux_macro::McpTool;
use vmux_service::protocol::AgentCommand;

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
    Query(vmux_service::protocol::AgentQuery),
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
(`terminal: <id>`, `exit: <code>`, `output: ...`). If it has not finished within ~50s, returns the \
output so far with a note to call read_terminal for the rest. \
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
        description:
            "Before making changes, isolate this task in its own git worktree so your work lands on \
a dedicated branch and never disturbs the user's main checkout. Call this once at the start when \
the working directory is a git repository. Creates (or reuses) a worktree for this tab and returns \
its absolute path — do your work there. No-op-safe: returns the existing path if already isolated."
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
~/.vmux/recording/ and a downscaled copy is returned inline. macOS only; the first call may prompt for \
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
and size (plus the GIF path if one was requested). By default saves to ~/.vmux/profiles/<profile>/recording/; pass `dir` \
(absolute) and `name` (basename, no extension) to save elsewhere - e.g. dir=<repo>/docs/recording, \
name=<feature> to drop a demo straight into the repo."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "dir": {"type": "string", "description": "Absolute output directory (default ~/.vmux/profiles/<profile>/recording)."},
                "name": {"type": "string", "description": "Output basename without extension (default vmux-<timestamp>)."}
            }
        }),
    }
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    tool_definitions_filtered(false)
}

/// Build the MCP tool list. When `acp_terminals` is set (ACP sessions, which get terminals through
/// the ACP terminal methods), `run` + `read_terminal` are omitted; `terminal_send` (no ACP
/// equivalent for keystrokes/TUIs) is always kept.
pub fn tool_definitions_filtered(acp_terminals: bool) -> Vec<ToolDefinition> {
    let mut defs: Vec<ToolDefinition> = AppCommand::mcp_tool_entries()
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
    if !acp_terminals {
        defs.push(run_definition());
    }
    defs.push(create_worktree_definition());
    if !acp_terminals {
        defs.push(read_terminal_definition());
    }
    defs.push(screenshot_definition());
    defs.push(browser_snapshot_definition());
    defs.push(browser_scroll_definition());
    defs.push(record_start_definition());
    defs.push(record_stop_definition());
    defs
}

pub fn dispatch_from_tool_call(name: &str, arguments: Value) -> Result<DispatchTarget, String> {
    dispatch_with_anchor(name, arguments, None)
}

pub fn dispatch_with_anchor(
    name: &str,
    arguments: Value,
    anchor: Option<vmux_service::protocol::ProcessId>,
) -> Result<DispatchTarget, String> {
    use vmux_service::protocol::AgentPaneDirection;
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
                s.parse::<vmux_service::protocol::ProcessId>()
                    .map_err(|_| format!("run.terminal is not a valid terminal id: {s}"))?,
            ),
            _ => None,
        };
        let beside = match arguments.get("beside").and_then(Value::as_str) {
            Some(s) if !s.is_empty() && s != "self" => Some(
                s.parse::<vmux_service::protocol::ProcessId>()
                    .map_err(|_| format!("run.beside is not a valid page id: {s}"))?,
            ),
            _ => None,
        };
        let mode = match arguments
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("auto")
        {
            "auto" => vmux_service::protocol::PlacementMode::Auto,
            "split" => vmux_service::protocol::PlacementMode::Split,
            "stack" => vmux_service::protocol::PlacementMode::Stack,
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
        return Ok(DispatchTarget::Command(AgentCommand::CreateWorktree {
            anchor,
        }));
    }
    if name == "read_terminal" {
        let process_id = arguments
            .get("terminal")
            .and_then(Value::as_str)
            .unwrap_or("")
            .parse::<vmux_service::protocol::ProcessId>()
            .map_err(|_| "read_terminal.terminal must be a valid terminal id".to_string())?;
        return Ok(DispatchTarget::Query(
            vmux_service::protocol::AgentQuery::ReadTerminal { process_id },
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
            vmux_service::protocol::AgentQuery::Screenshot { pane },
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
            vmux_service::protocol::AgentQuery::BrowserSnapshot { pane, anchor },
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
            vmux_service::protocol::AgentQuery::BrowserScroll {
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
            vmux_service::protocol::AgentQuery::RecordStart {
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
            vmux_service::protocol::AgentQuery::RecordStop {
                dir,
                name: out_name,
            },
        ));
    }
    if name == "read_layout" {
        return Ok(DispatchTarget::Query(
            vmux_service::protocol::AgentQuery::ReadLayout { anchor },
        ));
    }
    if name == "update_layout" {
        let layout: vmux_service::protocol::layout::LayoutSnapshot =
            serde_json::from_value(arguments)
                .map_err(|e| format!("update_layout: invalid layout payload: {e}"))?;
        return Ok(DispatchTarget::Command(AgentCommand::UpdateLayout {
            layout,
        }));
    }
    if name == "get_settings" {
        return Ok(DispatchTarget::Query(
            vmux_service::protocol::AgentQuery::GetSettings,
        ));
    }
    if name == "list_spaces" {
        return Ok(DispatchTarget::Query(
            vmux_service::protocol::AgentQuery::ListSpaces,
        ));
    }
    if let Some(parsed) = McpParamTool::from_mcp_call(name, arguments.clone()) {
        return parsed
            .and_then(McpParamTool::to_agent_command)
            .map(DispatchTarget::Command);
    }
    if AppCommand::from_mcp_id(name).is_some() {
        return Ok(DispatchTarget::Command(AgentCommand::AppCommand {
            id: name.to_string(),
            args_json: String::new(),
        }));
    }
    if AppCommand::from_mcp_call(name, arguments.clone()).is_some() {
        let args_json = serde_json::to_string(&arguments).unwrap_or_default();
        return Ok(DispatchTarget::Command(AgentCommand::AppCommand {
            id: name.to_string(),
            args_json,
        }));
    }
    Err(format!("unknown tool: {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_service::protocol::{AgentCommand, AgentQuery};

    fn tool_names() -> Vec<String> {
        tool_definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect()
    }

    fn dispatch_command(name: &str, args: serde_json::Value) -> Result<AgentCommand, String> {
        match dispatch_from_tool_call(name, args)? {
            DispatchTarget::Command(cmd) => Ok(cmd),
            DispatchTarget::Query(_) => Err("expected Command, got Query".to_string()),
        }
    }

    fn dispatch_query(name: &str, args: serde_json::Value) -> Result<AgentQuery, String> {
        match dispatch_from_tool_call(name, args)? {
            DispatchTarget::Query(q) => Ok(q),
            DispatchTarget::Command(_) => Err("expected Query, got Command".to_string()),
        }
    }

    #[test]
    fn record_tools_are_listed() {
        let names = tool_names();
        assert!(names.contains(&"record_start".to_string()));
        assert!(names.contains(&"record_stop".to_string()));
    }

    #[test]
    fn browser_snapshot_dispatches_to_query_with_pane() {
        let q = dispatch_query(
            "browser_snapshot",
            serde_json::json!({ "target": "pane:42" }),
        )
        .unwrap();
        assert_eq!(
            q,
            AgentQuery::BrowserSnapshot {
                pane: Some("pane:42".to_string()),
                anchor: None,
            }
        );
    }

    #[test]
    fn browser_snapshot_defaults_pane_to_none() {
        let q = dispatch_query("browser_snapshot", serde_json::json!({})).unwrap();
        assert_eq!(
            q,
            AgentQuery::BrowserSnapshot {
                pane: None,
                anchor: None,
            }
        );
    }

    #[test]
    fn browser_snapshot_is_listed() {
        assert!(tool_names().contains(&"browser_snapshot".to_string()));
    }

    #[test]
    fn browser_snapshot_rejects_non_string_target() {
        let err =
            dispatch_query("browser_snapshot", serde_json::json!({ "target": 123 })).unwrap_err();
        assert!(err.contains("target"));
    }

    #[test]
    fn browser_scroll_dispatches_with_delta() {
        let q = dispatch_query("browser_scroll", serde_json::json!({ "delta": 600 })).unwrap();
        assert_eq!(
            q,
            AgentQuery::BrowserScroll {
                pane: None,
                to: None,
                delta: Some(600),
                anchor: None,
            }
        );
    }

    #[test]
    fn browser_scroll_dispatches_to_bottom_with_pane() {
        let q = dispatch_query(
            "browser_scroll",
            serde_json::json!({ "to": "bottom", "target": "pane:3" }),
        )
        .unwrap();
        assert_eq!(
            q,
            AgentQuery::BrowserScroll {
                pane: Some("pane:3".to_string()),
                to: Some("bottom".to_string()),
                delta: None,
                anchor: None,
            }
        );
    }

    #[test]
    fn browser_scroll_requires_exactly_one_of_to_or_delta() {
        assert!(dispatch_query("browser_scroll", serde_json::json!({})).is_err());
        assert!(
            dispatch_query(
                "browser_scroll",
                serde_json::json!({ "to": "top", "delta": 5 })
            )
            .is_err()
        );
    }

    #[test]
    fn browser_scroll_rejects_non_integer_or_out_of_range_delta() {
        let err =
            dispatch_query("browser_scroll", serde_json::json!({ "delta": "600" })).unwrap_err();
        assert!(err.contains("delta must be an integer"));
        let err = dispatch_query(
            "browser_scroll",
            serde_json::json!({ "delta": 5_000_000_000i64 }),
        )
        .unwrap_err();
        assert!(err.contains("out of range"));
    }

    #[test]
    fn browser_scroll_is_listed() {
        assert!(tool_names().contains(&"browser_scroll".to_string()));
    }

    #[test]
    fn install_extension_is_listed() {
        assert!(tool_names().contains(&"browser_install_extension".to_string()));
    }

    #[test]
    fn install_extension_dispatches_with_source() {
        let cmd = dispatch_command(
            "browser_install_extension",
            serde_json::json!({ "source": "cjpalhdlnbpafiamejdnhcphjbkeiagm" }),
        )
        .unwrap();
        assert_eq!(
            cmd,
            AgentCommand::BrowserInstallExtension {
                source: "cjpalhdlnbpafiamejdnhcphjbkeiagm".to_string()
            }
        );
    }

    #[test]
    fn install_extension_rejects_empty_source() {
        let err = dispatch_command(
            "browser_install_extension",
            serde_json::json!({ "source": "  " }),
        )
        .unwrap_err();
        assert!(err.contains("source"));
    }

    #[test]
    fn record_start_dispatch_defaults() {
        let q = dispatch_query("record_start", serde_json::json!({})).unwrap();
        assert_eq!(
            q,
            AgentQuery::RecordStart {
                gif: false,
                max_secs: 600,
                pane: None
            }
        );
    }

    #[test]
    fn record_start_dispatch_args() {
        let q = dispatch_query(
            "record_start",
            serde_json::json!({"gif": true, "max_secs": 30, "pane": "pane:3"}),
        )
        .unwrap();
        assert_eq!(
            q,
            AgentQuery::RecordStart {
                gif: true,
                max_secs: 30,
                pane: Some("pane:3".into())
            }
        );
    }

    #[test]
    fn record_stop_dispatch_args() {
        let q = dispatch_query(
            "record_stop",
            serde_json::json!({"dir": "/tmp/out", "name": "feature-x"}),
        )
        .unwrap();
        assert_eq!(
            q,
            AgentQuery::RecordStop {
                dir: Some("/tmp/out".into()),
                name: Some("feature-x".into())
            }
        );
        let empty = dispatch_query("record_stop", serde_json::json!({})).unwrap();
        assert_eq!(
            empty,
            AgentQuery::RecordStop {
                dir: None,
                name: None
            }
        );
    }

    #[test]
    fn list_tools_includes_auto_generated_and_handwritten() {
        let names = tool_names();

        for hand in ["open_command_bar", "open_page", "run", "read_terminal"] {
            assert!(
                names.contains(&hand.to_string()),
                "missing hand-written {hand}"
            );
        }
        for removed_tool in ["new_terminal_tab", "run_shell", "in_pane"] {
            assert!(
                !names.contains(&removed_tool.to_string()),
                "superseded tool {removed_tool} should no longer appear in MCP tools"
            );
        }
        for auto in ["terminal_clear", "browser_reload"] {
            assert!(
                names.contains(&auto.to_string()),
                "missing auto-generated {auto}"
            );
        }
        assert!(
            names.iter().all(|n| !n.starts_with("vmux_")),
            "MCP tool names must not be vmux_-prefixed (server is already named vmux): {names:?}"
        );
        for removed in ["stack_new", "close_tab", "split_v"] {
            assert!(
                !names.contains(&removed.to_string()),
                "layout command {removed} should no longer appear in MCP tools"
            );
        }
    }

    #[test]
    fn pane_open_tool_descriptions_prefer_auto_placement() {
        let defs = tool_definitions();
        let open_page = defs.iter().find(|tool| tool.name == "open_page").unwrap();
        let open_file = defs.iter().find(|tool| tool.name == "open_file").unwrap();
        let run = defs.iter().find(|tool| tool.name == "run").unwrap();

        assert!(open_page.description.contains("Omit `direction`"));
        assert!(open_file.description.contains("Omit `direction`"));
        assert!(run.description.contains("Omit `direction`"));
    }

    #[test]
    fn auto_generated_tool_dispatches_as_app_command() {
        let command = dispatch_command("terminal_clear", serde_json::json!({})).unwrap();
        assert_eq!(
            command,
            AgentCommand::AppCommand {
                id: "terminal_clear".to_string(),
                args_json: String::new(),
            }
        );
    }

    #[test]
    fn unknown_tool_returns_error() {
        assert!(dispatch_from_tool_call("nope_not_a_tool", serde_json::json!({})).is_err());
    }

    #[test]
    fn list_tools_includes_notify() {
        assert!(tool_names().contains(&"notify".to_string()));
    }

    #[test]
    fn notify_dispatches_to_notify_command() {
        let command = dispatch_command(
            "notify",
            serde_json::json!({"title": "done", "body": "built X"}),
        )
        .unwrap();
        assert_eq!(
            command,
            AgentCommand::Notify {
                title: Some("done".to_string()),
                body: Some("built X".to_string()),
            }
        );
    }

    #[test]
    fn notify_allows_empty_args() {
        let command = dispatch_command("notify", serde_json::json!({})).unwrap();
        assert_eq!(
            command,
            AgentCommand::Notify {
                title: None,
                body: None,
            }
        );
    }

    #[test]
    fn list_tools_includes_browser_navigate() {
        let names = tool_names();
        assert!(names.contains(&"browser_navigate".to_string()));
    }

    #[test]
    fn browser_navigate_dispatches_with_url() {
        let command = dispatch_command(
            "browser_navigate",
            serde_json::json!({"url": "https://example.com"}),
        )
        .unwrap();
        assert_eq!(
            command,
            AgentCommand::BrowserNavigate {
                url: "https://example.com".to_string(),
                pane: None,
            }
        );
    }

    #[test]
    fn browser_navigate_missing_url_returns_error() {
        assert!(dispatch_from_tool_call("browser_navigate", serde_json::json!({})).is_err());
    }

    #[test]
    fn vmux_prefixed_tool_name_dispatches() {
        let command = dispatch_command(
            "vmux_browser_navigate",
            serde_json::json!({"url": "https://example.com"}),
        )
        .unwrap();
        assert_eq!(
            command,
            AgentCommand::BrowserNavigate {
                url: "https://example.com".to_string(),
                pane: None,
            }
        );
    }

    #[test]
    fn list_tools_includes_terminal_send() {
        let names = tool_names();
        assert!(names.contains(&"terminal_send".to_string()));
    }

    #[test]
    fn acp_terminals_toolset_hides_run_and_read_terminal_keeps_send() {
        let names: Vec<String> = tool_definitions_filtered(true)
            .into_iter()
            .map(|def| def.name)
            .collect();
        assert!(!names.contains(&"run".to_string()));
        assert!(!names.contains(&"read_terminal".to_string()));
        assert!(names.contains(&"terminal_send".to_string()));
        assert!(names.contains(&"open_page".to_string()));
    }

    #[test]
    fn terminal_send_dispatches_with_text() {
        let command = dispatch_command("terminal_send", serde_json::json!({"text": "ls"})).unwrap();
        assert_eq!(
            command,
            AgentCommand::TerminalSend {
                text: "ls".to_string(),
                terminal: None,
            }
        );
    }

    #[test]
    fn terminal_send_enter_appends_carriage_return() {
        let command = dispatch_command(
            "terminal_send",
            serde_json::json!({"text": "ls", "enter": true}),
        )
        .unwrap();
        assert_eq!(
            command,
            AgentCommand::TerminalSend {
                text: "ls\r".to_string(),
                terminal: None,
            }
        );
    }

    #[test]
    fn terminal_send_enter_with_empty_text_submits_carriage_return() {
        let command = dispatch_command(
            "terminal_send",
            serde_json::json!({"text": "", "enter": true}),
        )
        .unwrap();
        assert_eq!(
            command,
            AgentCommand::TerminalSend {
                text: "\r".to_string(),
                terminal: None,
            }
        );
    }

    #[test]
    fn terminal_send_missing_text_returns_error() {
        assert!(dispatch_from_tool_call("terminal_send", serde_json::json!({})).is_err());
    }

    #[test]
    fn rename_profile_dispatches_with_name() {
        let command =
            dispatch_command("rename_profile", serde_json::json!({"name": "Junichi"})).unwrap();
        assert_eq!(
            command,
            AgentCommand::RenameProfile {
                name: "Junichi".to_string()
            }
        );
    }

    #[test]
    fn rename_profile_empty_name_returns_error() {
        assert!(
            dispatch_from_tool_call("rename_profile", serde_json::json!({"name": "  "})).is_err()
        );
    }

    #[test]
    fn list_tools_includes_select_tab() {
        let names = tool_names();
        assert!(names.contains(&"select_tab".to_string()));
    }

    #[test]
    fn select_tab_dispatches_to_tab_select_id() {
        let command = dispatch_command("select_tab", serde_json::json!({"index": 3})).unwrap();
        assert_eq!(
            command,
            AgentCommand::AppCommand {
                id: "tab_select_3".to_string(),
                args_json: String::new(),
            }
        );
    }

    #[test]
    fn select_tab_out_of_range_returns_error() {
        assert!(dispatch_from_tool_call("select_tab", serde_json::json!({"index": 0})).is_err());
        assert!(dispatch_from_tool_call("select_tab", serde_json::json!({"index": 9})).is_err());
    }

    #[test]
    fn tool_list_includes_read_and_update_layout() {
        let names = tool_names();
        assert!(names.contains(&"read_layout".to_string()));
        assert!(names.contains(&"update_layout".to_string()));
    }

    #[test]
    fn list_tools_includes_screenshot() {
        assert!(tool_names().contains(&"screenshot".to_string()));
    }

    #[test]
    fn screenshot_dispatches_to_query_with_and_without_pane() {
        let target = dispatch_from_tool_call("screenshot", serde_json::json!({})).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(vmux_service::protocol::AgentQuery::Screenshot { pane: None })
        ));

        let target =
            dispatch_from_tool_call("screenshot", serde_json::json!({ "pane": "stack:7" }))
                .unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(vmux_service::protocol::AgentQuery::Screenshot { pane: Some(p) })
                if p == "stack:7"
        ));

        let target =
            dispatch_from_tool_call("screenshot", serde_json::json!({ "pane": "  " })).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(vmux_service::protocol::AgentQuery::Screenshot { pane: None })
        ));

        assert!(dispatch_from_tool_call("screenshot", serde_json::json!({ "pane": 123 })).is_err());
    }

    #[test]
    fn mcp_param_tool_entries_includes_all_param_tools() {
        let names: Vec<&'static str> = McpParamTool::mcp_tool_entries()
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();
        for expected in [
            "open_command_bar",
            "browser_navigate",
            "terminal_send",
            "select_tab",
        ] {
            assert!(names.contains(&expected), "missing param tool {expected}");
        }
    }

    #[test]
    fn mcp_param_tool_browser_navigate_schema_marks_url_required() {
        let entry = McpParamTool::mcp_tool_entries()
            .into_iter()
            .find(|(name, _, _)| *name == "browser_navigate")
            .expect("browser_navigate present");
        let schema = entry.2;
        let required = schema.get("required").expect("required key");
        assert_eq!(required, &serde_json::json!(["url"]));
        let properties = schema.get("properties").expect("properties key");
        assert!(properties.get("url").is_some());
        assert!(properties.get("pane").is_some());
    }

    #[test]
    fn mcp_param_tool_from_mcp_call_browser_navigate() {
        let parsed = McpParamTool::from_mcp_call(
            "browser_navigate",
            serde_json::json!({"url": "https://example.com", "pane": "12345"}),
        )
        .expect("recognized")
        .expect("parsed");
        assert!(matches!(
            parsed,
            McpParamTool::BrowserNavigate { url, pane: Some(p) }
                if url == "https://example.com" && p == "12345"
        ));
    }

    #[test]
    fn mcp_param_tool_from_mcp_call_browser_navigate_missing_url_errors() {
        let result = McpParamTool::from_mcp_call("browser_navigate", serde_json::json!({}))
            .expect("recognized");
        assert!(result.is_err());
    }

    #[test]
    fn mcp_param_tool_from_mcp_call_unknown_returns_none() {
        assert!(McpParamTool::from_mcp_call("nope", serde_json::json!({})).is_none());
    }

    #[test]
    fn dispatch_from_tool_call_routes_command() {
        let target = dispatch_from_tool_call("terminal_clear", serde_json::json!({})).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Command(AgentCommand::AppCommand { id, .. }) if id == "terminal_clear"
        ));
    }

    #[test]
    fn dispatch_read_layout_routes_to_query() {
        let target = dispatch_from_tool_call("read_layout", serde_json::json!({})).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(AgentQuery::ReadLayout { .. })
        ));
    }

    #[test]
    fn open_page_without_direction_is_auto() {
        let anchor = vmux_service::protocol::ProcessId::new();
        let target = dispatch_with_anchor(
            "open_page",
            serde_json::json!({"url": "https://x.com"}),
            Some(anchor),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::OpenBeside { direction, .. }) => {
                assert_eq!(direction, None, "absent direction => auto placement");
            }
            other => panic!("expected OpenBeside, got {other:?}"),
        }
    }

    #[test]
    fn open_page_default_does_not_request_focus() {
        let anchor = vmux_service::protocol::ProcessId::new();
        let target = dispatch_with_anchor(
            "open_page",
            serde_json::json!({"url": "https://x.com"}),
            Some(anchor),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::OpenBeside { focus, .. }) => {
                assert!(!focus);
            }
            other => panic!("expected OpenBeside, got {other:?}"),
        }
    }

    #[test]
    fn open_file_default_does_not_request_focus() {
        let anchor = vmux_service::protocol::ProcessId::new();
        let target = dispatch_with_anchor(
            "open_file",
            serde_json::json!({"path": "/tmp/example.rs"}),
            Some(anchor),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::OpenBeside { focus, .. }) => {
                assert!(!focus);
            }
            other => panic!("expected OpenBeside, got {other:?}"),
        }
    }

    #[test]
    fn open_page_with_direction_is_explicit() {
        let anchor = vmux_service::protocol::ProcessId::new();
        let target = dispatch_with_anchor(
            "open_page",
            serde_json::json!({"url": "https://x.com", "direction": "left"}),
            Some(anchor),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::OpenBeside { direction, .. }) => {
                assert_eq!(
                    direction,
                    Some(vmux_service::protocol::AgentPaneDirection::Left)
                );
            }
            other => panic!("expected OpenBeside, got {other:?}"),
        }
    }

    #[test]
    fn open_page_dispatch_uses_anchor() {
        let anchor = vmux_service::protocol::ProcessId::new();
        let target = dispatch_with_anchor(
            "open_page",
            serde_json::json!({"direction": "right", "url": "vmux://terminal/"}),
            Some(anchor),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::OpenBeside { anchor: a, url, .. }) => {
                assert_eq!(a, anchor);
                assert_eq!(url, "vmux://terminal/");
            }
            other => panic!("expected OpenBeside, got {other:?}"),
        }
        assert!(
            dispatch_with_anchor("open_page", serde_json::json!({"url": ""}), Some(anchor))
                .is_err()
        );
        assert!(dispatch_with_anchor("open_page", serde_json::json!({"url": "x"}), None).is_err());
        assert!(tool_definitions().iter().any(|d| d.name == "open_page"));
        assert!(tool_definitions().iter().any(|d| d.name == "run"));
        assert!(tool_definitions().iter().any(|d| d.name == "read_file"));
        assert!(tool_definitions().iter().any(|d| d.name == "grep"));
    }

    #[test]
    fn run_dispatch_uses_anchor() {
        let anchor = vmux_service::protocol::ProcessId::new();
        let target = dispatch_with_anchor(
            "run",
            serde_json::json!({"command": "echo hi"}),
            Some(anchor),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::Run {
                anchor: a, command, ..
            }) => {
                assert_eq!(a, anchor);
                assert_eq!(command, "echo hi");
            }
            other => panic!("expected Run, got {other:?}"),
        }
        assert!(
            dispatch_with_anchor("run", serde_json::json!({"command": " "}), Some(anchor)).is_err()
        );
        assert!(dispatch_with_anchor("run", serde_json::json!({"command": "x"}), None).is_err());
    }

    #[test]
    fn run_dispatch_tracks_explicit_placement_override() {
        let anchor = vmux_service::protocol::ProcessId::new();
        let bare = dispatch_with_anchor(
            "run",
            serde_json::json!({"command": "echo hi"}),
            Some(anchor),
        )
        .unwrap();
        match bare {
            DispatchTarget::Command(AgentCommand::Run { .. }) => {}
            other => panic!("expected Run, got {other:?}"),
        }

        let nulls = dispatch_with_anchor(
            "run",
            serde_json::json!({
                "command": "echo hi",
                "mode": null,
                "direction": null,
                "beside": null
            }),
            Some(anchor),
        )
        .unwrap();
        match nulls {
            DispatchTarget::Command(AgentCommand::Run { .. }) => {}
            other => panic!("expected Run for null placement values, got {other:?}"),
        }

        for arguments in [
            serde_json::json!({"command": "echo hi", "direction": "bottom"}),
            serde_json::json!({"command": "echo hi", "mode": "auto"}),
            serde_json::json!({"command": "echo hi", "beside": "self"}),
        ] {
            let explicit = dispatch_with_anchor("run", arguments, Some(anchor)).unwrap();
            match explicit {
                DispatchTarget::Command(AgentCommand::RunWithPlacementOverride { .. }) => {}
                other => panic!("expected RunWithPlacementOverride, got {other:?}"),
            }
        }
    }

    #[test]
    fn run_tool_documents_default_placement_policy() {
        let run = tool_definitions()
            .into_iter()
            .find(|definition| definition.name == "run")
            .expect("run definition");
        assert!(
            run.description
                .contains("agent.allow_run_placement_override")
        );
        assert!(
            run.description
                .contains("omit `mode`, `direction`, and `beside`")
        );
    }

    #[test]
    fn run_with_terminal_targets_existing() {
        let anchor = vmux_service::protocol::ProcessId::new();
        let term = vmux_service::protocol::ProcessId::new();
        let target = dispatch_with_anchor(
            "run",
            serde_json::json!({"command": "ls", "terminal": term.to_string()}),
            Some(anchor),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::Run {
                terminal: Some(t), ..
            }) => {
                assert_eq!(t, term);
            }
            other => panic!("expected Run with terminal, got {other:?}"),
        }
        assert!(
            dispatch_with_anchor(
                "run",
                serde_json::json!({"command": "ls", "terminal": "nope"}),
                Some(anchor)
            )
            .is_err()
        );
    }

    #[test]
    fn run_beside_and_mode_dispatch() {
        use vmux_service::protocol::PlacementMode;
        let anchor = vmux_service::protocol::ProcessId::new();
        let beside = vmux_service::protocol::ProcessId::new();

        // beside=<id> + mode=stack carries through.
        let target = dispatch_with_anchor(
            "run",
            serde_json::json!({"command": "ls", "beside": beside.to_string(), "mode": "stack"}),
            Some(anchor),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::RunWithPlacementOverride {
                beside: Some(b),
                mode,
                ..
            }) => {
                assert_eq!(b, beside);
                assert_eq!(mode, PlacementMode::Stack);
            }
            other => panic!("expected RunWithPlacementOverride with beside+stack, got {other:?}"),
        }

        // beside="self" => None; mode defaults to Auto (reuse the region).
        let target = dispatch_with_anchor(
            "run",
            serde_json::json!({"command": "ls", "beside": "self"}),
            Some(anchor),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::RunWithPlacementOverride {
                beside: None,
                mode,
                ..
            }) => assert_eq!(mode, PlacementMode::Auto),
            other => panic!("expected RunWithPlacementOverride with self+auto, got {other:?}"),
        }

        // explicit mode=split is honored.
        let target = dispatch_with_anchor(
            "run",
            serde_json::json!({"command": "ls", "mode": "split"}),
            Some(anchor),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::RunWithPlacementOverride { mode, .. }) => {
                assert_eq!(mode, PlacementMode::Split)
            }
            other => panic!("expected RunWithPlacementOverride with split, got {other:?}"),
        }

        // unknown mode errors.
        assert!(
            dispatch_with_anchor(
                "run",
                serde_json::json!({"command": "ls", "mode": "nope"}),
                Some(anchor),
            )
            .is_err()
        );
    }

    #[test]
    fn read_terminal_dispatch_routes_to_query() {
        let pid = vmux_service::protocol::ProcessId::new();
        let target = dispatch_from_tool_call(
            "read_terminal",
            serde_json::json!({"terminal": pid.to_string()}),
        )
        .unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(vmux_service::protocol::AgentQuery::ReadTerminal { .. })
        ));
        assert!(
            dispatch_from_tool_call("read_terminal", serde_json::json!({"terminal": "bad"}))
                .is_err()
        );
        assert!(tool_definitions().iter().any(|d| d.name == "read_terminal"));
    }

    #[test]
    fn dispatch_update_layout_parses_payload() {
        let payload = serde_json::json!({
            "tabs": [{
                "id": "tab:1",
                "name": "Work",
                "is_active": true,
                "root": { "kind": "pane", "id": "pane:2", "stacks": [{ "id": "stack:3" }] }
            }],
            "focused": { "tab": "tab:1", "pane": "pane:2", "stack": "stack:3" }
        });
        let target = dispatch_from_tool_call("update_layout", payload).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Command(AgentCommand::UpdateLayout { .. })
        ));
    }

    #[test]
    fn dispatch_update_layout_rejects_malformed_payload() {
        let payload = serde_json::json!({ "not_a_layout": true });
        assert!(dispatch_from_tool_call("update_layout", payload).is_err());
    }

    #[test]
    fn dispatch_from_tool_call_routes_param_command_with_pane() {
        let target = dispatch_from_tool_call(
            "browser_navigate",
            serde_json::json!({"url": "https://example.com", "pane": "12345"}),
        )
        .unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Command(AgentCommand::BrowserNavigate { url, pane: Some(p) })
                if url == "https://example.com" && p == "12345"
        ));
    }

    #[test]
    fn dispatch_from_tool_call_unknown_returns_error() {
        assert!(dispatch_from_tool_call("nope", serde_json::json!({})).is_err());
    }

    #[test]
    fn list_tools_includes_update_settings_and_get_settings() {
        let names = tool_names();
        assert!(names.contains(&"update_settings".to_string()));
        assert!(names.contains(&"get_settings".to_string()));
    }

    #[test]
    fn update_settings_dispatches_with_path_and_value() {
        let target = dispatch_from_tool_call(
            "update_settings",
            serde_json::json!({"path": "layout.pane.gap", "value": 12.0}),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::UpdateSettings { path, value_json }) => {
                assert_eq!(path, "layout.pane.gap");
                let parsed: serde_json::Value = serde_json::from_str(&value_json).unwrap();
                assert_eq!(parsed, serde_json::json!(12.0));
            }
            other => panic!("expected UpdateSettings command, got {other:?}"),
        }
    }

    #[test]
    fn update_settings_empty_path_returns_error() {
        let result = dispatch_from_tool_call(
            "update_settings",
            serde_json::json!({"path": "", "value": 1}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn get_settings_dispatches_to_query() {
        let target = dispatch_from_tool_call("get_settings", serde_json::json!({})).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(AgentQuery::GetSettings)
        ));
    }

    #[test]
    fn list_spaces_dispatches_to_query() {
        let target = dispatch_from_tool_call("list_spaces", serde_json::json!({})).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(AgentQuery::ListSpaces)
        ));
    }

    #[test]
    fn rename_space_dispatches_to_space_command() {
        let target = dispatch_from_tool_call(
            "rename_space",
            serde_json::json!({"space_id": "work", "name": "Client A"}),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::SpaceCommand {
                command,
                space_id,
                name,
            }) => {
                assert_eq!(command, "rename");
                assert_eq!(space_id.as_deref(), Some("work"));
                assert_eq!(name.as_deref(), Some("Client A"));
            }
            other => panic!("expected SpaceCommand, got {other:?}"),
        }
    }

    #[test]
    fn create_space_dispatches_to_space_command() {
        let target =
            dispatch_from_tool_call("create_space", serde_json::json!({"name": "Work"})).unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::SpaceCommand { command, name, .. }) => {
                assert_eq!(command, "new");
                assert_eq!(name.as_deref(), Some("Work"));
            }
            other => panic!("expected SpaceCommand, got {other:?}"),
        }
    }

    #[test]
    fn delete_space_empty_id_returns_error() {
        let result = dispatch_from_tool_call("delete_space", serde_json::json!({"space_id": ""}));
        assert!(result.is_err());
    }

    #[test]
    fn open_command_tools_are_exposed() {
        let names = tool_names();
        for expected in ["in_place", "in_new_stack", "in_new_tab", "in_new_space"] {
            assert!(
                names.contains(&expected.to_string()),
                "missing OpenCommand tool: {expected}"
            );
        }
        assert!(
            !names.contains(&"in_pane".to_string()),
            "in_pane is hidden, superseded by open_page"
        );
    }

    #[test]
    fn go_back_dispatches() {
        let r = McpParamTool::BrowserGoBack { pane: None }.to_agent_command();
        assert!(matches!(r, Ok(AgentCommand::BrowserGoBack { .. })));
    }

    #[test]
    fn go_forward_dispatches() {
        let r = McpParamTool::BrowserGoForward { pane: None }.to_agent_command();
        assert!(matches!(r, Ok(AgentCommand::BrowserGoForward { .. })));
    }

    #[test]
    fn history_search_rejects_empty_query() {
        let r = McpParamTool::BrowserHistorySearch {
            query: "  ".into(),
            limit: None,
        }
        .to_agent_command();
        assert!(r.is_err());
    }

    #[test]
    fn history_search_clamps_limit() {
        let r = McpParamTool::BrowserHistorySearch {
            query: "x".into(),
            limit: Some(500),
        }
        .to_agent_command();
        match r {
            Ok(AgentCommand::BrowserHistorySearch { limit, .. }) => assert_eq!(limit, 100),
            _ => panic!(),
        }
    }

    #[test]
    fn history_search_default_limit() {
        let r = McpParamTool::BrowserHistorySearch {
            query: "x".into(),
            limit: None,
        }
        .to_agent_command();
        match r {
            Ok(AgentCommand::BrowserHistorySearch { limit, .. }) => assert_eq!(limit, 20),
            _ => panic!(),
        }
    }
}
