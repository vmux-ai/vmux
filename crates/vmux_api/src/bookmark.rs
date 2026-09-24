use crate::PageMetadata;

enum Events {}

impl crate::BinEventFamily for Events {
    const TARGET: crate::BinEventTarget = crate::BinEventTarget::Host("layout");
}

#[vmux_api::contract(Eq, Default)]
pub struct BookmarkRow {
    pub uuid: String,
    pub metadata: PageMetadata,
    pub bookmarked: bool,
    pub pinned: bool,
}

#[vmux_api::contract(Eq)]
pub struct BookmarkFolderRow {
    pub uuid: String,
    pub name: String,
    pub collapsed: bool,
    pub parent: Option<String>,
    pub children: Vec<BookmarkRow>,
}

#[vmux_api::contract(Eq)]
pub enum BookmarkNode {
    Entry(BookmarkRow),
    Folder(BookmarkFolderRow),
}

#[vmux_api::contract(Eq, Default)]
pub struct BookmarkStateEvent {
    pub pins: Vec<BookmarkRow>,
    pub roots: Vec<BookmarkNode>,
    #[serde(default)]
    pub folders: Vec<BookmarkFolderChoice>,
}

#[vmux_api::contract(Eq)]
pub struct BookmarkFolderChoice {
    pub uuid: String,
    pub label: String,
    pub ancestors: Vec<String>,
}

#[vmux_api::ui_event(Copy, Eq, Default)]
pub struct BookmarkToggleRequest;

#[vmux_api::ui_event(Eq)]
pub struct BookmarkMenuRootRequest;

#[vmux_api::ui_event(Eq)]
pub struct BookmarkMenuPinRequest {
    pub uuid: String,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkMenuEntryRequest {
    pub uuid: String,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkMenuFolderRequest {
    pub uuid: String,
    pub active_page: Option<PageMetadata>,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkOpenRequest {
    pub url: String,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkAddRequest {
    pub metadata: PageMetadata,
    pub folder: Option<String>,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkPinUrlRequest {
    pub metadata: PageMetadata,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkRemoveRequest {
    pub uuid: String,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkRenameRequest {
    pub uuid: String,
    pub name: String,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkMoveRequest {
    pub uuid: String,
    pub folder: Option<String>,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkMovePinRequest {
    pub uuid: String,
    pub folder: Option<String>,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkReorderPinRequest {
    pub uuid: String,
    pub target_uuid: String,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkPinRequest {
    pub uuid: String,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkUnpinRequest {
    pub uuid: String,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkFolderToggleRequest {
    pub uuid: String,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkFolderCreateRequest {
    pub name: String,
    pub parent: Option<String>,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkFolderMoveRequest {
    pub uuid: String,
    pub parent: Option<String>,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkFolderRenameRequest {
    pub uuid: String,
    pub name: String,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkFolderRemoveRequest {
    pub uuid: String,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkTextInputRequest {
    pub active: bool,
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkContextMenuRequest {
    pub active: bool,
}

#[vmux_api::contract(Eq, Default)]
pub struct BookmarkMenuActionEvent {
    pub sequence: u64,
    pub action: String,
    pub uuid: Option<String>,
}
