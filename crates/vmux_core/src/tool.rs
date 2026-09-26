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

#[vmux_api::contract(Copy, Eq, PartialOrd, Ord, Hash)]
pub enum ToolOperationKind {
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
    pub operations: Vec<ToolOperationKind>,
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
    pub categories: Vec<ToolCategory>,
    pub installed: u32,
    pub updates: u32,
    pub conflicts: u32,
    pub error: String,
}

#[vmux_api::contract(Eq, PartialOrd, Ord, Hash)]
pub struct ToolOperationKey {
    pub provider: ToolProvider,
    pub kind: ToolOperationKind,
    pub item_id: String,
}

impl ToolOperationKey {
    pub fn new(
        provider: ToolProvider,
        kind: ToolOperationKind,
        item_id: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            kind,
            item_id: item_id.into(),
        }
    }
}

#[vmux_api::contract(Eq)]
pub struct ToolOperationNotice {
    pub operation: ToolOperationKey,
    pub success: bool,
    pub message: String,
}

#[vmux_api::ui_state(Default, Eq, version = 6, target = "tools")]
pub struct ToolsUiState {
    pub snapshot: ToolsSnapshot,
    pub pending: Vec<ToolOperationKey>,
    pub notice: Option<ToolOperationNotice>,
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
pub struct ToolInstallRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(Eq, target = "tools")]
pub struct ToolUpdateRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(Eq, target = "tools")]
pub struct ToolUninstallRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(Eq, target = "tools")]
pub struct ToolForgetRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(Eq, target = "tools")]
pub struct ToolAdoptRequest {
    pub provider: ToolProvider,
    pub id: String,
    pub value: String,
}

#[vmux_api::ui_event(Eq, target = "tools")]
pub struct ToolLinkRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(Eq, target = "tools")]
pub struct ToolUnlinkRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(Default, Eq, target = "tools")]
pub struct ToolApplyRequest;

#[vmux_api::ui_event(Eq, target = "tools")]
pub struct ToolImportRequest {
    pub provider: ToolProvider,
    pub value: String,
}

impl From<ToolInstallRequest> for ToolOperationKey {
    fn from(request: ToolInstallRequest) -> Self {
        Self::new(request.provider, ToolOperationKind::Install, request.id)
    }
}

impl From<ToolUpdateRequest> for ToolOperationKey {
    fn from(request: ToolUpdateRequest) -> Self {
        Self::new(request.provider, ToolOperationKind::Update, request.id)
    }
}

impl From<ToolUninstallRequest> for ToolOperationKey {
    fn from(request: ToolUninstallRequest) -> Self {
        Self::new(request.provider, ToolOperationKind::Uninstall, request.id)
    }
}

impl From<ToolForgetRequest> for ToolOperationKey {
    fn from(request: ToolForgetRequest) -> Self {
        Self::new(request.provider, ToolOperationKind::Forget, request.id)
    }
}

impl From<ToolAdoptRequest> for ToolOperationKey {
    fn from(request: ToolAdoptRequest) -> Self {
        Self::new(request.provider, ToolOperationKind::Adopt, request.id)
    }
}

impl From<ToolLinkRequest> for ToolOperationKey {
    fn from(request: ToolLinkRequest) -> Self {
        Self::new(request.provider, ToolOperationKind::Link, request.id)
    }
}

impl From<ToolUnlinkRequest> for ToolOperationKey {
    fn from(request: ToolUnlinkRequest) -> Self {
        Self::new(request.provider, ToolOperationKind::Unlink, request.id)
    }
}

impl From<ToolApplyRequest> for ToolOperationKey {
    fn from(_: ToolApplyRequest) -> Self {
        Self::new(ToolProvider::Dotfiles, ToolOperationKind::Apply, "")
    }
}

impl From<ToolImportRequest> for ToolOperationKey {
    fn from(request: ToolImportRequest) -> Self {
        Self::new(request.provider, ToolOperationKind::Import, "")
    }
}
