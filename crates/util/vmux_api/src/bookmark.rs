use crate::PageMetadata;

#[cfg(bevy_linked)]
use bevy_ecs::component::Component;
#[cfg(bevy_linked)]
use bevy_ecs::reflect::ReflectComponent;
#[cfg(bevy_linked)]
use bevy_reflect::Reflect;

#[vmux_api::contract(Copy, Eq)]
#[cfg_attr(bevy_linked, derive(Component, Reflect))]
#[cfg_attr(bevy_linked, reflect(Component))]
#[cfg_attr(bevy_linked, type_path = "vmux_api")]
#[serde(rename_all = "snake_case")]
pub enum SmartBookmarkFolder {
    Projects,
    Knowledge,
    Tools,
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

#[vmux_api::ui_event]
pub struct BookmarkToggleRequest;

#[vmux_api::ui_event]
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

#[vmux_api::contract(Eq)]
pub enum BookmarkDropSource {
    Page { metadata: PageMetadata },
    Bookmark { uuid: String },
    Pin { uuid: String },
    Folder { uuid: String },
}

#[vmux_api::contract(Eq)]
pub enum BookmarkDropTarget {
    Root,
    Folder { uuid: String },
    Pin { uuid: String },
}

#[vmux_api::ui_event(Eq)]
pub struct BookmarkDropRequest {
    pub source: BookmarkDropSource,
    pub target: BookmarkDropTarget,
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

#[vmux_api::ui_event(Default, Eq)]
pub struct BookmarkFolderEditOpenRequest {
    pub parent: Option<String>,
    pub draft: String,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct BookmarkRenameEditOpenRequest {
    pub uuid: String,
    pub folder: bool,
    pub draft: String,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct BookmarkEditInputRequest {
    pub draft: String,
}

#[vmux_api::ui_event]
pub struct BookmarkEditSubmitRequest;

#[vmux_api::ui_event]
pub struct BookmarkEditCloseRequest;

#[vmux_api::ui_event(Eq)]
pub struct BookmarkContextMenuRequest {
    pub active: bool,
}

#[vmux_api::contract(Eq, Default)]
pub struct BookmarkFolderCreateEffect {
    pub revision: u64,
    pub parent: Option<String>,
}

#[vmux_api::contract(Eq, Default)]
pub struct BookmarkRenameEffect {
    pub revision: u64,
    pub uuid: String,
}

#[vmux_api::contract(Eq, Default)]
pub struct BookmarkMenuEffect {
    pub create_folder: BookmarkFolderCreateEffect,
    pub rename: BookmarkRenameEffect,
}

#[vmux_api::contract(Default, Eq)]
pub struct BookmarkFolderEditState {
    pub open: bool,
    pub parent: Option<String>,
    pub draft: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct BookmarkRenameEditState {
    pub open: bool,
    pub uuid: String,
    pub folder: bool,
    pub draft: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct BookmarkEditState {
    pub create: BookmarkFolderEditState,
    pub rename: BookmarkRenameEditState,
}
