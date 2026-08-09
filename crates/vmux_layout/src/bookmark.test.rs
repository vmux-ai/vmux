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
                metadata: None,
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
fn context_menu_event_toggles_layout_pointer_marker() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_observer(on_bookmark_context_menu_emit);
    let webview = app.world_mut().spawn_empty().id();
    app.world_mut()
        .trigger(BinReceive::<BookmarkContextMenuEvent> {
            webview,
            payload: BookmarkContextMenuEvent { active: true },
        });
    app.update();
    assert!(
        app.world()
            .entity(webview)
            .contains::<BookmarkContextMenuActive>()
    );
    app.world_mut()
        .trigger(BinReceive::<BookmarkContextMenuEvent> {
            webview,
            payload: BookmarkContextMenuEvent { active: false },
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
        BookmarkOp::Add {
            metadata: metadata("A"),
            folder: None,
        },
    );
    assert_eq!(count::<With<Bookmark>>(&mut app), 1);
}

#[test]
fn bookmark_entities_are_not_space_save_entities() {
    let mut app = test_app();
    send(
        &mut app,
        BookmarkOp::Add {
            metadata: metadata("A"),
            folder: None,
        },
    );
    send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
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
        BookmarkOp::Add {
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
fn toggle_for_url_is_idempotent_add_then_remove() {
    let mut app = test_app();
    let op = || BookmarkOp::ToggleForUrl {
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
        BookmarkOp::Add {
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
    send(&mut app, BookmarkOp::Remove { uuid });
    assert_eq!(count::<With<Bookmark>>(&mut app), 0);
}

#[test]
fn remove_bookmark_keeps_pin() {
    let mut app = test_app();
    send(
        &mut app,
        BookmarkOp::PinUrl {
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
        BookmarkOp::ToggleForUrl {
            metadata: metadata("A"),
        },
    );
    send(&mut app, BookmarkOp::Remove { uuid });
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
    send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
    let fid = folder_uuid(&mut app);
    send(
        &mut app,
        BookmarkOp::Add {
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
        BookmarkOp::AddFolder {
            name: "Work".into(),
        },
    );
    let (parent, parent_uuid) = folder_named(&mut app, "Work");
    send(
        &mut app,
        BookmarkOp::AddFolderIn {
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
        BookmarkOp::AddFolder {
            name: "Work".into(),
        },
    );
    let (parent, parent_uuid) = folder_named(&mut app, "Work");
    send(
        &mut app,
        BookmarkOp::AddFolderIn {
            name: "PRs".into(),
            parent: parent_uuid.clone(),
        },
    );
    let (_, child_uuid) = folder_named(&mut app, "PRs");
    send(
        &mut app,
        BookmarkOp::MoveFolder {
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
        BookmarkOp::AddFolder {
            name: "Work".into(),
        },
    );
    let (parent, parent_uuid) = folder_named(&mut app, "Work");
    send(
        &mut app,
        BookmarkOp::AddFolderIn {
            name: "PRs".into(),
            parent: parent_uuid,
        },
    );
    let (_, child_uuid) = folder_named(&mut app, "PRs");
    send(
        &mut app,
        BookmarkOp::Add {
            metadata: metadata("A"),
            folder: Some(child_uuid.clone()),
        },
    );
    send(&mut app, BookmarkOp::RemoveFolder { uuid: child_uuid });
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
        BookmarkOp::Add {
            metadata: metadata("A"),
            folder: None,
        },
    );
    send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
    let fid = folder_uuid(&mut app);
    send(
        &mut app,
        BookmarkOp::Add {
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
    send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
    let fid = folder_uuid(&mut app);
    send(
        &mut app,
        BookmarkOp::Add {
            metadata: metadata("A"),
            folder: Some(fid),
        },
    );
    send(
        &mut app,
        BookmarkOp::Add {
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
        BookmarkOp::Add {
            metadata: metadata("A"),
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
            metadata: metadata("A"),
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
            metadata: metadata("A"),
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
fn pin_keeps_bookmark_in_its_folder() {
    let mut app = test_app();
    send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
    let folder = folder_uuid(&mut app);
    send(
        &mut app,
        BookmarkOp::Add {
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
    send(&mut app, BookmarkOp::Pin { uuid: uuid.clone() });
    assert_eq!(count::<With<Pin>>(&mut app), 1);
    assert_eq!(
        count::<(With<Bookmark>, With<Pin>, With<ChildOf>)>(&mut app),
        1
    );
    send(&mut app, BookmarkOp::Unpin { uuid });
    assert_eq!(
        count::<(With<Bookmark>, Without<Pin>, With<ChildOf>)>(&mut app),
        1
    );
}

#[test]
fn pin_url_promotes_existing_bookmark_without_duplication() {
    let mut app = test_app();
    send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
    let folder = folder_uuid(&mut app);
    send(
        &mut app,
        BookmarkOp::Add {
            metadata: metadata("A"),
            folder: Some(folder),
        },
    );
    send(
        &mut app,
        BookmarkOp::PinUrl {
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
        BookmarkOp::PinUrl {
            metadata: metadata("A"),
        },
    );
    send(
        &mut app,
        BookmarkOp::ToggleForUrl {
            metadata: metadata("A"),
        },
    );
    assert_eq!(count::<With<PageMetadata>>(&mut app), 1);
    assert_eq!(count::<With<Bookmark>>(&mut app), 1);
    assert_eq!(count::<With<Pin>>(&mut app), 1);
    send(
        &mut app,
        BookmarkOp::ToggleForUrl {
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
        BookmarkOp::PinUrl {
            metadata: metadata("A"),
        },
    );
    send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
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
        BookmarkOp::MovePin {
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
