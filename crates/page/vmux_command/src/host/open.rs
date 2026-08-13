use vmux_macro::{CommandBar, DefaultShortcuts, McpTool, OsSubMenu};

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, PartialEq, Eq)]
pub enum OpenCommand {
    #[menu(id = "open_in_place", label = "Open Here")]
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
    // Hidden from the agent MCP surface: superseded by the self-relative
    // `open_page` tool (in_pane targets the focused pane, which is unpredictable
    // for an agent). Still available via the command bar / keyboard.
    #[mcp(skip)]
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
        description = "Open a page in a brand-new Tab within the current Space. Tabs are the workspace-tab strip (one level above panes); creating one gives the user a fresh layout container."
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
        description = "Open a page in a brand-new Space (top-level profile). Spaces are the highest-level container and each carries its own profile (cookies, identity, theme). Use only when the user explicitly asks for a new profile, a separate identity, or a top-level workspace switch."
    )]
    InNewSpace {
        #[mcp(
            description = "Absolute URL to open in the new Space. If omitted, opens the startup URL."
        )]
        url: Option<String>,
    },
}

impl OpenCommand {
    /// The URL this command carries. Every variant has one, and it is optional in all of them.
    pub fn url(&self) -> Option<&str> {
        match self {
            OpenCommand::InPlace { url }
            | OpenCommand::InNewStack { url }
            | OpenCommand::InNewTab { url }
            | OpenCommand::InNewSpace { url }
            | OpenCommand::InPane { url, .. } => url.as_deref(),
        }
    }
}

/// The URL an open command opens.
///
/// The command's own URL wins. When it has none — or an empty one, which the command bar and the
/// MCP surface both produce for "just open something" — the configured startup URL stands in, and
/// when neither is set there is nothing to open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenUrl(String);

impl OpenUrl {
    /// The URL `command` opens, given the configured startup URL.
    pub fn of(command: &OpenCommand, startup_url: Option<&str>) -> Self {
        Self::resolve(command.url(), startup_url)
    }

    /// The URL to open, choosing between a command's own URL and the configured startup URL.
    pub fn resolve(cmd_url: Option<&str>, startup_url: Option<&str>) -> Self {
        for candidate in [cmd_url, startup_url] {
            let Some(url) = candidate else {
                continue;
            };
            if !url.is_empty() {
                return Self(url.to_string());
            }
        }
        Self::default()
    }

    /// Whether there is nothing to open.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

pub use crate::open_target::*;

/// [`OpenUrl::resolve`] as a bare `String`, for callers that predate [`OpenUrl`].
pub fn resolve_url(cmd_url: Option<&str>, startup_url: Option<&str>) -> String {
    OpenUrl::resolve(cmd_url, startup_url).into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_url_prefers_the_commands_own_url() {
        let resolved = OpenUrl::resolve(Some("https://explicit"), Some("https://startup"));
        assert_eq!(resolved.as_str(), "https://explicit");
    }

    #[test]
    fn open_url_falls_back_to_startup_when_none() {
        let resolved = OpenUrl::resolve(None, Some("https://startup"));
        assert_eq!(resolved.as_str(), "https://startup");
    }

    #[test]
    fn open_url_treats_an_empty_url_as_absent() {
        let resolved = OpenUrl::resolve(Some(""), Some("https://startup"));
        assert_eq!(resolved.as_str(), "https://startup");
    }

    #[test]
    fn open_url_is_empty_when_neither_is_provided() {
        let resolved = OpenUrl::resolve(None, None);
        assert!(resolved.is_empty());
        assert_eq!(resolved.as_str(), "");
    }

    #[test]
    fn every_open_command_variant_carries_its_url() {
        let commands = [
            OpenCommand::InPlace {
                url: Some("https://in-place".to_string()),
            },
            OpenCommand::InNewStack {
                url: Some("https://in-new-stack".to_string()),
            },
            OpenCommand::InNewTab {
                url: Some("https://in-new-tab".to_string()),
            },
            OpenCommand::InNewSpace {
                url: Some("https://in-new-space".to_string()),
            },
            OpenCommand::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: Some("https://in-pane".to_string()),
            },
        ];
        let urls: Vec<Option<&str>> = commands.iter().map(OpenCommand::url).collect();
        assert_eq!(
            urls,
            [
                Some("https://in-place"),
                Some("https://in-new-stack"),
                Some("https://in-new-tab"),
                Some("https://in-new-space"),
                Some("https://in-pane"),
            ]
        );
        for command in &commands {
            assert_eq!(
                OpenUrl::of(command, Some("https://startup")).as_str(),
                command.url().expect("every variant was given a url")
            );
        }
    }

    #[test]
    fn a_command_without_a_url_opens_the_startup_url() {
        let command = OpenCommand::InNewTab { url: None };
        assert_eq!(
            OpenUrl::of(&command, Some("https://startup")).as_str(),
            "https://startup"
        );
    }
}
