#[vmux_api::contract(Default, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct ToolProvider(pub String);

impl ToolProvider {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn id(&self) -> &str {
        &self.0
    }

    pub fn is(&self, id: &str) -> bool {
        self.0 == id
    }
}

#[vmux_api::contract(Eq)]
pub struct ToolProviderMetadata {
    pub provider: ToolProvider,
    pub title: String,
    pub title_message_id: String,
    pub route_title: String,
    pub route_title_message_id: String,
    pub short_label: String,
    pub route: String,
    pub rank: i32,
    pub thumbnails: bool,
    pub apply: bool,
    pub brewfile: bool,
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
    pub providers: Vec<ToolProviderMetadata>,
    pub installed: u32,
    pub updates: u32,
    pub conflicts: u32,
    pub error: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct ToolsView {
    pub route: String,
    pub query: String,
    pub route_title: String,
    pub route_title_message_id: String,
    pub categories: Vec<ToolCategory>,
    pub visible_count: u32,
    pub apply_provider: Option<ToolProvider>,
    pub show_brewfile: bool,
}

impl ToolsSnapshot {
    pub fn provider(&self, provider: &ToolProvider) -> Option<&ToolProviderMetadata> {
        self.providers
            .iter()
            .find(|metadata| &metadata.provider == provider)
    }
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

#[vmux_api::ui_state(Default, Eq, version = 9)]
pub struct ToolsUiState {
    pub snapshot: ToolsSnapshot,
    pub view: ToolsView,
    pub pending: Vec<ToolOperationKey>,
    pub notice: Option<ToolOperationNotice>,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct ToolsRefreshRequest {
    pub refresh: bool,
}

#[vmux_api::ui_event(Eq)]
pub struct ToolOpenRequest {
    pub path: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ToolsNavigateRequest {
    pub url: String,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct ToolsFilterRequest {
    pub query: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ToolInstallRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ToolUpdateRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ToolUninstallRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ToolForgetRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ToolAdoptRequest {
    pub provider: ToolProvider,
    pub id: String,
    pub value: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ToolLinkRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(Eq)]
pub struct ToolUnlinkRequest {
    pub provider: ToolProvider,
    pub id: String,
}

#[vmux_api::ui_event(version = 2)]
pub struct ToolApplyRequest {
    pub provider: ToolProvider,
}

#[vmux_api::ui_event(Eq)]
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
    fn from(request: ToolApplyRequest) -> Self {
        Self::new(request.provider, ToolOperationKind::Apply, "")
    }
}

impl From<ToolImportRequest> for ToolOperationKey {
    fn from(request: ToolImportRequest) -> Self {
        Self::new(request.provider, ToolOperationKind::Import, "")
    }
}
