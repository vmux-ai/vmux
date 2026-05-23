pub mod handler;

use vmux_macro::{CommandBar, DefaultShortcuts, McpTool, OsSubMenu};

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, PartialEq, Eq)]
pub enum OpenCommand {
    #[menu(id = "open_in_place", label = "Open Here", accel = "super+l")]
    #[mcp(
        description = "Navigate the currently focused stack to the given URL. Equivalent to the user typing a URL in the address bar. Use when the user asks to 'go to', 'navigate to', or 'open' a URL without specifying placement; the current page is replaced. If url is omitted, opens the configured startup URL."
    )]
    InPlace {
        #[mcp(description = "Absolute URL to open. If omitted, opens the startup URL.")]
        url: Option<String>,
    },

    #[menu(
        id = "open_in_new_stack",
        label = "Open in New Stack",
        accel = "super+n"
    )]
    #[mcp(
        description = "Open the URL as a new stack inside the currently focused pane. Stacks are the in-pane tab strip: the current stack stays alive and a new one is added next to it, becoming active. Use when the user wants to preserve the current page and view a new one alongside, in the same pane."
    )]
    InNewStack {
        #[mcp(
            description = "Absolute URL to open in the new stack. If omitted, opens the startup URL."
        )]
        url: Option<String>,
    },

    #[menu(
        expand = "direction",
        id_template = "open_in_pane_{dir}",
        label_template = "Open in Pane {Dir}"
    )]
    #[shortcut(
        expand = "direction",
        top = "Super+Shift+K",
        right = "Super+Shift+L",
        bottom = "Super+Shift+J",
        left = "Super+Shift+H"
    )]
    #[shortcut(
        chord = "Ctrl+g, %",
        variant = "InPane { direction: PaneDirection::Right, target: PaneTarget::NewSplit, mode: PaneOpenMode::NewStack, url: None }"
    )]
    #[shortcut(
        chord = "Ctrl+g, \"",
        variant = "InPane { direction: PaneDirection::Bottom, target: PaneTarget::NewSplit, mode: PaneOpenMode::NewStack, url: None }"
    )]
    #[mcp(
        description = "Open URL in a sibling pane in the given direction. Set target=NewSplit to split the current pane, target=Existing to reuse an adjacent pane (falls back to NewSplit if none). Set mode=InPlace to navigate the chosen pane's active stack, mode=NewStack to add a stack to it."
    )]
    InPane {
        #[mcp(description = "Which side of the current pane to act on.", enum_values = ["top", "right", "bottom", "left"])]
        direction: PaneDirection,
        #[mcp(description = "Existing reuses the sibling pane in `direction` (falls back to NewSplit if none). NewSplit always splits the current pane.", enum_values = ["existing", "new_split"])]
        target: PaneTarget,
        #[mcp(description = "InPlace navigates the chosen pane's active stack. NewStack appends a new stack within that pane.", enum_values = ["in_place", "new_stack"])]
        mode: PaneOpenMode,
        #[mcp(description = "Absolute URL to open. If omitted, opens the startup URL.")]
        url: Option<String>,
    },

    #[menu(id = "open_in_new_tab", label = "Open in New Tab", accel = "super+t")]
    #[mcp(
        description = "Open URL in a brand-new Tab within the current Space. Tabs are the workspace-tab strip (one level above panes); creating one gives the user a fresh layout container."
    )]
    InNewTab {
        #[mcp(
            description = "Absolute URL to open in the new Tab. If omitted, opens the startup URL."
        )]
        url: Option<String>,
    },

    #[menu(
        id = "open_in_new_space",
        label = "Open in New Space",
        accel = "super+shift+n"
    )]
    #[mcp(
        description = "Open URL in a brand-new Space (top-level profile). Spaces are the highest-level container and each carries its own profile (cookies, identity, theme). Use only when the user explicitly asks for a new profile, a separate identity, or a top-level workspace switch."
    )]
    InNewSpace {
        #[mcp(
            description = "Absolute URL to open in the new Space. If omitted, opens the startup URL."
        )]
        url: Option<String>,
    },
}

pub use crate::open_target::*;
