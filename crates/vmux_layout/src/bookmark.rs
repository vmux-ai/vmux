use crate::event::{BookmarkContextMenuEvent, BookmarkTextInputEvent, BookmarksCommandEvent};
use crate::pane::{Pane, PaneSplit};
use crate::stack::{ActiveTabParam, Stack, focused_stack};
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};
use vmux_command::{AppCommand, BookmarkCommand, BrowserCommand, OpenCommand, ReadAppCommands};
use vmux_core::{
    Bookmark, BookmarkOrder, Collapsed, Folder, LastActivatedAt, PageMetadata, Pin, Uuid,
};

pub struct BookmarkPlugin;

impl Plugin for BookmarkPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BookmarkOp>()
            .add_message::<ShowBookmarkMenuRequest>()
            .add_plugins(BinEventEmitterPlugin::<(
                BookmarksCommandEvent,
                BookmarkTextInputEvent,
                BookmarkContextMenuEvent,
            )>::for_hosts(&["layout"]))
            .add_observer(on_bookmarks_command_emit)
            .add_observer(on_bookmark_text_input_emit)
            .add_observer(on_bookmark_context_menu_emit)
            .add_systems(
                Update,
                (
                    handle_bookmark_app_commands.in_set(ReadAppCommands),
                    apply_bookmark_ops,
                )
                    .chain(),
            );
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub enum BookmarkOp {
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

#[derive(Message, Clone, Debug, Default)]
pub struct ShowBookmarkMenuRequest;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BookmarkTextInputActive;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BookmarkContextMenuActive;

fn on_bookmark_context_menu_emit(
    trigger: On<BinReceive<BookmarkContextMenuEvent>>,
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

fn on_bookmark_text_input_emit(
    trigger: On<BinReceive<BookmarkTextInputEvent>>,
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

fn apply_bookmark_ops(
    mut reader: MessageReader<BookmarkOp>,
    ids: Query<(Entity, &Uuid)>,
    bookmarks: Query<(Entity, &PageMetadata), With<Bookmark>>,
    pinned: Query<(Entity, &PageMetadata), With<Pin>>,
    folder_q: Query<(), With<Folder>>,
    collapsed_q: Query<(), With<Collapsed>>,
    orders: Query<&BookmarkOrder>,
    children_q: Query<&Children>,
    child_of_q: Query<&ChildOf>,
    mut commands: Commands,
) {
    for op in reader.read() {
        match op {
            BookmarkOp::ToggleForUrl { metadata } => {
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
            BookmarkOp::Add { metadata, folder } => {
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
            BookmarkOp::Remove { uuid } => {
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
            BookmarkOp::Rename { uuid, name } => {
                if let Some(entity) = find_by_uuid(uuid, &ids)
                    && let Ok((_, metadata)) = bookmarks.get(entity)
                {
                    let mut metadata = metadata.clone();
                    metadata.title = name.clone();
                    commands.entity(entity).insert(metadata);
                }
            }
            BookmarkOp::Move { uuid, folder } => {
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
            BookmarkOp::MovePin { uuid, folder } => {
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
            BookmarkOp::AddFolder { name } => {
                let order = next_top_order(orders.iter().map(|o| o.0));
                commands.spawn((Folder, new_uuid(), Name::new(name.clone()), order));
            }
            BookmarkOp::AddFolderIn { name, parent } => {
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
            BookmarkOp::MoveFolder { uuid, parent } => {
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
            BookmarkOp::RemoveFolder { uuid } => {
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
            BookmarkOp::RenameFolder { uuid, name } => {
                if let Some(folder_entity) = find_by_uuid(uuid, &ids)
                    && folder_q.get(folder_entity).is_ok()
                {
                    commands
                        .entity(folder_entity)
                        .insert(Name::new(name.clone()));
                }
            }
            BookmarkOp::ToggleFolder { uuid } => {
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
            BookmarkOp::Pin { uuid } => {
                if let Some(entity) = find_by_uuid(uuid, &ids)
                    && bookmarks.get(entity).is_ok()
                {
                    commands.entity(entity).insert(Pin);
                }
            }
            BookmarkOp::PinUrl { metadata } => {
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
            BookmarkOp::Unpin { uuid } => {
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

fn on_bookmarks_command_emit(
    trigger: On<BinReceive<BookmarksCommandEvent>>,
    mut ops: MessageWriter<BookmarkOp>,
    mut app_cmds: MessageWriter<AppCommand>,
    mut menu_req: MessageWriter<ShowBookmarkMenuRequest>,
) {
    let e = &trigger.event().payload;
    match e.command.as_str() {
        "toggle_active" => {
            app_cmds.write(AppCommand::Bookmark(BookmarkCommand::ToggleActive));
        }
        "menu_new_folder" => {
            menu_req.write(ShowBookmarkMenuRequest);
        }
        "open" => {
            if let Some(url) = e.url.clone() {
                app_cmds.write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InNewStack { url: Some(url) },
                )));
            }
        }
        "add" => {
            if let Some(metadata) = e.metadata.clone() {
                ops.write(BookmarkOp::Add {
                    metadata,
                    folder: e.folder.clone(),
                });
            }
        }
        "pin_url" => {
            if let Some(metadata) = e.metadata.clone() {
                ops.write(BookmarkOp::PinUrl { metadata });
            }
        }
        "remove" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::Remove { uuid });
            }
        }
        "rename" => {
            if let (Some(uuid), Some(name)) = (e.uuid.clone(), e.name.clone()) {
                ops.write(BookmarkOp::Rename { uuid, name });
            }
        }
        "move" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::Move {
                    uuid,
                    folder: e.folder.clone(),
                });
            }
        }
        "move_pin" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::MovePin {
                    uuid,
                    folder: e.folder.clone(),
                });
            }
        }
        "pin" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::Pin { uuid });
            }
        }
        "unpin" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::Unpin { uuid });
            }
        }
        "toggle_folder" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::ToggleFolder { uuid });
            }
        }
        "new_folder" => {
            if let Some(name) = e.name.clone() {
                if let Some(parent) = e.folder.clone() {
                    ops.write(BookmarkOp::AddFolderIn { name, parent });
                } else {
                    ops.write(BookmarkOp::AddFolder { name });
                }
            }
        }
        "move_folder" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::MoveFolder {
                    uuid,
                    parent: e.folder.clone(),
                });
            }
        }
        "rename_folder" => {
            if let (Some(uuid), Some(name)) = (e.uuid.clone(), e.name.clone()) {
                ops.write(BookmarkOp::RenameFolder { uuid, name });
            }
        }
        "remove_folder" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::RemoveFolder { uuid });
            }
        }
        _ => {}
    }
}

fn handle_bookmark_app_commands(
    mut reader: MessageReader<AppCommand>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_meta: Query<&PageMetadata, With<Stack>>,
    mut ops: MessageWriter<BookmarkOp>,
) {
    for cmd in reader.read() {
        let pin = match cmd {
            AppCommand::Bookmark(BookmarkCommand::ToggleActive) => false,
            AppCommand::Bookmark(BookmarkCommand::PinActive) => true,
            AppCommand::Bookmark(BookmarkCommand::NewFolder) => {
                ops.write(BookmarkOp::AddFolder {
                    name: "New Folder".to_string(),
                });
                continue;
            }
            _ => continue,
        };
        let (_, _, stack) = focused_stack(
            active_tab_param.get(),
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );
        let Some(stack) = stack else { continue };
        let Ok(meta) = stack_meta.get(stack) else {
            continue;
        };
        if meta.url.is_empty() {
            continue;
        }
        if pin {
            ops.write(BookmarkOp::PinUrl {
                metadata: meta.clone(),
            });
        } else {
            ops.write(BookmarkOp::ToggleForUrl {
                metadata: meta.clone(),
            });
        }
    }
}

#[cfg(test)]
#[path = "bookmark.test.rs"]
mod tests;
