#[vmux_api::contract(Copy, Eq, PartialOrd, Ord, Hash)]
pub enum ToolProvider {
    HomebrewFormula,
    HomebrewCask,
    Npm,
    Acp,
    Lsp,
    Dotfiles,
    Mcp,
}

impl ToolProvider {
    pub const ALL: [Self; 7] = [
        Self::HomebrewFormula,
        Self::HomebrewCask,
        Self::Npm,
        Self::Acp,
        Self::Lsp,
        Self::Mcp,
        Self::Dotfiles,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::HomebrewFormula => "homebrew-formula",
            Self::HomebrewCask => "homebrew-cask",
            Self::Npm => "npm",
            Self::Acp => "acp",
            Self::Lsp => "lsp",
            Self::Dotfiles => "dotfiles",
            Self::Mcp => "mcp",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::HomebrewFormula => "Homebrew Formulae",
            Self::HomebrewCask => "Homebrew Casks",
            Self::Npm => "NPM Globals",
            Self::Acp => "Agents",
            Self::Lsp => "LSP Servers",
            Self::Dotfiles => "Dotfiles",
            Self::Mcp => "MCP Servers",
        }
    }
}

#[vmux_api::contract(Copy, Eq)]
pub enum ToolStatus {
    Available,
    Installed,
    Outdated,
    Missing,
    Conflict,
    Failed,
}

#[vmux_api::contract(Copy, Eq)]
pub enum ToolAction {
    Install,
    Update,
    Uninstall,
    Forget,
    Adopt,
    Link,
    Unlink,
    Apply,
    Import,
}

#[vmux_api::contract(Eq)]
pub struct ToolItem {
    pub provider: ToolProvider,
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub version: Option<String>,
    pub detail: String,
    pub status: ToolStatus,
    pub managed: bool,
    pub actions: Vec<ToolAction>,
}

#[vmux_api::contract(Eq)]
pub struct ToolCategory {
    pub provider: ToolProvider,
    pub items: Vec<ToolItem>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ToolsSnapshot {
    pub loaded: bool,
    pub root: String,
    pub vault: crate::vault::VaultSnapshot,
    pub categories: Vec<ToolCategory>,
    pub installed: u32,
    pub updates: u32,
    pub conflicts: u32,
    pub error: String,
}

#[vmux_api::contract(Eq)]
pub enum ToolUiOperationState {
    Pending,
    Completed { success: bool, message: String },
}

#[vmux_api::contract(Eq)]
pub struct ToolUiOperation {
    pub operation_id: u64,
    pub provider: ToolProvider,
    pub action: ToolAction,
    pub item_id: String,
    pub state: ToolUiOperationState,
}

impl ToolUiOperation {
    pub fn pending(
        operation_id: u64,
        provider: ToolProvider,
        action: ToolAction,
        item_id: String,
    ) -> Self {
        Self {
            operation_id,
            provider,
            action,
            item_id,
            state: ToolUiOperationState::Pending,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.state == ToolUiOperationState::Pending
    }

    pub fn completion(&self) -> Option<(bool, &str)> {
        let ToolUiOperationState::Completed { success, message } = &self.state else {
            return None;
        };
        Some((*success, message))
    }
}

#[vmux_api::ui_state(Default, Eq, version = 3, target = "tools")]
pub struct ToolsUiState {
    pub snapshot: ToolsSnapshot,
    pub operations: Vec<ToolUiOperation>,
}

#[vmux_api::ui_event(Default, Eq, target = "tools")]
pub struct ToolsRefreshRequest {
    pub refresh: bool,
}

#[vmux_api::ui_event(Eq, target = "tools")]
pub struct ToolOpenRequest {
    pub path: String,
}

#[vmux_api::ui_event(Eq, target = "tools")]
pub struct ToolsNavigateRequest {
    pub url: String,
}

impl ToolsNavigateRequest {
    pub fn canonical_url(&self) -> Option<&'static str> {
        match self.url.trim().trim_end_matches('/') {
            "vmux://tools/acp" => Some("vmux://tools/acp"),
            "vmux://tools/lsp" => Some("vmux://tools/lsp"),
            "vmux://tools/homebrew" => Some("vmux://tools/homebrew"),
            "vmux://tools/npm" => Some("vmux://tools/npm"),
            "vmux://tools/mcp" => Some("vmux://tools/mcp"),
            "vmux://tools/dotfiles" => Some("vmux://tools/dotfiles"),
            "vmux://tools/extensions" => Some("vmux://tools/extensions"),
            _ => None,
        }
    }
}

#[vmux_api::ui_event(Eq, target = "tools")]
pub struct ToolRequest {
    pub provider: ToolProvider,
    pub action: ToolAction,
    pub id: String,
    #[serde(default)]
    pub value: String,
}
