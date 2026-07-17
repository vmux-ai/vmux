use crate::event::{BookmarkTextInputEvent, BookmarksCommandEvent};
use crate::pane::{Pane, PaneSplit};
use crate::stack::{ActiveTabParam, Stack, focused_stack};
use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};
use vmux_command::{AppCommand, BookmarkCommand, BrowserCommand, OpenCommand, ReadAppCommands};
use vmux_core::{Bookmark, Collapsed, Folder, LastActivatedAt, Order, PageMetadata, Pin, Uuid};

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub enum BookmarkOp {
    ToggleForUrl {
        url: String,
        title: String,
        favicon_url: String,
    },
    Add {
        url: String,
        title: String,
        favicon_url: String,
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
    AddFolder {
        name: String,
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
        url: String,
        title: String,
        favicon_url: String,
    },
    Unpin {
        uuid: String,
    },
}

#[derive(Message, Clone, Debug, Default)]
pub struct ShowBookmarkMenuRequest;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BookmarkTextInputActive;

pub struct BookmarkPlugin;

impl Plugin for BookmarkPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BookmarkOp>()
            .add_message::<ShowBookmarkMenuRequest>()
            .add_plugins(BinEventEmitterPlugin::<(
                BookmarksCommandEvent,
                BookmarkTextInputEvent,
            )>::for_hosts(&["layout"]))
            .add_observer(on_bookmarks_command_emit)
            .add_observer(on_bookmark_text_input_emit)
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

fn next_top_order(orders: impl Iterator<Item = u32>) -> Order {
    Order(orders.max().map(|m| m + 1).unwrap_or(0))
}

fn page_metadata(title: &str, url: &str, favicon_url: &str) -> PageMetadata {
    PageMetadata {
        title: title.to_string(),
        url: url.to_string(),
        icon: vmux_core::icon::PageIcon::favicon(favicon_url.to_string()),
        bg_color: None,
    }
}

fn apply_bookmark_ops(
    mut reader: MessageReader<BookmarkOp>,
    ids: Query<(Entity, &Uuid)>,
    bookmarks: Query<(Entity, &PageMetadata), With<Bookmark>>,
    pinned: Query<(Entity, &PageMetadata), With<Pin>>,
    folder_q: Query<(), With<Folder>>,
    collapsed_q: Query<(), With<Collapsed>>,
    orders: Query<&Order>,
    children_q: Query<&Children>,
    mut commands: Commands,
) {
    for op in reader.read() {
        match op {
            BookmarkOp::ToggleForUrl {
                url,
                title,
                favicon_url,
            } => {
                let existing = bookmarks
                    .iter()
                    .find(|(_, meta)| &meta.url == url)
                    .map(|(entity, _)| entity);
                if let Some(entity) = existing {
                    if pinned.get(entity).is_ok() {
                        commands.entity(entity).remove::<Bookmark>();
                    } else {
                        commands.entity(entity).despawn();
                    }
                } else {
                    let order = next_top_order(orders.iter().map(|o| o.0));
                    commands.spawn((
                        Bookmark,
                        new_uuid(),
                        page_metadata(title, url, favicon_url),
                        order,
                    ));
                }
            }
            BookmarkOp::Add {
                url,
                title,
                favicon_url,
                folder,
            } => {
                let folder_entity = folder.as_ref().and_then(|folder_uuid| {
                    let entity = find_by_uuid(folder_uuid, &ids)?;
                    folder_q.get(entity).ok().map(|_| entity)
                });
                if folder.is_some() && folder_entity.is_none() {
                    continue;
                }
                if let Some((entity, _)) = bookmarks.iter().find(|(_, meta)| &meta.url == url) {
                    let mut entity_commands = commands.entity(entity);
                    entity_commands.insert(page_metadata(title, url, favicon_url));
                    if let Some(folder_entity) = folder_entity {
                        entity_commands
                            .insert(ChildOf(folder_entity))
                            .remove::<Pin>();
                    }
                    continue;
                }
                if let Some((entity, _)) = pinned.iter().find(|(_, meta)| &meta.url == url) {
                    let mut entity_commands = commands.entity(entity);
                    entity_commands.insert((Bookmark, page_metadata(title, url, favicon_url)));
                    if let Some(folder_entity) = folder_entity {
                        entity_commands
                            .insert(ChildOf(folder_entity))
                            .remove::<Pin>();
                    }
                    continue;
                }
                let order = next_top_order(orders.iter().map(|o| o.0));
                let mut e = commands.spawn((
                    Bookmark,
                    new_uuid(),
                    page_metadata(title, url, favicon_url),
                    order,
                ));
                if let Some(folder_entity) = folder_entity {
                    e.insert(ChildOf(folder_entity));
                }
            }
            BookmarkOp::Remove { uuid } => {
                if let Some(entity) = find_by_uuid(uuid, &ids)
                    && (bookmarks.get(entity).is_ok() || pinned.get(entity).is_ok())
                {
                    commands.entity(entity).despawn();
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
                    && pinned.get(entity).is_err()
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
            BookmarkOp::AddFolder { name } => {
                let order = next_top_order(orders.iter().map(|o| o.0));
                commands.spawn((Folder, new_uuid(), Name::new(name.clone()), order));
            }
            BookmarkOp::RemoveFolder { uuid } => {
                if let Some(folder_entity) = find_by_uuid(uuid, &ids)
                    && folder_q.get(folder_entity).is_ok()
                {
                    if let Ok(children) = children_q.get(folder_entity) {
                        for child in children.iter() {
                            commands.entity(child).remove::<ChildOf>();
                        }
                    }
                    commands.entity(folder_entity).despawn();
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
                    commands.entity(entity).insert(Pin).remove::<ChildOf>();
                }
            }
            BookmarkOp::PinUrl {
                url,
                title,
                favicon_url,
            } => {
                if let Some((entity, _)) = pinned.iter().find(|(_, meta)| &meta.url == url) {
                    commands
                        .entity(entity)
                        .insert(page_metadata(title, url, favicon_url))
                        .remove::<ChildOf>();
                    continue;
                }
                if let Some((entity, _)) = bookmarks.iter().find(|(_, meta)| &meta.url == url) {
                    commands
                        .entity(entity)
                        .insert((Pin, page_metadata(title, url, favicon_url)))
                        .remove::<ChildOf>();
                    continue;
                }
                let order = next_top_order(orders.iter().map(|o| o.0));
                commands.spawn((
                    Pin,
                    new_uuid(),
                    page_metadata(title, url, favicon_url),
                    order,
                ));
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
            if let Some(url) = e.url.clone() {
                ops.write(BookmarkOp::Add {
                    url,
                    title: e.title.clone().unwrap_or_default(),
                    favicon_url: e.favicon_url.clone().unwrap_or_default(),
                    folder: e.folder.clone(),
                });
            }
        }
        "pin_url" => {
            if let Some(url) = e.url.clone() {
                ops.write(BookmarkOp::PinUrl {
                    url,
                    title: e.title.clone().unwrap_or_default(),
                    favicon_url: e.favicon_url.clone().unwrap_or_default(),
                });
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
                ops.write(BookmarkOp::AddFolder { name });
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
        let (url, title, favicon_url) = (
            meta.url.clone(),
            meta.title.clone(),
            meta.icon.favicon_url().to_string(),
        );
        if pin {
            ops.write(BookmarkOp::PinUrl {
                url,
                title,
                favicon_url,
            });
        } else {
            ops.write(BookmarkOp::ToggleForUrl {
                url,
                title,
                favicon_url,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<BookmarkOp>()
            .add_systems(Update, apply_bookmark_ops);
        app
    }

    fn send(app: &mut App, op: BookmarkOp) {
        app.world_mut()
            .resource_mut::<Messages<BookmarkOp>>()
            .write(op);
        app.update();
    }

    fn count<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, F>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn open_event_requests_new_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<BookmarkOp>()
            .add_message::<ShowBookmarkMenuRequest>()
            .add_message::<AppCommand>()
            .add_observer(on_bookmarks_command_emit);
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut()
            .trigger(BinReceive::<BookmarksCommandEvent> {
                webview,
                payload: BookmarksCommandEvent {
                    command: "open".into(),
                    uuid: None,
                    name: None,
                    url: Some("https://a.test".into()),
                    title: None,
                    favicon_url: None,
                    folder: None,
                },
            });
        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();
        assert_eq!(
            commands,
            vec![AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewStack {
                    url: Some("https://a.test".into()),
                }
            ))]
        );
    }

    #[test]
    fn text_input_event_toggles_layout_keyboard_marker() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_observer(on_bookmark_text_input_emit);
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut()
            .trigger(BinReceive::<BookmarkTextInputEvent> {
                webview,
                payload: BookmarkTextInputEvent { active: true },
            });
        app.update();
        assert!(
            app.world()
                .entity(webview)
                .contains::<BookmarkTextInputActive>()
        );
        app.world_mut()
            .trigger(BinReceive::<BookmarkTextInputEvent> {
                webview,
                payload: BookmarkTextInputEvent { active: false },
            });
        app.update();
        assert!(
            !app.world()
                .entity(webview)
                .contains::<BookmarkTextInputActive>()
        );
    }

    #[test]
    fn add_creates_bookmark_entity() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: None,
            },
        );
        assert_eq!(count::<With<Bookmark>>(&mut app), 1);
    }

    #[test]
    fn toggle_for_url_is_idempotent_add_then_remove() {
        let mut app = test_app();
        let op = || BookmarkOp::ToggleForUrl {
            url: "https://a.test".into(),
            title: "A".into(),
            favicon_url: String::new(),
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
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
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
        send(&mut app, BookmarkOp::Remove { uuid });
        assert_eq!(count::<With<Bookmark>>(&mut app), 0);
    }

    fn folder_uuid(app: &mut App) -> String {
        app.world_mut()
            .query_filtered::<&Uuid, With<Folder>>()
            .single(app.world())
            .unwrap()
            .0
            .clone()
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
        send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: Some(fid),
            },
        );
        assert_eq!(count::<(With<Bookmark>, With<ChildOf>)>(&mut app), 1);
    }

    #[test]
    fn add_existing_bookmark_moves_it_into_folder() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: None,
            },
        );
        send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A updated".into(),
                favicon_url: String::new(),
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
        send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: Some(fid),
            },
        );
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A updated".into(),
                favicon_url: String::new(),
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
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: None,
            },
        );
        let uuid = bookmark_uuid(&mut app);
        send(
            &mut app,
            BookmarkOp::Rename {
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
        send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: None,
            },
        );
        let uuid = bookmark_uuid(&mut app);
        send(
            &mut app,
            BookmarkOp::Move {
                uuid: uuid.clone(),
                folder: Some(fid),
            },
        );
        assert_eq!(count::<(With<Bookmark>, With<ChildOf>)>(&mut app), 1);
        send(&mut app, BookmarkOp::Move { uuid, folder: None });
        assert_eq!(count::<(With<Bookmark>, Without<ChildOf>)>(&mut app), 1);
    }

    #[test]
    fn remove_folder_reparents_children_to_top_level() {
        let mut app = test_app();
        send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: Some(fid.clone()),
            },
        );
        send(&mut app, BookmarkOp::RemoveFolder { uuid: fid });
        assert_eq!(count::<With<Folder>>(&mut app), 0);
        assert_eq!(count::<(With<Bookmark>, Without<ChildOf>)>(&mut app), 1);
    }

    #[test]
    fn toggle_folder_adds_then_removes_collapsed() {
        let mut app = test_app();
        send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(&mut app, BookmarkOp::ToggleFolder { uuid: fid.clone() });
        assert_eq!(count::<With<Collapsed>>(&mut app), 1);
        send(&mut app, BookmarkOp::ToggleFolder { uuid: fid });
        assert_eq!(count::<With<Collapsed>>(&mut app), 0);
    }

    #[test]
    fn pin_promotes_bookmark_out_of_list_into_grid() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
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
        send(&mut app, BookmarkOp::Pin { uuid: uuid.clone() });
        assert_eq!(count::<With<Pin>>(&mut app), 1);
        assert_eq!(count::<(With<Bookmark>, Without<Pin>)>(&mut app), 0);
        send(&mut app, BookmarkOp::Unpin { uuid });
        assert_eq!(count::<(With<Bookmark>, Without<Pin>)>(&mut app), 1);
    }

    #[test]
    fn pin_url_promotes_existing_bookmark_without_duplication() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: None,
            },
        );
        send(
            &mut app,
            BookmarkOp::PinUrl {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
            },
        );
        assert_eq!(count::<With<Bookmark>>(&mut app), 1);
        assert_eq!(count::<With<Pin>>(&mut app), 1);
    }
}
