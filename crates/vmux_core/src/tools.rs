pub const TOOLS_SNAPSHOT_EVENT: &str = "tools-snapshot";
pub const TOOL_ACTION_RESULT_EVENT: &str = "tool-action-result";

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum ToolStatus {
    Available,
    Installed,
    Outdated,
    Missing,
    Conflict,
    Failed,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ToolItem {
    pub provider: ToolProvider,
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub detail: String,
    pub status: ToolStatus,
    pub managed: bool,
    pub actions: Vec<ToolAction>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ToolCategory {
    pub provider: ToolProvider,
    pub items: Vec<ToolItem>,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ToolsSnapshot {
    pub root: String,
    pub vault: crate::vault::VaultSnapshot,
    pub categories: Vec<ToolCategory>,
    pub installed: u32,
    pub updates: u32,
    pub conflicts: u32,
    pub error: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ToolsRefreshRequest {
    pub refresh: bool,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ToolOpenRequest {
    pub path: String,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ToolActionRequest {
    pub provider: ToolProvider,
    pub action: ToolAction,
    pub id: String,
    #[serde(default)]
    pub value: String,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ToolActionResult {
    pub provider: ToolProvider,
    pub action: ToolAction,
    pub id: String,
    pub success: bool,
    pub message: String,
}
