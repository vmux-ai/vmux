//! Wire model for local tools and dotfiles.

/// Host-to-page event carrying a complete tools snapshot.
pub const TOOLS_SNAPSHOT_EVENT: &str = "tools-snapshot";
/// Host-to-page event carrying one completed mutation.
pub const TOOL_ACTION_RESULT_EVENT: &str = "tool-action-result";

/// Source that owns an installed tool item.
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
    /// Providers rendered by the tools manager, in display order.
    pub const ALL: [Self; 7] = [
        Self::HomebrewFormula,
        Self::HomebrewCask,
        Self::Npm,
        Self::Acp,
        Self::Lsp,
        Self::Mcp,
        Self::Dotfiles,
    ];

    /// Stable manifest and wire identifier.
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

    /// User-facing category title.
    pub const fn title(self) -> &'static str {
        match self {
            Self::HomebrewFormula => "Homebrew Formulae",
            Self::HomebrewCask => "Homebrew Casks",
            Self::Npm => "npm Globals",
            Self::Acp => "Agents",
            Self::Lsp => "Language Tools",
            Self::Dotfiles => "Dotfiles",
            Self::Mcp => "MCP Servers",
        }
    }
}

/// Reconciled state of one installed or declared item.
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

/// Mutation offered for a tool item.
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

/// Provider-qualified package or dotfile-package row.
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

/// Items grouped under one provider.
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

/// Complete tools state rendered by the manager and side sheet.
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
    pub categories: Vec<ToolCategory>,
    pub installed: u32,
    pub updates: u32,
    pub conflicts: u32,
    pub error: String,
}

/// Requests a cached scan or an explicit catalog/update refresh.
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

/// Requests one package, manifest, or dotfile mutation.
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

/// Completion result for a tools mutation.
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
