use super::*;

#[test]
fn save_then_load_round_trips_bookmarks_and_excludes_save_entities() {
    let dir = std::env::temp_dir().join(format!("vmux-bm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bookmarks.ron");

    let mut save_app = App::new();
    save_app
        .add_plugins(MinimalPlugins)
        .add_plugins(bevy::asset::AssetPlugin::default())
        .add_plugins(bevy::scene::ScenePlugin)
        .add_plugins(vmux_core::CorePlugin)
        .add_observer(save_on::<SaveWorld<BookmarkFilter>>);
    save_app.world_mut().spawn((
        Folder,
        Uuid("f1".into()),
        Name::new("PRs"),
        BookmarkOrder(0),
    ));
    save_app.world_mut().spawn((
        Bookmark,
        Uuid("b1".into()),
        PageMetadata {
            title: "A".into(),
            url: "https://a.test".into(),
            icon: vmux_core::icon::PageIcon::default(),
            bg_color: None,
        },
        BookmarkOrder(1),
    ));
    save_app
        .world_mut()
        .spawn((Save, Name::new("excluded-save-entity")));
    let p = path.clone();
    save_app.add_systems(Update, move |mut c: Commands| {
        let mut s = SaveWorld::<BookmarkFilter>::into_file(p.clone());
        s.components = bookmark_scene_filter();
        c.trigger_save(s);
    });
    save_app.update();
    save_app.update();

    assert!(path.exists(), "bookmarks.ron written");
    let ron = std::fs::read_to_string(&path).unwrap();
    assert!(ron.contains("b1"), "bookmark uuid persisted");
    assert!(ron.contains("PRs"), "folder name persisted");

    let mut load_app = App::new();
    load_app
        .add_plugins(MinimalPlugins)
        .add_plugins(bevy::asset::AssetPlugin::default())
        .add_plugins(bevy::scene::ScenePlugin)
        .add_plugins(vmux_core::CorePlugin)
        .add_observer(load_on::<LoadWorld<BookmarkFilter>>);
    let p2 = path.clone();
    load_app.add_systems(Update, move |mut c: Commands| {
        c.trigger_load(LoadWorld::<BookmarkFilter>::from_file(p2.clone()));
    });
    load_app.update();
    load_app.update();

    let bookmarks = load_app
        .world_mut()
        .query_filtered::<Entity, With<Bookmark>>()
        .iter(load_app.world())
        .count();
    let folders = load_app
        .world_mut()
        .query_filtered::<Entity, With<Folder>>()
        .iter(load_app.world())
        .count();
    assert_eq!(bookmarks, 1, "bookmark rebuilt");
    assert_eq!(folders, 1, "folder rebuilt");
    let excluded = load_app
        .world_mut()
        .query::<&Name>()
        .iter(load_app.world())
        .any(|name| name.as_str() == "excluded-save-entity");
    assert!(!excluded, "Save-only entity excluded");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn legacy_bookmark_order_migration_removes_space_save_marker() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(vmux_core::CorePlugin)
        .add_systems(Update, migrate_legacy_bookmark_order);
    let entity = app
        .world_mut()
        .spawn((Bookmark, Uuid("b1".into()), Order(3)))
        .id();
    assert!(app.world().get::<Save>(entity).is_some());

    app.update();

    assert_eq!(
        app.world().get::<BookmarkOrder>(entity),
        Some(&BookmarkOrder(3))
    );
    assert!(app.world().get::<Order>(entity).is_none());
    assert!(app.world().get::<Save>(entity).is_none());
}
