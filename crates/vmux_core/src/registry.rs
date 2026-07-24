//! Wire model for the local tool and dotfile Registry.

/// Host-to-page event carrying a complete Registry snapshot.
pub const REGISTRY_SNAPSHOT_EVENT: &str = "registry-snapshot";
/// Host-to-page event carrying one completed mutation.
pub const REGISTRY_ACTION_RESULT_EVENT: &str = "registry-action-result";

/// Source that owns an installed Registry item.
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
pub enum RegistryProvider {
    HomebrewFormula,
    HomebrewCask,
    Npm,
    Acp,
    Lsp,
    Dotfiles,
    Mcp,
}

impl RegistryProvider {
    /// Providers rendered by the Registry, in display order.
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
pub enum RegistryStatus {
    Available,
    Installed,
    Outdated,
    Missing,
    Conflict,
    Failed,
}

/// Mutation offered for a Registry item.
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
pub enum RegistryAction {
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
pub struct RegistryItem {
    pub provider: RegistryProvider,
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub detail: String,
    pub status: RegistryStatus,
    pub managed: bool,
    pub actions: Vec<RegistryAction>,
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
pub struct RegistryCategory {
    pub provider: RegistryProvider,
    pub items: Vec<RegistryItem>,
}

/// Complete Registry state rendered by the manager and side sheet.
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
pub struct RegistrySnapshot {
    pub root: String,
    pub categories: Vec<RegistryCategory>,
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
pub struct RegistryRefreshRequest {
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
pub struct RegistryActionRequest {
    pub provider: RegistryProvider,
    pub action: RegistryAction,
    pub id: String,
    #[serde(default)]
    pub value: String,
}

/// Completion result for a Registry mutation.
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
pub struct RegistryActionResult {
    pub provider: RegistryProvider,
    pub action: RegistryAction,
    pub id: String,
    pub success: bool,
    pub message: String,
}
