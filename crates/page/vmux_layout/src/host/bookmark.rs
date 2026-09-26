use crate::pane::{Pane, PaneSplit};
use crate::stack::{ActiveTabParam, Stack, focused_stack};
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::bookmark::{
    BookmarkAddRequest as BookmarkAddUiRequest, BookmarkContextMenuRequest, BookmarkDropRequest,
    BookmarkDropSource, BookmarkDropTarget,
    BookmarkFolderCreateRequest as BookmarkFolderCreateUiRequest,
    BookmarkFolderMoveRequest as BookmarkFolderMoveUiRequest,
    BookmarkFolderRemoveRequest as BookmarkFolderRemoveUiRequest,
    BookmarkFolderRenameRequest as BookmarkFolderRenameUiRequest,
    BookmarkFolderToggleRequest as BookmarkFolderToggleUiRequest, BookmarkMenuEntryRequest,
    BookmarkMenuFolderRequest, BookmarkMenuPinRequest, BookmarkMenuRootRequest,
    BookmarkMoveRequest as BookmarkMoveUiRequest, BookmarkOpenRequest,
    BookmarkPinRequest as BookmarkPinUiRequest, BookmarkPinUrlRequest as BookmarkPinUrlUiRequest,
    BookmarkRemoveRequest as BookmarkRemoveUiRequest,
    BookmarkRenameRequest as BookmarkRenameUiRequest, BookmarkTextInputRequest,
    BookmarkToggleRequest, BookmarkUnpinRequest as BookmarkUnpinUiRequest,
};
use vmux_core::host::page::PageManifest;
use vmux_core::{
    Bookmark, BookmarkOrder, Collapsed, Folder, LastActivatedAt, PageMetadata, Pin, Uuid,
};

use super::{command::LayoutRequestSet, stack::OpenRequest};

pub struct BookmarkPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BookmarkRequestSet;

impl Plugin for BookmarkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_command::CommandTypePlugin::<BookmarkToggleActiveRequest>::default(),
            vmux_command::CommandTypePlugin::<BookmarkPinActiveRequest>::default(),
            vmux_command::CommandTypePlugin::<CreateFolderRequest>::default(),
        ))
        .add_message::<ShowBookmarkMenuRequest>()
        .add_plugins(UiEventPlugin::<(
            BookmarkToggleRequest,
            BookmarkMenuRootRequest,
            BookmarkMenuPinRequest,
            BookmarkMenuEntryRequest,
            BookmarkMenuFolderRequest,
            BookmarkOpenRequest,
            BookmarkAddUiRequest,
            BookmarkPinUrlUiRequest,
            BookmarkRemoveUiRequest,
            BookmarkRenameUiRequest,
            BookmarkMoveUiRequest,
        )>::default())
        .add_plugins(UiEventPlugin::<(
            BookmarkPinUiRequest,
            BookmarkUnpinUiRequest,
            BookmarkFolderToggleUiRequest,
            BookmarkFolderCreateUiRequest,
            BookmarkFolderMoveUiRequest,
            BookmarkFolderRenameUiRequest,
            BookmarkFolderRemoveUiRequest,
            BookmarkTextInputRequest,
            BookmarkContextMenuRequest,
            BookmarkDropRequest,
        )>::default())
        .add_observer(on_bookmark_toggle_request)
        .add_observer(on_bookmark_menu_request::<BookmarkMenuRootRequest>)
        .add_observer(on_bookmark_menu_request::<BookmarkMenuPinRequest>)
        .add_observer(on_bookmark_menu_request::<BookmarkMenuEntryRequest>)
        .add_observer(on_bookmark_menu_request::<BookmarkMenuFolderRequest>)
        .add_observer(on_bookmark_open_request)
        .add_observer(on_bookmark_add_request)
        .add_observer(on_bookmark_pin_url_request)
        .add_observer(on_bookmark_remove_request)
        .add_observer(on_bookmark_rename_request)
        .add_observer(on_bookmark_move_request)
        .add_observer(on_bookmark_pin_request)
        .add_observer(on_bookmark_unpin_request)
        .add_observer(on_bookmark_folder_toggle_request)
        .add_observer(on_bookmark_folder_create_request)
        .add_observer(on_bookmark_folder_move_request)
        .add_observer(on_bookmark_folder_rename_request)
        .add_observer(on_bookmark_folder_remove_request)
        .add_observer(on_bookmark_text_input_request)
        .add_observer(on_bookmark_context_menu_request)
        .add_observer(on_bookmark_drop_request)
        .add_systems(
            Update,
            (
                handle_bookmark_requests.in_set(LayoutRequestSet::Handle),
                (
                    apply_toggle_for_url_requests,
                    apply_add_requests,
                    apply_remove_requests,
                    apply_rename_requests,
                    apply_move_requests,
                    apply_move_pin_requests,
                    apply_reorder_pin_requests,
                    apply_create_folder_requests,
                    apply_move_folder_requests,
                    apply_remove_folder_requests,
                    apply_rename_folder_requests,
                    apply_toggle_folder_requests,
                    apply_pin_requests,
                    apply_pin_url_requests,
                    apply_unpin_requests,
                )
                    .chain()
                    .in_set(BookmarkRequestSet),
                sync_bookmark_metadata,
            )
                .chain(),
        );
    }
}

#[derive(vmux_macro::CommandBar)]
#[shortcut(direct = "Super+d")]
struct BookmarkToggleActiveRequest;

#[derive(vmux_macro::CommandBar)]
struct BookmarkPinActiveRequest;

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct CreateFolderRequest {
    pub name: String,
    pub parent: Option<String>,
}

impl CreateFolderRequest {
    pub fn root(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parent: None,
        }
    }

    pub fn child(name: impl Into<String>, parent: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parent: Some(parent.into()),
        }
    }
}

impl vmux_command::CommandRequest for CreateFolderRequest {
    fn definitions() -> Vec<vmux_command::CommandDefinition> {
        vec![
            vmux_command::CommandDefinition::new("bookmark_new_folder", "New Folder", "Bookmark")
                .hidden(),
        ]
    }
}

impl TryFrom<&vmux_command::CommandInvocation> for CreateFolderRequest {
    type Error = ();

    fn try_from(invocation: &vmux_command::CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "bookmark_new_folder")
            .then(|| Self::root("New Folder"))
            .ok_or(())
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct ToggleForUrlRequest {
    pub metadata: PageMetadata,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct AddRequest {
    pub metadata: PageMetadata,
    pub folder: Option<String>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct RemoveRequest {
    pub uuid: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct RenameRequest {
    pub uuid: String,
    pub name: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct MoveRequest {
    pub uuid: String,
    pub folder: Option<String>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct MovePinRequest {
    pub uuid: String,
    pub folder: Option<String>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct ReorderPinRequest {
    pub uuid: String,
    pub target_uuid: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct MoveFolderRequest {
    pub uuid: String,
    pub parent: Option<String>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct RemoveFolderRequest {
    pub uuid: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct RenameFolderRequest {
    pub uuid: String,
    pub name: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct ToggleFolderRequest {
    pub uuid: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct PinRequest {
    pub uuid: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct PinUrlRequest {
    pub metadata: PageMetadata,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct UnpinRequest {
    pub uuid: String,
}

#[derive(Message, Clone, Debug)]
pub struct ShowBookmarkMenuRequest {
    pub webview: Entity,
    pub target: BookmarkMenuTarget,
}

#[derive(Clone, Debug)]
pub enum BookmarkMenuTarget {
    Root,
    Pin {
        uuid: String,
    },
    Entry {
        uuid: String,
    },
    Folder {
        uuid: String,
        active_page: Option<PageMetadata>,
    },
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BookmarkTextInputActive;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BookmarkContextMenuActive;

fn on_bookmark_context_menu_request(
    trigger: On<UiInput<BookmarkContextMenuRequest>>,
    mut commands: Commands,
) {
    let Ok(mut webview) = commands.get_entity(trigger.event().webview) else {
        return;
    };
    if trigger.event().payload.active {
        webview.insert(BookmarkContextMenuActive);
    } else {
        webview.remove::<BookmarkContextMenuActive>();
    }
}

fn on_bookmark_text_input_request(
    trigger: On<UiInput<BookmarkTextInputRequest>>,
    mut commands: Commands,
) {
    let Ok(mut webview) = commands.get_entity(trigger.event().webview) else {
        return;
    };
    if trigger.event().payload.active {
        webview.insert(BookmarkTextInputActive);
    } else {
        webview.remove::<BookmarkTextInputActive>();
    }
}

fn new_uuid() -> Uuid {
    Uuid(uuid::Uuid::new_v4().to_string())
}

fn find_by_uuid(target: &str, q: &Query<(Entity, &Uuid)>) -> Option<Entity> {
    q.iter()
        .find(|(_, id)| id.0 == target)
        .map(|(entity, _)| entity)
}

fn next_top_order(orders: impl Iterator<Item = u32>) -> BookmarkOrder {
    BookmarkOrder(orders.max().map(|m| m + 1).unwrap_or(0))
}

fn can_parent_folder(folder: Entity, parent: Entity, child_of_q: &Query<&ChildOf>) -> bool {
    let mut current = Some(parent);
    let mut seen = std::collections::HashSet::new();
    while let Some(entity) = current {
        if entity == folder || !seen.insert(entity) {
            return false;
        }
        current = child_of_q.get(entity).ok().map(Relationship::get);
    }
    true
}

fn apply_toggle_for_url_requests(
    mut reader: MessageReader<ToggleForUrlRequest>,
    bookmarks: Query<(Entity, &PageMetadata), With<Bookmark>>,
    pinned: Query<(Entity, &PageMetadata), With<Pin>>,
    orders: Query<&BookmarkOrder>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let metadata = &request.metadata;
        let existing = bookmarks
            .iter()
            .find(|(_, candidate)| candidate.url == metadata.url)
            .map(|(entity, _)| entity);
        if let Some(entity) = existing {
            if pinned.get(entity).is_ok() {
                commands
                    .entity(entity)
                    .remove::<Bookmark>()
                    .remove::<ChildOf>();
            } else {
                commands.entity(entity).despawn();
            }
        } else if let Some((entity, _)) = pinned
            .iter()
            .find(|(_, candidate)| candidate.url == metadata.url)
        {
            commands.entity(entity).insert((Bookmark, metadata.clone()));
        } else {
            let order = next_top_order(orders.iter().map(|order| order.0));
            commands.spawn((Bookmark, new_uuid(), metadata.clone(), order));
        }
    }
}

fn apply_add_requests(
    mut reader: MessageReader<AddRequest>,
    ids: Query<(Entity, &Uuid)>,
    bookmarks: Query<(Entity, &PageMetadata), With<Bookmark>>,
    pinned: Query<(Entity, &PageMetadata), With<Pin>>,
    folders: Query<(), With<Folder>>,
    orders: Query<&BookmarkOrder>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let folder_entity = request.folder.as_ref().and_then(|folder_uuid| {
            let entity = find_by_uuid(folder_uuid, &ids)?;
            folders.get(entity).ok().map(|_| entity)
        });
        if request.folder.is_some() && folder_entity.is_none() {
            continue;
        }
        if let Some((entity, _)) = bookmarks
            .iter()
            .find(|(_, metadata)| metadata.url == request.metadata.url)
        {
            let mut entity_commands = commands.entity(entity);
            entity_commands.insert(request.metadata.clone());
            if let Some(folder_entity) = folder_entity {
                entity_commands.insert(ChildOf(folder_entity));
            }
            continue;
        }
        if let Some((entity, _)) = pinned
            .iter()
            .find(|(_, metadata)| metadata.url == request.metadata.url)
        {
            let mut entity_commands = commands.entity(entity);
            entity_commands.insert((Bookmark, request.metadata.clone()));
            if let Some(folder_entity) = folder_entity {
                entity_commands.insert(ChildOf(folder_entity));
            }
            continue;
        }
        let order = next_top_order(orders.iter().map(|order| order.0));
        let mut entity = commands.spawn((Bookmark, new_uuid(), request.metadata.clone(), order));
        if let Some(folder_entity) = folder_entity {
            entity.insert(ChildOf(folder_entity));
        }
    }
}

fn apply_remove_requests(
    mut reader: MessageReader<RemoveRequest>,
    ids: Query<(Entity, &Uuid)>,
    bookmarks: Query<(), With<Bookmark>>,
    pinned: Query<(), With<Pin>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        if let Some(entity) = find_by_uuid(&request.uuid, &ids)
            && (bookmarks.get(entity).is_ok() || pinned.get(entity).is_ok())
        {
            if bookmarks.get(entity).is_ok() && pinned.get(entity).is_ok() {
                commands
                    .entity(entity)
                    .remove::<Bookmark>()
                    .remove::<ChildOf>();
            } else {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn apply_rename_requests(
    mut reader: MessageReader<RenameRequest>,
    ids: Query<(Entity, &Uuid)>,
    bookmarks: Query<&PageMetadata, With<Bookmark>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let name = request.name.trim();
        if name.is_empty() {
            continue;
        }
        if let Some(entity) = find_by_uuid(&request.uuid, &ids)
            && let Ok(metadata) = bookmarks.get(entity)
        {
            let mut metadata = metadata.clone();
            metadata.title = name.to_string();
            commands.entity(entity).insert(metadata);
        }
    }
}

fn apply_move_requests(
    mut reader: MessageReader<MoveRequest>,
    ids: Query<(Entity, &Uuid)>,
    bookmarks: Query<(), With<Bookmark>>,
    folders: Query<(), With<Folder>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        if let Some(entity) = find_by_uuid(&request.uuid, &ids)
            && bookmarks.get(entity).is_ok()
        {
            if let Some(folder_uuid) = &request.folder
                && let Some(folder_entity) = find_by_uuid(folder_uuid, &ids)
                && folders.get(folder_entity).is_ok()
            {
                commands.entity(entity).insert(ChildOf(folder_entity));
            } else if request.folder.is_none() {
                commands.entity(entity).remove::<ChildOf>();
            }
        }
    }
}

fn apply_move_pin_requests(
    mut reader: MessageReader<MovePinRequest>,
    ids: Query<(Entity, &Uuid)>,
    pinned: Query<(), With<Pin>>,
    folders: Query<(), With<Folder>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let folder_entity = request.folder.as_ref().and_then(|folder_uuid| {
            let entity = find_by_uuid(folder_uuid, &ids)?;
            folders.get(entity).ok().map(|_| entity)
        });
        if request.folder.is_some() && folder_entity.is_none() {
            continue;
        }
        if let Some(entity) = find_by_uuid(&request.uuid, &ids)
            && pinned.get(entity).is_ok()
        {
            let mut entity_commands = commands.entity(entity);
            entity_commands.insert(Bookmark);
            if let Some(folder_entity) = folder_entity {
                entity_commands.insert(ChildOf(folder_entity));
            } else {
                entity_commands.remove::<ChildOf>();
            }
        }
    }
}

fn apply_reorder_pin_requests(
    mut reader: MessageReader<ReorderPinRequest>,
    pin_orders: Query<(Entity, &Uuid, &BookmarkOrder), With<Pin>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let mut pins = pin_orders
            .iter()
            .map(|(entity, uuid, order)| (entity, uuid.0.clone(), order.0))
            .collect::<Vec<_>>();
        pins.sort_by_key(|(entity, _, order)| (*order, entity.to_bits()));
        let Some(source_index) = pins.iter().position(|(_, id, _)| id == &request.uuid) else {
            continue;
        };
        let Some(target_index) = pins
            .iter()
            .position(|(_, id, _)| id == &request.target_uuid)
        else {
            continue;
        };
        if source_index == target_index {
            continue;
        }
        let order_values = pins.iter().map(|(_, _, order)| *order).collect::<Vec<_>>();
        let moved = pins.remove(source_index);
        pins.insert(target_index, moved);
        for ((entity, _, _), order) in pins.into_iter().zip(order_values) {
            commands.entity(entity).insert(BookmarkOrder(order));
        }
    }
}

fn apply_create_folder_requests(
    mut reader: MessageReader<CreateFolderRequest>,
    ids: Query<(Entity, &Uuid)>,
    folders: Query<(), With<Folder>>,
    orders: Query<&BookmarkOrder>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let name = request.name.trim();
        if name.is_empty() {
            continue;
        }
        let parent_entity = if let Some(parent) = &request.parent {
            let Some(parent_entity) = find_by_uuid(parent, &ids) else {
                continue;
            };
            if folders.get(parent_entity).is_err() {
                continue;
            }
            Some(parent_entity)
        } else {
            None
        };
        let order = next_top_order(orders.iter().map(|order| order.0));
        let mut entity = commands.spawn((Folder, new_uuid(), Name::new(name.to_string()), order));
        if let Some(parent_entity) = parent_entity {
            entity.insert(ChildOf(parent_entity));
        }
    }
}

fn apply_move_folder_requests(
    mut reader: MessageReader<MoveFolderRequest>,
    ids: Query<(Entity, &Uuid)>,
    folders: Query<(), With<Folder>>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let Some(folder_entity) = find_by_uuid(&request.uuid, &ids) else {
            continue;
        };
        if folders.get(folder_entity).is_err() {
            continue;
        }
        if let Some(parent_uuid) = &request.parent {
            let Some(parent_entity) = find_by_uuid(parent_uuid, &ids) else {
                continue;
            };
            if folders.get(parent_entity).is_ok()
                && can_parent_folder(folder_entity, parent_entity, &child_of)
            {
                commands
                    .entity(folder_entity)
                    .insert(ChildOf(parent_entity));
            }
        } else {
            commands.entity(folder_entity).remove::<ChildOf>();
        }
    }
}

fn apply_remove_folder_requests(
    mut reader: MessageReader<RemoveFolderRequest>,
    ids: Query<(Entity, &Uuid)>,
    folders: Query<(), With<Folder>>,
    children: Query<&Children>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    for request in reader.read() {
        if let Some(folder_entity) = find_by_uuid(&request.uuid, &ids)
            && folders.get(folder_entity).is_ok()
        {
            let parent = child_of.get(folder_entity).ok().map(Relationship::get);
            if let Ok(folder_children) = children.get(folder_entity) {
                for child in folder_children.iter() {
                    if let Some(parent) = parent {
                        commands.entity(child).insert(ChildOf(parent));
                    } else {
                        commands.entity(child).remove::<ChildOf>();
                    }
                }
            }
            commands.entity(folder_entity).remove::<ChildOf>().despawn();
        }
    }
}

fn apply_rename_folder_requests(
    mut reader: MessageReader<RenameFolderRequest>,
    ids: Query<(Entity, &Uuid)>,
    folders: Query<(), With<Folder>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let name = request.name.trim();
        if name.is_empty() {
            continue;
        }
        if let Some(folder_entity) = find_by_uuid(&request.uuid, &ids)
            && folders.get(folder_entity).is_ok()
        {
            commands
                .entity(folder_entity)
                .insert(Name::new(name.to_string()));
        }
    }
}

fn apply_toggle_folder_requests(
    mut reader: MessageReader<ToggleFolderRequest>,
    ids: Query<(Entity, &Uuid)>,
    folders: Query<(), With<Folder>>,
    collapsed: Query<(), With<Collapsed>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        if let Some(folder_entity) = find_by_uuid(&request.uuid, &ids)
            && folders.get(folder_entity).is_ok()
        {
            if collapsed.get(folder_entity).is_ok() {
                commands.entity(folder_entity).remove::<Collapsed>();
            } else {
                commands.entity(folder_entity).insert(Collapsed);
            }
        }
    }
}

fn apply_pin_requests(
    mut reader: MessageReader<PinRequest>,
    ids: Query<(Entity, &Uuid)>,
    bookmarks: Query<(), With<Bookmark>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        if let Some(entity) = find_by_uuid(&request.uuid, &ids)
            && bookmarks.get(entity).is_ok()
        {
            commands.entity(entity).insert(Pin);
        }
    }
}

fn apply_pin_url_requests(
    mut reader: MessageReader<PinUrlRequest>,
    pinned: Query<(Entity, &PageMetadata), With<Pin>>,
    bookmarks: Query<(Entity, &PageMetadata), With<Bookmark>>,
    orders: Query<&BookmarkOrder>,
    mut commands: Commands,
) {
    for request in reader.read() {
        if let Some((entity, _)) = pinned
            .iter()
            .find(|(_, metadata)| metadata.url == request.metadata.url)
        {
            commands.entity(entity).insert(request.metadata.clone());
            continue;
        }
        if let Some((entity, _)) = bookmarks
            .iter()
            .find(|(_, metadata)| metadata.url == request.metadata.url)
        {
            commands
                .entity(entity)
                .insert((Pin, request.metadata.clone()));
            continue;
        }
        let order = next_top_order(orders.iter().map(|order| order.0));
        commands.spawn((Pin, new_uuid(), request.metadata.clone(), order));
    }
}

fn apply_unpin_requests(
    mut reader: MessageReader<UnpinRequest>,
    ids: Query<(Entity, &Uuid)>,
    pinned: Query<(), With<Pin>>,
    bookmarks: Query<(), With<Bookmark>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        if let Some(entity) = find_by_uuid(&request.uuid, &ids)
            && pinned.get(entity).is_ok()
        {
            if bookmarks.get(entity).is_ok() {
                commands.entity(entity).remove::<Pin>();
            } else {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn sync_bookmark_metadata(
    pages: Query<
        &PageMetadata,
        (
            With<Stack>,
            Changed<PageMetadata>,
            Without<Bookmark>,
            Without<Pin>,
        ),
    >,
    manifests: Query<Ref<PageManifest>>,
    bookmarks: Query<
        (Entity, Ref<PageMetadata>),
        (Or<(With<Bookmark>, With<Pin>)>, Without<Stack>),
    >,
    mut commands: Commands,
) {
    let manifests_changed = manifests
        .iter()
        .any(|manifest| manifest.is_added() || manifest.is_changed());
    let mut metadata_by_url = std::collections::HashMap::new();
    for page in &pages {
        if page.url.is_empty() {
            continue;
        }
        metadata_by_url.insert(page.url.clone(), page.clone());
    }
    for (entity, bookmark) in &bookmarks {
        let mut metadata = PageMetadata::clone(&bookmark);
        if let Some(page) = metadata_by_url.get(&bookmark.url) {
            if bookmark.title == bookmark.url && !page.title.is_empty() {
                metadata.title.clone_from(&page.title);
            }
            if !page.icon.is_none() && bookmark.icon != page.icon {
                metadata.icon.clone_from(&page.icon);
            }
        }
        if !manifests_changed
            && !bookmark.is_added()
            && !bookmark.is_changed()
            && metadata == *bookmark
        {
            continue;
        }
        let Some(manifest) = manifests
            .iter()
            .find(|manifest| manifest.answers_for(&metadata.url))
        else {
            if metadata != *bookmark {
                commands.entity(entity).insert(metadata);
            }
            continue;
        };
        if metadata.title == metadata.url {
            metadata.title = manifest.title.to_string();
        }
        if let Some(icon) = manifest.icon
            && metadata.icon != vmux_core::PageIcon::Builtin(icon)
        {
            metadata.icon = vmux_core::PageIcon::Builtin(icon);
        }
        if metadata != *bookmark {
            commands.entity(entity).insert(metadata);
        }
    }
}

fn on_bookmark_toggle_request(
    _trigger: On<UiInput<BookmarkToggleRequest>>,
    mut requests: MessageWriter<BookmarkToggleActiveRequest>,
) {
    requests.write(BookmarkToggleActiveRequest);
}

fn on_bookmark_open_request(
    trigger: On<UiInput<BookmarkOpenRequest>>,
    mut requests: MessageWriter<OpenRequest>,
) {
    requests.write(OpenRequest {
        url: Some(trigger.event().payload.url.clone()),
    });
}

fn on_bookmark_menu_request<R>(
    trigger: On<UiInput<R>>,
    mut menu_req: MessageWriter<ShowBookmarkMenuRequest>,
) where
    R: Clone + Send + Sync + 'static,
    BookmarkMenuTarget: From<R>,
{
    menu_req.write(ShowBookmarkMenuRequest {
        webview: trigger.event().webview,
        target: trigger.event().payload.clone().into(),
    });
}

fn on_bookmark_add_request(
    trigger: On<UiInput<BookmarkAddUiRequest>>,
    mut requests: MessageWriter<AddRequest>,
) {
    requests.write(AddRequest {
        metadata: trigger.event().payload.metadata.clone(),
        folder: trigger.event().payload.folder.clone(),
    });
}

fn on_bookmark_pin_url_request(
    trigger: On<UiInput<BookmarkPinUrlUiRequest>>,
    mut requests: MessageWriter<PinUrlRequest>,
) {
    requests.write(PinUrlRequest {
        metadata: trigger.event().payload.metadata.clone(),
    });
}

fn on_bookmark_remove_request(
    trigger: On<UiInput<BookmarkRemoveUiRequest>>,
    mut requests: MessageWriter<RemoveRequest>,
) {
    requests.write(RemoveRequest {
        uuid: trigger.event().payload.uuid.clone(),
    });
}

fn on_bookmark_rename_request(
    trigger: On<UiInput<BookmarkRenameUiRequest>>,
    mut requests: MessageWriter<RenameRequest>,
) {
    let name = trigger.event().payload.name.trim();
    if name.is_empty() {
        return;
    }
    requests.write(RenameRequest {
        uuid: trigger.event().payload.uuid.clone(),
        name: name.to_string(),
    });
}

fn on_bookmark_move_request(
    trigger: On<UiInput<BookmarkMoveUiRequest>>,
    mut requests: MessageWriter<MoveRequest>,
) {
    requests.write(MoveRequest {
        uuid: trigger.event().payload.uuid.clone(),
        folder: trigger.event().payload.folder.clone(),
    });
}

fn on_bookmark_drop_request(
    trigger: On<UiInput<BookmarkDropRequest>>,
    mut add_requests: MessageWriter<AddRequest>,
    mut move_requests: MessageWriter<MoveRequest>,
    mut move_pin_requests: MessageWriter<MovePinRequest>,
    mut reorder_pin_requests: MessageWriter<ReorderPinRequest>,
    mut move_folder_requests: MessageWriter<MoveFolderRequest>,
) {
    let request = &trigger.event().payload;
    match (&request.source, &request.target) {
        (BookmarkDropSource::Page { metadata }, BookmarkDropTarget::Root) => {
            add_requests.write(AddRequest {
                metadata: metadata.clone(),
                folder: None,
            });
        }
        (BookmarkDropSource::Page { metadata }, BookmarkDropTarget::Folder { uuid: folder }) => {
            add_requests.write(AddRequest {
                metadata: metadata.clone(),
                folder: Some(folder.clone()),
            });
        }
        (BookmarkDropSource::Bookmark { uuid }, BookmarkDropTarget::Root) => {
            move_requests.write(MoveRequest {
                uuid: uuid.clone(),
                folder: None,
            });
        }
        (BookmarkDropSource::Bookmark { uuid }, BookmarkDropTarget::Folder { uuid: folder }) => {
            move_requests.write(MoveRequest {
                uuid: uuid.clone(),
                folder: Some(folder.clone()),
            });
        }
        (BookmarkDropSource::Pin { uuid }, BookmarkDropTarget::Root) => {
            move_pin_requests.write(MovePinRequest {
                uuid: uuid.clone(),
                folder: None,
            });
        }
        (BookmarkDropSource::Pin { uuid }, BookmarkDropTarget::Folder { uuid: folder }) => {
            move_pin_requests.write(MovePinRequest {
                uuid: uuid.clone(),
                folder: Some(folder.clone()),
            });
        }
        (BookmarkDropSource::Pin { uuid }, BookmarkDropTarget::Pin { uuid: target_uuid })
            if uuid != target_uuid =>
        {
            reorder_pin_requests.write(ReorderPinRequest {
                uuid: uuid.clone(),
                target_uuid: target_uuid.clone(),
            });
        }
        (BookmarkDropSource::Folder { uuid }, BookmarkDropTarget::Root) => {
            move_folder_requests.write(MoveFolderRequest {
                uuid: uuid.clone(),
                parent: None,
            });
        }
        (BookmarkDropSource::Folder { uuid }, BookmarkDropTarget::Folder { uuid: parent })
            if uuid != parent =>
        {
            move_folder_requests.write(MoveFolderRequest {
                uuid: uuid.clone(),
                parent: Some(parent.clone()),
            });
        }
        _ => {}
    }
}

fn on_bookmark_pin_request(
    trigger: On<UiInput<BookmarkPinUiRequest>>,
    mut requests: MessageWriter<PinRequest>,
) {
    requests.write(PinRequest {
        uuid: trigger.event().payload.uuid.clone(),
    });
}

fn on_bookmark_unpin_request(
    trigger: On<UiInput<BookmarkUnpinUiRequest>>,
    mut requests: MessageWriter<UnpinRequest>,
) {
    requests.write(UnpinRequest {
        uuid: trigger.event().payload.uuid.clone(),
    });
}

fn on_bookmark_folder_toggle_request(
    trigger: On<UiInput<BookmarkFolderToggleUiRequest>>,
    mut requests: MessageWriter<ToggleFolderRequest>,
) {
    requests.write(ToggleFolderRequest {
        uuid: trigger.event().payload.uuid.clone(),
    });
}

fn on_bookmark_folder_create_request(
    trigger: On<UiInput<BookmarkFolderCreateUiRequest>>,
    mut requests: MessageWriter<CreateFolderRequest>,
) {
    let name = trigger.event().payload.name.trim();
    if name.is_empty() {
        return;
    }
    requests.write(CreateFolderRequest {
        name: name.to_string(),
        parent: trigger.event().payload.parent.clone(),
    });
}

fn on_bookmark_folder_move_request(
    trigger: On<UiInput<BookmarkFolderMoveUiRequest>>,
    mut requests: MessageWriter<MoveFolderRequest>,
) {
    requests.write(MoveFolderRequest {
        uuid: trigger.event().payload.uuid.clone(),
        parent: trigger.event().payload.parent.clone(),
    });
}

fn on_bookmark_folder_rename_request(
    trigger: On<UiInput<BookmarkFolderRenameUiRequest>>,
    mut rename_requests: MessageWriter<RenameFolderRequest>,
    mut remove_requests: MessageWriter<RemoveFolderRequest>,
) {
    let request = &trigger.event().payload;
    let name = request.name.trim();
    if name.is_empty() {
        remove_requests.write(RemoveFolderRequest {
            uuid: request.uuid.clone(),
        });
        return;
    }
    rename_requests.write(RenameFolderRequest {
        uuid: request.uuid.clone(),
        name: name.to_string(),
    });
}

fn on_bookmark_folder_remove_request(
    trigger: On<UiInput<BookmarkFolderRemoveUiRequest>>,
    mut requests: MessageWriter<RemoveFolderRequest>,
) {
    requests.write(RemoveFolderRequest {
        uuid: trigger.event().payload.uuid.clone(),
    });
}

impl From<BookmarkMenuRootRequest> for BookmarkMenuTarget {
    fn from(_: BookmarkMenuRootRequest) -> Self {
        Self::Root
    }
}

impl From<BookmarkMenuPinRequest> for BookmarkMenuTarget {
    fn from(request: BookmarkMenuPinRequest) -> Self {
        Self::Pin { uuid: request.uuid }
    }
}

impl From<BookmarkMenuEntryRequest> for BookmarkMenuTarget {
    fn from(request: BookmarkMenuEntryRequest) -> Self {
        Self::Entry { uuid: request.uuid }
    }
}

impl From<BookmarkMenuFolderRequest> for BookmarkMenuTarget {
    fn from(request: BookmarkMenuFolderRequest) -> Self {
        Self::Folder {
            uuid: request.uuid,
            active_page: request.active_page,
        }
    }
}

fn handle_bookmark_requests(
    mut toggles: MessageReader<BookmarkToggleActiveRequest>,
    mut pins: MessageReader<BookmarkPinActiveRequest>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_meta: Query<&PageMetadata, With<Stack>>,
    mut toggle_requests: MessageWriter<ToggleForUrlRequest>,
    mut pin_requests: MessageWriter<PinUrlRequest>,
) {
    let toggle_count = toggles.read().count();
    let pin_count = pins.read().count();
    if toggle_count == 0 && pin_count == 0 {
        return;
    }
    let (_, _, Some(stack)) = focused_stack(
        active_tab_param.get(),
        &all_children,
        &leaf_panes,
        &pane_ts,
        &pane_children,
        &stack_ts,
    ) else {
        return;
    };
    let Ok(meta) = stack_meta.get(stack) else {
        return;
    };
    if meta.url.is_empty() {
        return;
    }
    for _ in 0..toggle_count {
        toggle_requests.write(ToggleForUrlRequest {
            metadata: meta.clone(),
        });
    }
    for _ in 0..pin_count {
        pin_requests.write(PinUrlRequest {
            metadata: meta.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::PageIcon;

    #[test]
    fn command_id_dispatches_the_typed_bookmark_request() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandTypePlugin::<BookmarkToggleActiveRequest>::default());
        let caller = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<Messages<vmux_command::CommandInvocation>>()
            .write(vmux_command::CommandInvocation::new(
                caller,
                "bookmark_toggle_active",
            ));

        app.update();

        let request_count = app
            .world_mut()
            .resource_mut::<Messages<BookmarkToggleActiveRequest>>()
            .drain()
            .count();
        assert_eq!(request_count, 1);
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::LayoutContractPlugin))
            .add_systems(
                Update,
                (
                    apply_toggle_for_url_requests,
                    apply_add_requests,
                    apply_remove_requests,
                    apply_rename_requests,
                    apply_move_requests,
                    apply_move_pin_requests,
                    apply_reorder_pin_requests,
                    apply_create_folder_requests,
                    apply_move_folder_requests,
                    apply_remove_folder_requests,
                    apply_rename_folder_requests,
                    apply_toggle_folder_requests,
                    apply_pin_requests,
                    apply_pin_url_requests,
                    apply_unpin_requests,
                    sync_bookmark_metadata,
                )
                    .chain(),
            );
        app
    }

    struct TestRequest;

    impl TestRequest {
        fn send<M: Message>(app: &mut App, request: M) {
            app.world_mut().resource_mut::<Messages<M>>().write(request);
            app.update();
        }
    }

    fn count<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, F>()
            .iter(app.world())
            .count()
    }

    fn metadata(title: &str) -> PageMetadata {
        PageMetadata {
            title: title.to_string(),
            url: "https://a.test".to_string(),
            ..default()
        }
    }

    #[test]
    fn open_event_requests_new_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<ShowBookmarkMenuRequest>()
            .add_message::<OpenRequest>()
            .add_observer(on_bookmark_open_request);
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(UiInput::<BookmarkOpenRequest> {
            webview,
            payload: BookmarkOpenRequest {
                url: "https://a.test".into(),
            },
        });
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .drain()
            .collect();
        assert_eq!(
            requests,
            vec![OpenRequest {
                url: Some("https://a.test".into()),
            }]
        );
    }

    #[test]
    fn drop_event_routes_domain_requests() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AddRequest>()
            .add_message::<MoveRequest>()
            .add_message::<MovePinRequest>()
            .add_message::<ReorderPinRequest>()
            .add_message::<MoveFolderRequest>()
            .add_observer(on_bookmark_drop_request);
        let webview = app.world_mut().spawn_empty().id();
        for payload in [
            BookmarkDropRequest {
                source: BookmarkDropSource::Page {
                    metadata: metadata("Page"),
                },
                target: BookmarkDropTarget::Folder {
                    uuid: "folder".into(),
                },
            },
            BookmarkDropRequest {
                source: BookmarkDropSource::Bookmark {
                    uuid: "bookmark".into(),
                },
                target: BookmarkDropTarget::Root,
            },
            BookmarkDropRequest {
                source: BookmarkDropSource::Pin { uuid: "pin".into() },
                target: BookmarkDropTarget::Folder {
                    uuid: "folder".into(),
                },
            },
            BookmarkDropRequest {
                source: BookmarkDropSource::Pin { uuid: "pin".into() },
                target: BookmarkDropTarget::Pin {
                    uuid: "target".into(),
                },
            },
            BookmarkDropRequest {
                source: BookmarkDropSource::Folder {
                    uuid: "folder".into(),
                },
                target: BookmarkDropTarget::Folder {
                    uuid: "parent".into(),
                },
            },
        ] {
            app.world_mut().trigger(UiInput { webview, payload });
        }
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<AddRequest>>()
                .drain()
                .collect::<Vec<_>>(),
            [AddRequest {
                metadata: metadata("Page"),
                folder: Some("folder".into()),
            }]
        );
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<MoveRequest>>()
                .drain()
                .collect::<Vec<_>>(),
            [MoveRequest {
                uuid: "bookmark".into(),
                folder: None,
            }]
        );
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<MovePinRequest>>()
                .drain()
                .collect::<Vec<_>>(),
            [MovePinRequest {
                uuid: "pin".into(),
                folder: Some("folder".into()),
            }]
        );
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<ReorderPinRequest>>()
                .drain()
                .collect::<Vec<_>>(),
            [ReorderPinRequest {
                uuid: "pin".into(),
                target_uuid: "target".into(),
            }]
        );
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<MoveFolderRequest>>()
                .drain()
                .collect::<Vec<_>>(),
            [MoveFolderRequest {
                uuid: "folder".into(),
                parent: Some("parent".into()),
            }]
        );
    }

    #[test]
    fn text_input_event_toggles_layout_keyboard_marker() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_observer(on_bookmark_text_input_request);
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut()
            .trigger(UiInput::<BookmarkTextInputRequest> {
                webview,
                payload: BookmarkTextInputRequest { active: true },
            });
        app.update();
        assert!(
            app.world()
                .entity(webview)
                .contains::<BookmarkTextInputActive>()
        );
        app.world_mut()
            .trigger(UiInput::<BookmarkTextInputRequest> {
                webview,
                payload: BookmarkTextInputRequest { active: false },
            });
        app.update();
        assert!(
            !app.world()
                .entity(webview)
                .contains::<BookmarkTextInputActive>()
        );
    }

    #[test]
    fn context_menu_event_toggles_layout_pointer_marker() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_observer(on_bookmark_context_menu_request);
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut()
            .trigger(UiInput::<BookmarkContextMenuRequest> {
                webview,
                payload: BookmarkContextMenuRequest { active: true },
            });
        app.update();
        assert!(
            app.world()
                .entity(webview)
                .contains::<BookmarkContextMenuActive>()
        );
        app.world_mut()
            .trigger(UiInput::<BookmarkContextMenuRequest> {
                webview,
                payload: BookmarkContextMenuRequest { active: false },
            });
        app.update();
        assert!(
            !app.world()
                .entity(webview)
                .contains::<BookmarkContextMenuActive>()
        );
    }

    #[test]
    fn add_creates_bookmark_entity() {
        let mut app = test_app();
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: None,
            },
        );
        assert_eq!(count::<With<Bookmark>>(&mut app), 1);
    }

    #[test]
    fn add_accepts_any_page_url() {
        let mut app = test_app();
        for url in [
            "vmux://projects/",
            "https://example.com/docs",
            "file:///tmp/readme.md",
        ] {
            TestRequest::send(
                &mut app,
                AddRequest {
                    metadata: PageMetadata {
                        title: url.into(),
                        url: url.into(),
                        ..default()
                    },
                    folder: None,
                },
            );
        }
        let mut urls = app
            .world_mut()
            .query_filtered::<&PageMetadata, With<Bookmark>>()
            .iter(app.world())
            .map(|metadata| metadata.url.clone())
            .collect::<Vec<_>>();
        urls.sort();
        assert_eq!(
            urls,
            [
                "file:///tmp/readme.md",
                "https://example.com/docs",
                "vmux://projects/",
            ]
        );
    }

    #[test]
    fn reorder_pin_moves_source_to_target_slot() {
        let mut app = test_app();
        for (uuid, order) in [("a", 2), ("b", 5), ("c", 9)] {
            app.world_mut()
                .spawn((Pin, Uuid(uuid.into()), metadata(uuid), BookmarkOrder(order)));
        }

        TestRequest::send(
            &mut app,
            ReorderPinRequest {
                uuid: "a".into(),
                target_uuid: "c".into(),
            },
        );

        let mut pins = app
            .world_mut()
            .query_filtered::<(&Uuid, &BookmarkOrder), With<Pin>>()
            .iter(app.world())
            .map(|(uuid, order)| (order.0, uuid.0.clone()))
            .collect::<Vec<_>>();
        pins.sort();
        assert_eq!(pins, [(2, "b".into()), (5, "c".into()), (9, "a".into())]);
    }

    #[test]
    fn bookmark_entities_are_not_space_save_entities() {
        let mut app = test_app();
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: None,
            },
        );
        TestRequest::send(&mut app, CreateFolderRequest::root("PRs"));
        assert_eq!(count::<With<moonshine_save::prelude::Save>>(&mut app), 0);
    }

    #[test]
    fn add_preserves_page_metadata() {
        let mut app = test_app();
        let expected = PageMetadata {
            title: "Start".into(),
            url: "vmux://start/".into(),
            icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Sparkles),
            bg_color: Some("#111111".into()),
        };
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: expected.clone(),
                folder: None,
            },
        );
        let actual = app
            .world_mut()
            .query_filtered::<&PageMetadata, With<Bookmark>>()
            .single(app.world())
            .unwrap();
        assert_eq!(actual, &expected);
    }

    #[test]
    fn live_page_icon_replaces_the_bookmark_icon_without_renaming_it() {
        let mut app = test_app();
        let bookmark = app
            .world_mut()
            .spawn((
                Bookmark,
                PageMetadata {
                    title: "Renamed bookmark".into(),
                    url: "https://a.test".into(),
                    icon: PageIcon::Favicon("https://old.test/icon.png".into()),
                    bg_color: None,
                },
            ))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                title: "Live title".into(),
                url: "https://a.test".into(),
                icon: PageIcon::Favicon("https://a.test/favicon.ico".into()),
                bg_color: None,
            },
        ));

        app.update();

        let metadata = app.world().get::<PageMetadata>(bookmark).unwrap();
        assert_eq!(metadata.title, "Renamed bookmark");
        assert_eq!(
            metadata.icon,
            PageIcon::Favicon("https://a.test/favicon.ico".into())
        );
    }

    #[test]
    fn live_page_title_replaces_a_seeded_url_title() {
        let mut app = test_app();
        let bookmark = app
            .world_mut()
            .spawn((
                Pin,
                PageMetadata {
                    title: "vmux://history/".into(),
                    url: "vmux://history/".into(),
                    icon: PageIcon::None,
                    bg_color: None,
                },
            ))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                title: "History".into(),
                url: "vmux://history/".into(),
                icon: PageIcon::Favicon("vmux://history/assets/favicons/history.svg".into()),
                bg_color: None,
            },
        ));

        app.update();

        let metadata = app.world().get::<PageMetadata>(bookmark).unwrap();
        assert_eq!(metadata.title, "History");
        assert_eq!(
            metadata.icon,
            PageIcon::Favicon("vmux://history/assets/favicons/history.svg".into())
        );
    }

    #[test]
    fn manifest_fills_seeded_internal_bookmark_metadata() {
        let mut app = test_app();
        let bookmark = app
            .world_mut()
            .spawn((
                Pin,
                PageMetadata {
                    title: "vmux://simulator/".into(),
                    url: "vmux://simulator/".into(),
                    icon: PageIcon::None,
                    bg_color: None,
                },
            ))
            .id();
        app.world_mut().spawn(PageManifest {
            host: "simulator",
            title: "Simulator",
            title_message_id: None,
            replaces_command: None,
            keywords: &[],
            icon: Some(vmux_core::BuiltinIcon::Smartphone),
            command_bar: true,
        });

        app.update();

        let metadata = app.world().get::<PageMetadata>(bookmark).unwrap();
        assert_eq!(metadata.title, "Simulator");
        assert_eq!(
            metadata.icon,
            PageIcon::Builtin(vmux_core::BuiltinIcon::Smartphone)
        );
    }

    #[test]
    fn live_page_without_an_icon_keeps_the_bookmark_stable_while_loading() {
        let mut app = test_app();
        let bookmark = app
            .world_mut()
            .spawn((
                Bookmark,
                PageMetadata {
                    title: "A".into(),
                    url: "https://a.test".into(),
                    icon: PageIcon::Favicon("https://old.test/icon.png".into()),
                    bg_color: None,
                },
            ))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                title: "A".into(),
                url: "https://a.test".into(),
                icon: PageIcon::None,
                bg_color: None,
            },
        ));

        app.update();

        assert_eq!(
            app.world().get::<PageMetadata>(bookmark).unwrap().icon,
            PageIcon::Favicon("https://old.test/icon.png".into())
        );
    }

    #[test]
    fn toggle_for_url_is_idempotent_add_then_remove() {
        let mut app = test_app();
        let op = || ToggleForUrlRequest {
            metadata: metadata("A"),
        };
        TestRequest::send(&mut app, op());
        assert_eq!(count::<With<Bookmark>>(&mut app), 1);
        TestRequest::send(&mut app, op());
        assert_eq!(count::<With<Bookmark>>(&mut app), 0);
    }

    #[test]
    fn remove_despawns_by_uuid() {
        let mut app = test_app();
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: None,
            },
        );
        let uuid = app
            .world_mut()
            .query_filtered::<&Uuid, With<Bookmark>>()
            .single(app.world())
            .unwrap()
            .0
            .clone();
        TestRequest::send(&mut app, RemoveRequest { uuid });
        assert_eq!(count::<With<Bookmark>>(&mut app), 0);
    }

    #[test]
    fn remove_bookmark_keeps_pin() {
        let mut app = test_app();
        TestRequest::send(
            &mut app,
            PinUrlRequest {
                metadata: metadata("A"),
            },
        );
        let uuid = app
            .world_mut()
            .query_filtered::<&Uuid, With<Pin>>()
            .single(app.world())
            .unwrap()
            .0
            .clone();
        TestRequest::send(
            &mut app,
            ToggleForUrlRequest {
                metadata: metadata("A"),
            },
        );
        TestRequest::send(&mut app, RemoveRequest { uuid });
        assert_eq!(count::<With<Bookmark>>(&mut app), 0);
        assert_eq!(count::<With<Pin>>(&mut app), 1);
    }

    fn folder_uuid(app: &mut App) -> String {
        app.world_mut()
            .query_filtered::<&Uuid, With<Folder>>()
            .single(app.world())
            .unwrap()
            .0
            .clone()
    }

    fn folder_named(app: &mut App, target: &str) -> (Entity, String) {
        app.world_mut()
            .query_filtered::<(Entity, &Name, &Uuid), With<Folder>>()
            .iter(app.world())
            .find(|(_, name, _)| name.as_str() == target)
            .map(|(entity, _, uuid)| (entity, uuid.0.clone()))
            .unwrap()
    }

    fn bookmark_uuid(app: &mut App) -> String {
        app.world_mut()
            .query_filtered::<&Uuid, With<Bookmark>>()
            .single(app.world())
            .unwrap()
            .0
            .clone()
    }

    #[test]
    fn add_into_folder_sets_childof() {
        let mut app = test_app();
        TestRequest::send(&mut app, CreateFolderRequest::root("PRs"));
        let fid = folder_uuid(&mut app);
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: Some(fid),
            },
        );
        assert_eq!(count::<(With<Bookmark>, With<ChildOf>)>(&mut app), 1);
    }

    #[test]
    fn folders_can_be_nested() {
        let mut app = test_app();
        TestRequest::send(&mut app, CreateFolderRequest::root("Work"));
        let (parent, parent_uuid) = folder_named(&mut app, "Work");
        TestRequest::send(&mut app, CreateFolderRequest::child("PRs", parent_uuid));
        let (child, _) = folder_named(&mut app, "PRs");
        assert_eq!(app.world().get::<ChildOf>(child).unwrap().get(), parent);
    }

    #[test]
    fn moving_folder_rejects_descendant_cycle() {
        let mut app = test_app();
        TestRequest::send(&mut app, CreateFolderRequest::root("Work"));
        let (parent, parent_uuid) = folder_named(&mut app, "Work");
        TestRequest::send(
            &mut app,
            CreateFolderRequest::child("PRs", parent_uuid.clone()),
        );
        let (_, child_uuid) = folder_named(&mut app, "PRs");
        TestRequest::send(
            &mut app,
            MoveFolderRequest {
                uuid: parent_uuid,
                parent: Some(child_uuid),
            },
        );
        assert!(app.world().get::<ChildOf>(parent).is_none());
    }

    #[test]
    fn removing_nested_folder_reparents_children_to_parent() {
        let mut app = test_app();
        TestRequest::send(&mut app, CreateFolderRequest::root("Work"));
        let (parent, parent_uuid) = folder_named(&mut app, "Work");
        TestRequest::send(&mut app, CreateFolderRequest::child("PRs", parent_uuid));
        let (_, child_uuid) = folder_named(&mut app, "PRs");
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: Some(child_uuid.clone()),
            },
        );
        TestRequest::send(&mut app, RemoveFolderRequest { uuid: child_uuid });
        let bookmark = app
            .world_mut()
            .query_filtered::<Entity, With<Bookmark>>()
            .single(app.world())
            .unwrap();
        assert_eq!(app.world().get::<ChildOf>(bookmark).unwrap().get(), parent);
    }

    #[test]
    fn add_existing_bookmark_moves_it_into_folder() {
        let mut app = test_app();
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: None,
            },
        );
        TestRequest::send(&mut app, CreateFolderRequest::root("PRs"));
        let fid = folder_uuid(&mut app);
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A updated"),
                folder: Some(fid),
            },
        );
        assert_eq!(count::<With<Bookmark>>(&mut app), 1);
        assert_eq!(count::<(With<Bookmark>, With<ChildOf>)>(&mut app), 1);
        let title = app
            .world_mut()
            .query_filtered::<&PageMetadata, With<Bookmark>>()
            .single(app.world())
            .unwrap()
            .title
            .clone();
        assert_eq!(title, "A updated");
    }

    #[test]
    fn add_existing_bookmark_without_folder_preserves_parent() {
        let mut app = test_app();
        TestRequest::send(&mut app, CreateFolderRequest::root("PRs"));
        let fid = folder_uuid(&mut app);
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: Some(fid),
            },
        );
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A updated"),
                folder: None,
            },
        );
        assert_eq!(count::<(With<Bookmark>, With<ChildOf>)>(&mut app), 1);
    }

    #[test]
    fn rename_updates_bookmark_title() {
        let mut app = test_app();
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: None,
            },
        );
        let uuid = bookmark_uuid(&mut app);
        TestRequest::send(
            &mut app,
            RenameRequest {
                uuid,
                name: "  Renamed  ".into(),
            },
        );
        let title = app
            .world_mut()
            .query_filtered::<&PageMetadata, With<Bookmark>>()
            .single(app.world())
            .unwrap()
            .title
            .clone();
        assert_eq!(title, "Renamed");
    }

    #[test]
    fn move_reparents_bookmark_and_returns_it_to_root() {
        let mut app = test_app();
        TestRequest::send(&mut app, CreateFolderRequest::root("PRs"));
        let fid = folder_uuid(&mut app);
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: None,
            },
        );
        let uuid = bookmark_uuid(&mut app);
        TestRequest::send(
            &mut app,
            MoveRequest {
                uuid: uuid.clone(),
                folder: Some(fid),
            },
        );
        assert_eq!(count::<(With<Bookmark>, With<ChildOf>)>(&mut app), 1);
        TestRequest::send(&mut app, MoveRequest { uuid, folder: None });
        assert_eq!(count::<(With<Bookmark>, Without<ChildOf>)>(&mut app), 1);
    }

    #[test]
    fn remove_folder_reparents_children_to_top_level() {
        let mut app = test_app();
        TestRequest::send(&mut app, CreateFolderRequest::root("PRs"));
        let fid = folder_uuid(&mut app);
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: Some(fid.clone()),
            },
        );
        TestRequest::send(&mut app, RemoveFolderRequest { uuid: fid });
        assert_eq!(count::<With<Folder>>(&mut app), 0);
        assert_eq!(count::<(With<Bookmark>, Without<ChildOf>)>(&mut app), 1);
    }

    #[test]
    fn toggle_folder_adds_then_removes_collapsed() {
        let mut app = test_app();
        TestRequest::send(&mut app, CreateFolderRequest::root("PRs"));
        let fid = folder_uuid(&mut app);
        TestRequest::send(&mut app, ToggleFolderRequest { uuid: fid.clone() });
        assert_eq!(count::<With<Collapsed>>(&mut app), 1);
        TestRequest::send(&mut app, ToggleFolderRequest { uuid: fid });
        assert_eq!(count::<With<Collapsed>>(&mut app), 0);
    }

    #[test]
    fn pin_keeps_bookmark_in_its_folder() {
        let mut app = test_app();
        TestRequest::send(&mut app, CreateFolderRequest::root("PRs"));
        let folder = folder_uuid(&mut app);
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: Some(folder),
            },
        );
        let uuid = app
            .world_mut()
            .query_filtered::<&Uuid, With<Bookmark>>()
            .single(app.world())
            .unwrap()
            .0
            .clone();
        TestRequest::send(&mut app, PinRequest { uuid: uuid.clone() });
        assert_eq!(count::<With<Pin>>(&mut app), 1);
        assert_eq!(
            count::<(With<Bookmark>, With<Pin>, With<ChildOf>)>(&mut app),
            1
        );
        TestRequest::send(&mut app, UnpinRequest { uuid });
        assert_eq!(
            count::<(With<Bookmark>, Without<Pin>, With<ChildOf>)>(&mut app),
            1
        );
    }

    #[test]
    fn pin_url_promotes_existing_bookmark_without_duplication() {
        let mut app = test_app();
        TestRequest::send(&mut app, CreateFolderRequest::root("PRs"));
        let folder = folder_uuid(&mut app);
        TestRequest::send(
            &mut app,
            AddRequest {
                metadata: metadata("A"),
                folder: Some(folder),
            },
        );
        TestRequest::send(
            &mut app,
            PinUrlRequest {
                metadata: metadata("A"),
            },
        );
        assert_eq!(count::<With<Bookmark>>(&mut app), 1);
        assert_eq!(count::<With<Pin>>(&mut app), 1);
        assert_eq!(
            count::<(With<Bookmark>, With<Pin>, With<ChildOf>)>(&mut app),
            1
        );
    }

    #[test]
    fn toggle_bookmark_on_pin_reuses_the_pin_entity() {
        let mut app = test_app();
        TestRequest::send(
            &mut app,
            PinUrlRequest {
                metadata: metadata("A"),
            },
        );
        TestRequest::send(
            &mut app,
            ToggleForUrlRequest {
                metadata: metadata("A"),
            },
        );
        assert_eq!(count::<With<PageMetadata>>(&mut app), 1);
        assert_eq!(count::<With<Bookmark>>(&mut app), 1);
        assert_eq!(count::<With<Pin>>(&mut app), 1);
        TestRequest::send(
            &mut app,
            ToggleForUrlRequest {
                metadata: metadata("A"),
            },
        );
        assert_eq!(count::<With<PageMetadata>>(&mut app), 1);
        assert_eq!(count::<With<Bookmark>>(&mut app), 0);
        assert_eq!(count::<With<Pin>>(&mut app), 1);
    }

    #[test]
    fn move_pin_adds_it_to_a_folder_without_unpinning() {
        let mut app = test_app();
        TestRequest::send(
            &mut app,
            PinUrlRequest {
                metadata: metadata("A"),
            },
        );
        TestRequest::send(&mut app, CreateFolderRequest::root("PRs"));
        let folder = folder_uuid(&mut app);
        let uuid = app
            .world_mut()
            .query_filtered::<&Uuid, With<Pin>>()
            .single(app.world())
            .unwrap()
            .0
            .clone();
        TestRequest::send(
            &mut app,
            MovePinRequest {
                uuid,
                folder: Some(folder),
            },
        );
        assert_eq!(count::<With<Pin>>(&mut app), 1);
        assert_eq!(
            count::<(With<Bookmark>, With<Pin>, With<ChildOf>)>(&mut app),
            1
        );
    }
}
