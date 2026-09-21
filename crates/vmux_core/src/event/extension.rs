use serde::{Deserialize, Serialize};

pub const EXTENSIONS_PAGE_URL: &str = "vmux://extensions/";

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum ExtStatus {
    Installing,
    Installed,
    Disabled,
    Failed,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum ExtInstallPhase {
    Resolving,
    Downloading,
    Unpacking,
    Done,
    Failed,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "extensions", name = "list", targets = ["extensions", "layout", "tools"])]
pub struct ExtensionsEvent {
    pub extensions: Vec<ExtRow>,
    pub pending: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "ext", name = "install_progress", targets = ["extensions", "layout", "tools"])]
pub struct ExtInstallProgress {
    pub key: String,
    pub phase: ExtInstallPhase,
    pub pct: Option<u8>,
    pub message: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "ext", name = "status", targets = ["extensions", "layout", "tools"])]
pub struct ExtStatusEvent {
    pub id: String,
    pub status: ExtStatus,
    pub version: Option<String>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "ext", name = "toggle_request", targets = ["extensions", "tools"])]
pub struct ExtToggleRequest {
    pub id: String,
    pub enabled: bool,
    pub approve_permissions: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "ext", name = "uninstall_request", targets = ["extensions", "tools"])]
pub struct ExtUninstallRequest {
    pub id: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "ext", name = "action_request", targets = ["extensions", "layout", "tools"])]
pub struct ExtActionRequest {
    pub id: String,
    pub anchor: ExtensionPopupAnchor,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ExtensionPopupAnchor {
    pub right: i32,
    pub bottom: i32,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "extension", name = "popup", target = "layout")]
pub struct ExtensionPopupEvent {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub anchor: ExtensionPopupAnchor,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "extension", name = "popup_bounds_request", targets = ["extensions", "layout", "tools"])]
pub struct ExtensionPopupBoundsRequest {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "extension", name = "popup_size", target = "layout")]
pub struct ExtensionPopupSizeEvent {
    pub id: String,
    pub width: f32,
    pub height: f32,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "extension", name = "popup_close_request", targets = ["extensions", "layout", "tools"])]
pub struct ExtensionPopupCloseRequest;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "ext", name = "pin_request", targets = ["extensions", "layout", "tools"])]
pub struct ExtPinRequest {
    pub id: String,
    pub pinned: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "ext", name = "open_manager_request", targets = ["extensions", "layout", "tools"])]
pub struct ExtOpenManagerRequest;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "ext", name = "list_request", targets = ["extensions", "layout", "tools"])]
pub struct ExtListRequest;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "ext", name = "browse_store_request", targets = ["extensions", "tools"])]
pub struct ExtBrowseStoreRequest {
    pub query: String,
}
