use crate::PageMetadata;

enum Events {}

impl crate::BinEventFamily for Events {
    const TARGET: crate::BinEventTarget = crate::BinEventTarget::Host("layout");
}

#[vmux_api::payload(Eq, Default)]
pub struct BookmarkRow {
    pub uuid: String,
    pub metadata: PageMetadata,
    pub bookmarked: bool,
    pub pinned: bool,
}

#[vmux_api::payload(Eq)]
pub struct BookmarkFolderRow {
    pub uuid: String,
    pub name: String,
    pub collapsed: bool,
    pub parent: Option<String>,
    pub children: Vec<BookmarkRow>,
}

#[vmux_api::payload(Eq)]
pub enum BookmarkNode {
    Entry(BookmarkRow),
    Folder(BookmarkFolderRow),
}

#[vmux_api::payload(Eq, Default)]
#[derive(vmux_api::HostEvent)]
pub struct BookmarkStateEvent {
    pub pins: Vec<BookmarkRow>,
    pub roots: Vec<BookmarkNode>,
}

#[vmux_api::payload(Copy, Eq, Default)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkToggleRequest;

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkMenuRootRequest;

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkMenuPinRequest {
    pub uuid: String,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkMenuEntryRequest {
    pub uuid: String,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkMenuFolderRequest {
    pub uuid: String,
    pub active_page: Option<PageMetadata>,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkOpenRequest {
    pub url: String,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkAddRequest {
    pub metadata: PageMetadata,
    pub folder: Option<String>,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkPinUrlRequest {
    pub metadata: PageMetadata,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkRemoveRequest {
    pub uuid: String,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkRenameRequest {
    pub uuid: String,
    pub name: String,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkMoveRequest {
    pub uuid: String,
    pub folder: Option<String>,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkMovePinRequest {
    pub uuid: String,
    pub folder: Option<String>,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkReorderPinRequest {
    pub uuid: String,
    pub target_uuid: String,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkPinRequest {
    pub uuid: String,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkUnpinRequest {
    pub uuid: String,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkFolderToggleRequest {
    pub uuid: String,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkFolderCreateRequest {
    pub name: String,
    pub parent: Option<String>,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkFolderMoveRequest {
    pub uuid: String,
    pub parent: Option<String>,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkFolderRenameRequest {
    pub uuid: String,
    pub name: String,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkFolderRemoveRequest {
    pub uuid: String,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkTextInputRequest {
    pub active: bool,
}

#[vmux_api::payload(Eq)]
#[derive(vmux_api::UiEvent)]
pub struct BookmarkContextMenuRequest {
    pub active: bool,
}

#[vmux_api::payload(Eq, Default)]
#[derive(vmux_api::HostEvent)]
pub struct BookmarkMenuActionEvent {
    pub sequence: u64,
    pub action: String,
    pub uuid: Option<String>,
}
