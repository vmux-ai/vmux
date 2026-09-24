pub const EXTENSIONS_PAGE_URL: &str = "vmux://extensions/";

#[vmux_api::contract(Copy, Eq)]
pub enum ExtStatus {
    Installing,
    Installed,
    Disabled,
    Failed,
}

#[vmux_api::contract(Copy, Eq)]
pub enum ExtInstallPhase {
    Resolving,
    Downloading,
    Unpacking,
    Done,
    Failed,
}

#[vmux_api::contract(Eq)]
pub struct ExtRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub icon: Option<String>,
    pub popup: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub pinned: bool,
    pub needs_approval: bool,
    pub required_permissions: Vec<String>,
    pub required_host_permissions: Vec<String>,
    pub status: ExtStatus,
}

#[vmux_api::ui_state(Eq, Default, version = 2, targets = ["extensions", "layout", "tools"])]
pub struct ExtensionsEvent {
    pub loaded: bool,
    pub extensions: Vec<ExtRow>,
    pub installing: Vec<ExtInstallProgress>,
    pub pending: bool,
}

#[vmux_api::contract(Eq)]
pub struct ExtInstallProgress {
    pub key: String,
    pub phase: ExtInstallPhase,
    pub pct: Option<u8>,
    pub message: String,
}

#[vmux_api::ui_event(Eq, targets = ["extensions", "tools"])]
pub struct ExtToggleRequest {
    pub id: String,
    pub enabled: bool,
    pub approve_permissions: bool,
}

#[vmux_api::ui_event(Eq, targets = ["extensions", "tools"])]
pub struct ExtUninstallRequest {
    pub id: String,
}

#[vmux_api::ui_event(Eq, targets = ["extensions", "layout", "tools"])]
pub struct ExtensionPopupOpenRequest {
    pub id: String,
    pub anchor: ExtensionPopupAnchor,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub struct ExtensionPopupAnchor {
    pub right: i32,
    pub bottom: i32,
}

#[vmux_api::contract(Default, Eq)]
pub struct ExtensionPopupEvent {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub anchor: ExtensionPopupAnchor,
}

#[vmux_api::ui_event(Copy, Default, targets = ["extensions", "layout", "tools"])]
pub struct ExtensionPopupBoundsRequest {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

#[vmux_api::contract(Default)]
pub struct ExtensionPopupSizeEvent {
    pub id: String,
    pub width: f32,
    pub height: f32,
}

#[vmux_api::ui_event(Copy, Default, Eq, targets = ["extensions", "layout", "tools"])]
pub struct ExtensionPopupCloseRequest;

#[vmux_api::ui_event(Eq, targets = ["extensions", "layout", "tools"])]
pub struct ExtPinRequest {
    pub id: String,
    pub pinned: bool,
}

#[vmux_api::ui_event(Eq, targets = ["extensions", "layout", "tools"])]
pub struct ExtOpenManagerRequest;

#[vmux_api::ui_event(Eq, targets = ["extensions", "layout", "tools"])]
pub struct ExtListRequest;

#[vmux_api::ui_event(Eq, targets = ["extensions", "tools"])]
pub struct ExtBrowseStoreRequest {
    pub query: String,
}
