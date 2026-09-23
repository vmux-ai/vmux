use crate::pane::{Pane, PaneSplit};
use crate::stack::{ActiveTabParam, Stack, focused_stack};
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{BinReceive, UiEventPlugin};
use vmux_api::bookmark::{
    BookmarkAddRequest, BookmarkContextMenuRequest, BookmarkFolderCreateRequest,
    BookmarkFolderMoveRequest, BookmarkFolderRemoveRequest, BookmarkFolderRenameRequest,
    BookmarkFolderToggleRequest, BookmarkMenuEntryRequest, BookmarkMenuFolderRequest,
    BookmarkMenuPinRequest, BookmarkMenuRootRequest, BookmarkMovePinRequest, BookmarkMoveRequest,
    BookmarkOpenRequest, BookmarkPinRequest, BookmarkPinUrlRequest, BookmarkRemoveRequest,
    BookmarkRenameRequest, BookmarkReorderPinRequest, BookmarkTextInputRequest,
    BookmarkToggleRequest, BookmarkUnpinRequest,
};
use vmux_core::host::page::PageManifest;
use vmux_core::{
    Bookmark, BookmarkOrder, Collapsed, Folder, LastActivatedAt, PageMetadata, Pin, Uuid,
};

use super::{command::LayoutRequestSet, stack::StackRequest};

pub struct BookmarkPlugin;

impl Plugin for BookmarkPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ToggleActiveRequest>()
            .add_message::<PinActiveRequest>()
            .add_message::<CreateFolderRequest>()
            .add_message::<BookmarkMutation>()
            .add_message::<ShowBookmarkMenuRequest>()
            .add_plugins(UiEventPlugin::<(
                BookmarkToggleRequest,
                BookmarkMenuRootRequest,
                BookmarkMenuPinRequest,
                BookmarkMenuEntryRequest,
                BookmarkMenuFolderRequest,
                BookmarkOpenRequest,
                BookmarkAddRequest,
                BookmarkPinUrlRequest,
                BookmarkRemoveRequest,
                BookmarkRenameRequest,
                BookmarkMoveRequest,
                BookmarkMovePinRequest,
            )>::default())
            .add_plugins(UiEventPlugin::<(
                BookmarkReorderPinRequest,
                BookmarkPinRequest,
                BookmarkUnpinRequest,
                BookmarkFolderToggleRequest,
                BookmarkFolderCreateRequest,
                BookmarkFolderMoveRequest,
                BookmarkFolderRenameRequest,
                BookmarkFolderRemoveRequest,
                BookmarkTextInputRequest,
                BookmarkContextMenuRequest,
            )>::default())
            .add_observer(on_bookmark_toggle_request)
            .add_observer(on_bookmark_menu_request::<BookmarkMenuRootRequest>)
            .add_observer(on_bookmark_menu_request::<BookmarkMenuPinRequest>)
            .add_observer(on_bookmark_menu_request::<BookmarkMenuEntryRequest>)
            .add_observer(on_bookmark_menu_request::<BookmarkMenuFolderRequest>)
            .add_observer(on_bookmark_open_request)
            .add_observer(on_bookmark_mutation_request::<BookmarkAddRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkPinUrlRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkRemoveRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkRenameRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkMoveRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkMovePinRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkReorderPinRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkPinRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkUnpinRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkFolderToggleRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkFolderCreateRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkFolderMoveRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkFolderRenameRequest>)
            .add_observer(on_bookmark_mutation_request::<BookmarkFolderRemoveRequest>)
            .add_observer(on_bookmark_text_input_request)
            .add_observer(on_bookmark_context_menu_request)
            .add_systems(
                Update,
                (
                    handle_bookmark_requests.in_set(LayoutRequestSet::Handle),
                    apply_bookmark_mutations,
                    sync_bookmark_metadata,
                )
                    .chain(),
            );
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToggleActiveRequest;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinActiveRequest;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreateFolderRequest;

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub enum BookmarkMutation {
    ToggleForUrl {
        metadata: PageMetadata,
    },
    Add {
        metadata: PageMetadata,
        folder: Option<String>,
    },
    Remove {
        uuid: String,
    },
    Rename {
        uuid: String,
        name: String,
    },
    Move {
        uuid: String,
        folder: Option<String>,
    },
    MovePin {
        uuid: String,
        folder: Option<String>,
    },
    ReorderPin {
        uuid: String,
        target_uuid: String,
    },
    AddFolder {
        name: String,
    },
    AddFolderIn {
        name: String,
        parent: String,
    },
    MoveFolder {
        uuid: String,
        parent: Option<String>,
    },
    RemoveFolder {
        uuid: String,
    },
    RenameFolder {
        uuid: String,
        name: String,
    },
    ToggleFolder {
        uuid: String,
    },
    Pin {
        uuid: String,
    },
    PinUrl {
        metadata: PageMetadata,
    },
    Unpin {
        uuid: String,
    },
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
    trigger: On<BinReceive<BookmarkContextMenuRequest>>,
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
    trigger: On<BinReceive<BookmarkTextInputRequest>>,
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

fn apply_bookmark_mutations(
    mut reader: MessageReader<BookmarkMutation>,
    ids: Query<(Entity, &Uuid)>,
    bookmarks: Query<(Entity, &PageMetadata), With<Bookmark>>,
    pinned: Query<(Entity, &PageMetadata), With<Pin>>,
    folder_q: Query<(), With<Folder>>,
    collapsed_q: Query<(), With<Collapsed>>,
    orders: Query<&BookmarkOrder>,
    pin_orders: Query<(Entity, &Uuid, &BookmarkOrder), With<Pin>>,
    children_q: Query<&Children>,
    child_of_q: Query<&ChildOf>,
    mut commands: Commands,
) {
    for op in reader.read() {
        match op {
            BookmarkMutation::ToggleForUrl { metadata } => {
                let existing = bookmarks
                    .iter()
                    .find(|(_, meta)| meta.url == metadata.url)
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
                } else if let Some((entity, _)) =
                    pinned.iter().find(|(_, meta)| meta.url == metadata.url)
                {
                    commands.entity(entity).insert((Bookmark, metadata.clone()));
                } else {
                    let order = next_top_order(orders.iter().map(|o| o.0));
                    commands.spawn((Bookmark, new_uuid(), metadata.clone(), order));
                }
            }
            BookmarkMutation::Add { metadata, folder } => {
                let folder_entity = folder.as_ref().and_then(|folder_uuid| {
                    let entity = find_by_uuid(folder_uuid, &ids)?;
                    folder_q.get(entity).ok().map(|_| entity)
                });
                if folder.is_some() && folder_entity.is_none() {
                    continue;
                }
                if let Some((entity, _)) =
                    bookmarks.iter().find(|(_, meta)| meta.url == metadata.url)
                {
                    let mut entity_commands = commands.entity(entity);
                    entity_commands.insert(metadata.clone());
                    if let Some(folder_entity) = folder_entity {
                        entity_commands.insert(ChildOf(folder_entity));
                    }
                    continue;
                }
                if let Some((entity, _)) = pinned.iter().find(|(_, meta)| meta.url == metadata.url)
                {
                    let mut entity_commands = commands.entity(entity);
                    entity_commands.insert((Bookmark, metadata.clone()));
                    if let Some(folder_entity) = folder_entity {
                        entity_commands.insert(ChildOf(folder_entity));
                    }
                    continue;
                }
                let order = next_top_order(orders.iter().map(|o| o.0));
                let mut e = commands.spawn((Bookmark, new_uuid(), metadata.clone(), order));
                if let Some(folder_entity) = folder_entity {
                    e.insert(ChildOf(folder_entity));
                }
            }
            BookmarkMutation::Remove { uuid } => {
                if let Some(entity) = find_by_uuid(uuid, &ids)
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
            BookmarkMutation::Rename { uuid, name } => {
                if let Some(entity) = find_by_uuid(uuid, &ids)
                    && let Ok((_, metadata)) = bookmarks.get(entity)
                {
                    let mut metadata = metadata.clone();
                    metadata.title = name.clone();
                    commands.entity(entity).insert(metadata);
                }
            }
            BookmarkMutation::Move { uuid, folder } => {
                if let Some(entity) = find_by_uuid(uuid, &ids)
                    && bookmarks.get(entity).is_ok()
                {
                    if let Some(folder_uuid) = folder
                        && let Some(folder_entity) = find_by_uuid(folder_uuid, &ids)
                        && folder_q.get(folder_entity).is_ok()
                    {
                        commands.entity(entity).insert(ChildOf(folder_entity));
                    } else if folder.is_none() {
                        commands.entity(entity).remove::<ChildOf>();
                    }
                }
            }
            BookmarkMutation::MovePin { uuid, folder } => {
                let folder_entity = folder.as_ref().and_then(|folder_uuid| {
                    let entity = find_by_uuid(folder_uuid, &ids)?;
                    folder_q.get(entity).ok().map(|_| entity)
                });
                if folder.is_some() && folder_entity.is_none() {
                    continue;
                }
                if let Some(entity) = find_by_uuid(uuid, &ids)
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
            BookmarkMutation::ReorderPin { uuid, target_uuid } => {
                let mut pins = pin_orders
                    .iter()
                    .map(|(entity, uuid, order)| (entity, uuid.0.clone(), order.0))
                    .collect::<Vec<_>>();
                pins.sort_by_key(|(entity, _, order)| (*order, entity.to_bits()));
                let Some(source_index) = pins.iter().position(|(_, id, _)| id == uuid) else {
                    continue;
                };
                let Some(target_index) = pins.iter().position(|(_, id, _)| id == target_uuid)
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
            BookmarkMutation::AddFolder { name } => {
                let order = next_top_order(orders.iter().map(|o| o.0));
                commands.spawn((Folder, new_uuid(), Name::new(name.clone()), order));
            }
            BookmarkMutation::AddFolderIn { name, parent } => {
                let Some(parent_entity) = find_by_uuid(parent, &ids) else {
                    continue;
                };
                if folder_q.get(parent_entity).is_err() {
                    continue;
                }
                let order = next_top_order(orders.iter().map(|o| o.0));
                commands.spawn((
                    Folder,
                    new_uuid(),
                    Name::new(name.clone()),
                    order,
                    ChildOf(parent_entity),
                ));
            }
            BookmarkMutation::MoveFolder { uuid, parent } => {
                let Some(folder_entity) = find_by_uuid(uuid, &ids) else {
                    continue;
                };
                if folder_q.get(folder_entity).is_err() {
                    continue;
                }
                if let Some(parent_uuid) = parent {
                    let Some(parent_entity) = find_by_uuid(parent_uuid, &ids) else {
                        continue;
                    };
                    if folder_q.get(parent_entity).is_ok()
                        && can_parent_folder(folder_entity, parent_entity, &child_of_q)
                    {
                        commands
                            .entity(folder_entity)
                            .insert(ChildOf(parent_entity));
                    }
                } else {
                    commands.entity(folder_entity).remove::<ChildOf>();
                }
            }
            BookmarkMutation::RemoveFolder { uuid } => {
                if let Some(folder_entity) = find_by_uuid(uuid, &ids)
                    && folder_q.get(folder_entity).is_ok()
                {
                    let parent = child_of_q.get(folder_entity).ok().map(Relationship::get);
                    if let Ok(children) = children_q.get(folder_entity) {
                        for child in children.iter() {
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
            BookmarkMutation::RenameFolder { uuid, name } => {
                if let Some(folder_entity) = find_by_uuid(uuid, &ids)
                    && folder_q.get(folder_entity).is_ok()
                {
                    commands
                        .entity(folder_entity)
                        .insert(Name::new(name.clone()));
                }
            }
            BookmarkMutation::ToggleFolder { uuid } => {
                if let Some(folder_entity) = find_by_uuid(uuid, &ids)
                    && folder_q.get(folder_entity).is_ok()
                {
                    if collapsed_q.get(folder_entity).is_ok() {
                        commands.entity(folder_entity).remove::<Collapsed>();
                    } else {
                        commands.entity(folder_entity).insert(Collapsed);
                    }
                }
            }
            BookmarkMutation::Pin { uuid } => {
                if let Some(entity) = find_by_uuid(uuid, &ids)
                    && bookmarks.get(entity).is_ok()
                {
                    commands.entity(entity).insert(Pin);
                }
            }
            BookmarkMutation::PinUrl { metadata } => {
                if let Some((entity, _)) = pinned.iter().find(|(_, meta)| meta.url == metadata.url)
                {
                    commands.entity(entity).insert(metadata.clone());
                    continue;
                }
                if let Some((entity, _)) =
                    bookmarks.iter().find(|(_, meta)| meta.url == metadata.url)
                {
                    commands.entity(entity).insert((Pin, metadata.clone()));
                    continue;
                }
                let order = next_top_order(orders.iter().map(|o| o.0));
                commands.spawn((Pin, new_uuid(), metadata.clone(), order));
            }
            BookmarkMutation::Unpin { uuid } => {
                if let Some(entity) = find_by_uuid(uuid, &ids)
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
    _trigger: On<BinReceive<BookmarkToggleRequest>>,
    mut requests: MessageWriter<ToggleActiveRequest>,
) {
    requests.write(ToggleActiveRequest);
}

fn on_bookmark_open_request(
    trigger: On<BinReceive<BookmarkOpenRequest>>,
    mut requests: MessageWriter<StackRequest>,
) {
    requests.write(StackRequest::Open {
        url: Some(trigger.event().payload.url.clone()),
    });
}

fn on_bookmark_menu_request<R>(
    trigger: On<BinReceive<R>>,
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

fn on_bookmark_mutation_request<R>(
    trigger: On<BinReceive<R>>,
    mut ops: MessageWriter<BookmarkMutation>,
) where
    R: Clone + Send + Sync + 'static,
    BookmarkMutation: From<R>,
{
    ops.write(trigger.event().payload.clone().into());
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

impl From<BookmarkAddRequest> for BookmarkMutation {
    fn from(request: BookmarkAddRequest) -> Self {
        Self::Add {
            metadata: request.metadata,
            folder: request.folder,
        }
    }
}

impl From<BookmarkPinUrlRequest> for BookmarkMutation {
    fn from(request: BookmarkPinUrlRequest) -> Self {
        Self::PinUrl {
            metadata: request.metadata,
        }
    }
}

impl From<BookmarkRemoveRequest> for BookmarkMutation {
    fn from(request: BookmarkRemoveRequest) -> Self {
        Self::Remove { uuid: request.uuid }
    }
}

impl From<BookmarkRenameRequest> for BookmarkMutation {
    fn from(request: BookmarkRenameRequest) -> Self {
        Self::Rename {
            uuid: request.uuid,
            name: request.name,
        }
    }
}

impl From<BookmarkMoveRequest> for BookmarkMutation {
    fn from(request: BookmarkMoveRequest) -> Self {
        Self::Move {
            uuid: request.uuid,
            folder: request.folder,
        }
    }
}

impl From<BookmarkMovePinRequest> for BookmarkMutation {
    fn from(request: BookmarkMovePinRequest) -> Self {
        Self::MovePin {
            uuid: request.uuid,
            folder: request.folder,
        }
    }
}

impl From<BookmarkReorderPinRequest> for BookmarkMutation {
    fn from(request: BookmarkReorderPinRequest) -> Self {
        Self::ReorderPin {
            uuid: request.uuid,
            target_uuid: request.target_uuid,
        }
    }
}

impl From<BookmarkPinRequest> for BookmarkMutation {
    fn from(request: BookmarkPinRequest) -> Self {
        Self::Pin { uuid: request.uuid }
    }
}

impl From<BookmarkUnpinRequest> for BookmarkMutation {
    fn from(request: BookmarkUnpinRequest) -> Self {
        Self::Unpin { uuid: request.uuid }
    }
}

impl From<BookmarkFolderToggleRequest> for BookmarkMutation {
    fn from(request: BookmarkFolderToggleRequest) -> Self {
        Self::ToggleFolder { uuid: request.uuid }
    }
}

impl From<BookmarkFolderCreateRequest> for BookmarkMutation {
    fn from(request: BookmarkFolderCreateRequest) -> Self {
        match request.parent {
            Some(parent) => Self::AddFolderIn {
                name: request.name,
                parent,
            },
            None => Self::AddFolder { name: request.name },
        }
    }
}

impl From<BookmarkFolderMoveRequest> for BookmarkMutation {
    fn from(request: BookmarkFolderMoveRequest) -> Self {
        Self::MoveFolder {
            uuid: request.uuid,
            parent: request.parent,
        }
    }
}

impl From<BookmarkFolderRenameRequest> for BookmarkMutation {
    fn from(request: BookmarkFolderRenameRequest) -> Self {
        Self::RenameFolder {
            uuid: request.uuid,
            name: request.name,
        }
    }
}

impl From<BookmarkFolderRemoveRequest> for BookmarkMutation {
    fn from(request: BookmarkFolderRemoveRequest) -> Self {
        Self::RemoveFolder { uuid: request.uuid }
    }
}

fn handle_bookmark_requests(
    mut toggles: MessageReader<ToggleActiveRequest>,
    mut pins: MessageReader<PinActiveRequest>,
    mut create_folders: MessageReader<CreateFolderRequest>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_meta: Query<&PageMetadata, With<Stack>>,
    mut ops: MessageWriter<BookmarkMutation>,
) {
    for _ in create_folders.read() {
        ops.write(BookmarkMutation::AddFolder {
            name: "New Folder".to_string(),
        });
    }
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
        ops.write(BookmarkMutation::ToggleForUrl {
            metadata: meta.clone(),
        });
    }
    for _ in 0..pin_count {
        ops.write(BookmarkMutation::PinUrl {
            metadata: meta.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::PageIcon;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<BookmarkMutation>()
            .add_systems(
                Update,
                (apply_bookmark_mutations, sync_bookmark_metadata).chain(),
            );
        app
    }

    fn send(app: &mut App, op: BookmarkMutation) {
        app.world_mut()
            .resource_mut::<Messages<BookmarkMutation>>()
            .write(op);
        app.update();
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
            .add_message::<BookmarkMutation>()
            .add_message::<ShowBookmarkMenuRequest>()
            .add_message::<StackRequest>()
            .add_observer(on_bookmark_open_request);
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(BinReceive::<BookmarkOpenRequest> {
            webview,
            payload: BookmarkOpenRequest {
                url: "https://a.test".into(),
            },
        });
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<StackRequest>>()
            .drain()
            .collect();
        assert_eq!(
            requests,
            vec![StackRequest::Open {
                url: Some("https://a.test".into()),
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
            .trigger(BinReceive::<BookmarkTextInputRequest> {
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
            .trigger(BinReceive::<BookmarkTextInputRequest> {
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
            .trigger(BinReceive::<BookmarkContextMenuRequest> {
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
            .trigger(BinReceive::<BookmarkContextMenuRequest> {
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
        send(
            &mut app,
            BookmarkMutation::Add {
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
            send(
                &mut app,
                BookmarkMutation::Add {
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

        send(
            &mut app,
            BookmarkMutation::ReorderPin {
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
        send(
            &mut app,
            BookmarkMutation::Add {
                metadata: metadata("A"),
                folder: None,
            },
        );
        send(&mut app, BookmarkMutation::AddFolder { name: "PRs".into() });
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
        send(
            &mut app,
            BookmarkMutation::Add {
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
        let op = || BookmarkMutation::ToggleForUrl {
            metadata: metadata("A"),
        };
        send(&mut app, op());
        assert_eq!(count::<With<Bookmark>>(&mut app), 1);
        send(&mut app, op());
        assert_eq!(count::<With<Bookmark>>(&mut app), 0);
    }

    #[test]
    fn remove_despawns_by_uuid() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkMutation::Add {
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
        send(&mut app, BookmarkMutation::Remove { uuid });
        assert_eq!(count::<With<Bookmark>>(&mut app), 0);
    }

    #[test]
    fn remove_bookmark_keeps_pin() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkMutation::PinUrl {
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
        send(
            &mut app,
            BookmarkMutation::ToggleForUrl {
                metadata: metadata("A"),
            },
        );
        send(&mut app, BookmarkMutation::Remove { uuid });
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
        send(&mut app, BookmarkMutation::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkMutation::Add {
                metadata: metadata("A"),
                folder: Some(fid),
            },
        );
        assert_eq!(count::<(With<Bookmark>, With<ChildOf>)>(&mut app), 1);
    }

    #[test]
    fn folders_can_be_nested() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkMutation::AddFolder {
                name: "Work".into(),
            },
        );
        let (parent, parent_uuid) = folder_named(&mut app, "Work");
        send(
            &mut app,
            BookmarkMutation::AddFolderIn {
                name: "PRs".into(),
                parent: parent_uuid,
            },
        );
        let (child, _) = folder_named(&mut app, "PRs");
        assert_eq!(app.world().get::<ChildOf>(child).unwrap().get(), parent);
    }

    #[test]
    fn moving_folder_rejects_descendant_cycle() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkMutation::AddFolder {
                name: "Work".into(),
            },
        );
        let (parent, parent_uuid) = folder_named(&mut app, "Work");
        send(
            &mut app,
            BookmarkMutation::AddFolderIn {
                name: "PRs".into(),
                parent: parent_uuid.clone(),
            },
        );
        let (_, child_uuid) = folder_named(&mut app, "PRs");
        send(
            &mut app,
            BookmarkMutation::MoveFolder {
                uuid: parent_uuid,
                parent: Some(child_uuid),
            },
        );
        assert!(app.world().get::<ChildOf>(parent).is_none());
    }

    #[test]
    fn removing_nested_folder_reparents_children_to_parent() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkMutation::AddFolder {
                name: "Work".into(),
            },
        );
        let (parent, parent_uuid) = folder_named(&mut app, "Work");
        send(
            &mut app,
            BookmarkMutation::AddFolderIn {
                name: "PRs".into(),
                parent: parent_uuid,
            },
        );
        let (_, child_uuid) = folder_named(&mut app, "PRs");
        send(
            &mut app,
            BookmarkMutation::Add {
                metadata: metadata("A"),
                folder: Some(child_uuid.clone()),
            },
        );
        send(
            &mut app,
            BookmarkMutation::RemoveFolder { uuid: child_uuid },
        );
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
        send(
            &mut app,
            BookmarkMutation::Add {
                metadata: metadata("A"),
                folder: None,
            },
        );
        send(&mut app, BookmarkMutation::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkMutation::Add {
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
        send(&mut app, BookmarkMutation::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkMutation::Add {
                metadata: metadata("A"),
                folder: Some(fid),
            },
        );
        send(
            &mut app,
            BookmarkMutation::Add {
                metadata: metadata("A updated"),
                folder: None,
            },
        );
        assert_eq!(count::<(With<Bookmark>, With<ChildOf>)>(&mut app), 1);
    }

    #[test]
    fn rename_updates_bookmark_title() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkMutation::Add {
                metadata: metadata("A"),
                folder: None,
            },
        );
        let uuid = bookmark_uuid(&mut app);
        send(
            &mut app,
            BookmarkMutation::Rename {
                uuid,
                name: "Renamed".into(),
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
        send(&mut app, BookmarkMutation::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkMutation::Add {
                metadata: metadata("A"),
                folder: None,
            },
        );
        let uuid = bookmark_uuid(&mut app);
        send(
            &mut app,
            BookmarkMutation::Move {
                uuid: uuid.clone(),
                folder: Some(fid),
            },
        );
        assert_eq!(count::<(With<Bookmark>, With<ChildOf>)>(&mut app), 1);
        send(&mut app, BookmarkMutation::Move { uuid, folder: None });
        assert_eq!(count::<(With<Bookmark>, Without<ChildOf>)>(&mut app), 1);
    }

    #[test]
    fn remove_folder_reparents_children_to_top_level() {
        let mut app = test_app();
        send(&mut app, BookmarkMutation::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkMutation::Add {
                metadata: metadata("A"),
                folder: Some(fid.clone()),
            },
        );
        send(&mut app, BookmarkMutation::RemoveFolder { uuid: fid });
        assert_eq!(count::<With<Folder>>(&mut app), 0);
        assert_eq!(count::<(With<Bookmark>, Without<ChildOf>)>(&mut app), 1);
    }

    #[test]
    fn toggle_folder_adds_then_removes_collapsed() {
        let mut app = test_app();
        send(&mut app, BookmarkMutation::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkMutation::ToggleFolder { uuid: fid.clone() },
        );
        assert_eq!(count::<With<Collapsed>>(&mut app), 1);
        send(&mut app, BookmarkMutation::ToggleFolder { uuid: fid });
        assert_eq!(count::<With<Collapsed>>(&mut app), 0);
    }

    #[test]
    fn pin_keeps_bookmark_in_its_folder() {
        let mut app = test_app();
        send(&mut app, BookmarkMutation::AddFolder { name: "PRs".into() });
        let folder = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkMutation::Add {
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
        send(&mut app, BookmarkMutation::Pin { uuid: uuid.clone() });
        assert_eq!(count::<With<Pin>>(&mut app), 1);
        assert_eq!(
            count::<(With<Bookmark>, With<Pin>, With<ChildOf>)>(&mut app),
            1
        );
        send(&mut app, BookmarkMutation::Unpin { uuid });
        assert_eq!(
            count::<(With<Bookmark>, Without<Pin>, With<ChildOf>)>(&mut app),
            1
        );
    }

    #[test]
    fn pin_url_promotes_existing_bookmark_without_duplication() {
        let mut app = test_app();
        send(&mut app, BookmarkMutation::AddFolder { name: "PRs".into() });
        let folder = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkMutation::Add {
                metadata: metadata("A"),
                folder: Some(folder),
            },
        );
        send(
            &mut app,
            BookmarkMutation::PinUrl {
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
        send(
            &mut app,
            BookmarkMutation::PinUrl {
                metadata: metadata("A"),
            },
        );
        send(
            &mut app,
            BookmarkMutation::ToggleForUrl {
                metadata: metadata("A"),
            },
        );
        assert_eq!(count::<With<PageMetadata>>(&mut app), 1);
        assert_eq!(count::<With<Bookmark>>(&mut app), 1);
        assert_eq!(count::<With<Pin>>(&mut app), 1);
        send(
            &mut app,
            BookmarkMutation::ToggleForUrl {
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
        send(
            &mut app,
            BookmarkMutation::PinUrl {
                metadata: metadata("A"),
            },
        );
        send(&mut app, BookmarkMutation::AddFolder { name: "PRs".into() });
        let folder = folder_uuid(&mut app);
        let uuid = app
            .world_mut()
            .query_filtered::<&Uuid, With<Pin>>()
            .single(app.world())
            .unwrap()
            .0
            .clone();
        send(
            &mut app,
            BookmarkMutation::MovePin {
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
