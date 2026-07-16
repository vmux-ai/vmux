//! MCP schema for commands handled by the desktop application.

#![allow(dead_code)]

use vmux_macro::McpTool;

#[derive(Debug, McpTool)]
enum AppCommand {
    Scene(SceneCommand),
    Terminal(TerminalCommand),
    Browser(BrowserCommand),
    Service(ServiceCommand),
}

#[derive(Debug, McpTool)]
enum SceneCommand {
    InteractiveMode(SceneInteractiveModeCommand),
}

#[derive(Debug, McpTool)]
enum SceneInteractiveModeCommand {
    #[menu(id = "interactive_mode_user", label = "User")]
    User,
    #[menu(id = "interactive_mode_player", label = "Player")]
    Player,
    #[menu(id = "toggle_player_mode", label = "Toggle Player Mode")]
    #[mcp(skip)]
    Toggle,
}

#[derive(Debug, McpTool)]
enum TerminalCommand {
    #[menu(id = "terminal_close", label = "Close Terminal")]
    Close,
    #[menu(id = "terminal_next", label = "Next Terminal")]
    Next,
    #[menu(id = "terminal_prev", label = "Previous Terminal")]
    Previous,
    #[menu(id = "terminal_clear", label = "Clear Terminal")]
    Clear,
    #[menu(id = "terminal_copy_mode", label = "Visual Mode")]
    CopyMode,
}

#[derive(Debug, McpTool)]
enum BrowserCommand {
    Navigation(BrowserNavigationCommand),
    Open(OpenCommand),
    View(BrowserViewCommand),
    Bar(BrowserBarCommand),
}

#[derive(Debug, McpTool)]
enum BrowserNavigationCommand {
    #[menu(id = "browser_prev_page", label = "Back")]
    PrevPage,
    #[menu(id = "browser_next_page", label = "Forward")]
    NextPage,
    #[menu(id = "browser_reload", label = "Reload")]
    Reload,
    #[menu(id = "browser_hard_reload", label = "Hard Reload")]
    HardReload,
    #[menu(id = "browser_stop", label = "Stop Loading")]
    Stop,
}

#[derive(Debug, McpTool)]
#[allow(clippy::enum_variant_names)]
enum OpenCommand {
    #[menu(id = "open_in_place", label = "Open Here")]
    #[mcp(
        description = "Navigate the currently focused stack to the given URL. Equivalent to the user typing a URL in the address bar. Use when the user asks to 'go to', 'navigate to', or 'open' a URL without specifying placement; the current page is replaced. If url is omitted, opens the configured startup URL."
    )]
    InPlace {
        #[mcp(description = "Absolute URL to open. If omitted, opens the startup URL.")]
        url: Option<String>,
    },
    #[menu(id = "open_in_new_stack", label = "Open in New Stack")]
    #[mcp(
        description = "Open the URL as a new stack inside the currently focused pane. Stacks are the in-pane tab strip: the current stack stays alive and a new one is added next to it, becoming active. Use when the user wants to preserve the current page and view a new one alongside, in the same pane."
    )]
    InNewStack {
        #[mcp(
            description = "Absolute URL to open in the new stack. If omitted, opens the startup URL."
        )]
        url: Option<String>,
    },
    #[menu(id = "open_in_pane", label = "Open in Pane")]
    #[mcp(skip)]
    InPane {
        direction: PaneDirection,
        target: PaneTarget,
        mode: PaneOpenMode,
        url: Option<String>,
    },
    #[menu(id = "open_in_new_tab", label = "Open in New Tab")]
    #[mcp(
        description = "Open a page in a brand-new Tab within the current Space. Tabs are the workspace-tab strip (one level above panes); creating one gives the user a fresh layout container."
    )]
    InNewTab {
        #[mcp(
            description = "Absolute URL to open in the new Tab. If omitted, opens the startup URL."
        )]
        url: Option<String>,
    },
    #[menu(id = "open_in_new_space", label = "Open in New Space")]
    #[mcp(
        description = "Open a page in a brand-new Space (top-level profile). Spaces are the highest-level container and each carries its own profile (cookies, identity, theme). Use only when the user explicitly asks for a new profile, a separate identity, or a top-level workspace switch."
    )]
    InNewSpace {
        #[mcp(
            description = "Absolute URL to open in the new Space. If omitted, opens the startup URL."
        )]
        url: Option<String>,
    },
}

#[derive(Debug)]
enum PaneDirection {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Debug)]
enum PaneTarget {
    Existing,
    NewSplit,
}

#[derive(Debug)]
enum PaneOpenMode {
    InPlace,
    NewStack,
}

#[derive(Debug, McpTool)]
enum BrowserViewCommand {
    #[menu(id = "browser_zoom_in", label = "Zoom In")]
    ZoomIn,
    #[menu(id = "browser_zoom_out", label = "Zoom Out")]
    ZoomOut,
    #[menu(id = "browser_zoom_reset", label = "Actual Size")]
    ZoomReset,
    #[menu(id = "browser_dev_tools", label = "Developer Tools")]
    DevTools,
    #[menu(id = "browser_view_source", label = "View Source")]
    ViewSource,
    #[menu(id = "browser_print", label = "Print")]
    Print,
}

#[derive(Debug, McpTool)]
enum BrowserBarCommand {
    #[menu(id = "browser_open_command_bar", label = "Command Bar")]
    OpenCommandBar,
    #[menu(id = "browser_open_page_in_command_bar", label = "Edit Page")]
    OpenPageInCommandBar,
    #[menu(id = "browser_open_path_bar", label = "Path Navigator")]
    OpenPathBar,
    #[menu(id = "browser_open_commands", label = "Commands")]
    OpenCommands,
    #[menu(id = "browser_open_history", label = "History")]
    OpenHistory,
    #[menu(id = "browser_find", label = "Find")]
    Find,
}

#[derive(Debug, McpTool)]
enum ServiceCommand {
    #[menu(id = "service_open", label = "Open Service Monitor")]
    Open,
}

pub fn tool_entries() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    AppCommand::mcp_tool_entries()
}

pub fn accepts_id(id: &str) -> bool {
    AppCommand::from_mcp_id(id).is_some()
}

pub fn accepts_call(id: &str, arguments: serde_json::Value) -> bool {
    AppCommand::from_mcp_call(id, arguments).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_dispatches() {
        for (id, _, schema) in tool_entries() {
            let has_required_arguments = schema
                .get("required")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|required| !required.is_empty());
            assert!(
                accepts_id(id)
                    || !has_required_arguments && accepts_call(id, serde_json::json!({})),
                "{id}"
            );
        }
    }

    #[test]
    fn schema_matches_desktop_commands() {
        assert_eq!(tool_entries(), vmux_command::AppCommand::mcp_tool_entries());
    }
}
