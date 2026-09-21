use vmux_client::protocol::{AgentCommand, AgentSpaceCommand};
use vmux_macro::McpTool;

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
            McpParamTool::CreateSpace { name } => {
                Ok(AgentCommand::SpaceCommand(AgentSpaceCommand::Create {
                    name: name.filter(|name| !name.trim().is_empty()),
                }))
            }
            McpParamTool::RenameSpace { space_id, name } => {
                if space_id.trim().is_empty() {
                    return Err("rename_space.space_id is empty".into());
                }
                if name.trim().is_empty() {
                    return Err("rename_space.name is empty".into());
                }
                Ok(AgentCommand::SpaceCommand(AgentSpaceCommand::Rename {
                    space_id,
                    name,
                }))
            }
            McpParamTool::DeleteSpace { space_id } => {
                if space_id.trim().is_empty() {
                    return Err("delete_space.space_id is empty".into());
                }
                Ok(AgentCommand::SpaceCommand(AgentSpaceCommand::Delete {
                    space_id,
                }))
            }
            McpParamTool::Notify { title, body } => Ok(AgentCommand::Notify { title, body }),
        }
    }
}
