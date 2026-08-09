use super::*;
use vmux_core::PageOpenId;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<vmux_core::event::RecordVisitRequest>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_file_page_open);
    app
}

#[test]
fn file_open_records_history_visit() {
    use bevy::ecs::message::Messages;
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    app.world_mut().spawn(PageOpenTask {
        id: PageOpenId::new(),
        stack,
        url: "file:///etc/hostname#L3".to_string(),
        request_id: None,
    });
    app.update();
    let msgs = app
        .world()
        .resource::<Messages<vmux_core::event::RecordVisitRequest>>();
    let mut cursor = msgs.get_cursor();
    let recorded: Vec<_> = cursor.read(msgs).collect();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].url, "file:///etc/hostname");
    assert_eq!(recorded[0].title, "hostname");
}

#[test]
fn claims_files_url_and_attaches_fileview() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    let task = app
        .world_mut()
        .spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "file:///etc/hostname".to_string(),
            request_id: None,
        })
        .id();
    app.update();
    assert!(app.world().get::<PageOpenHandled>(task).is_some());
    let mut q = app.world_mut().query::<(&ChildOf, &FileView)>();
    let found: Vec<_> = q
        .iter(app.world())
        .filter(|(c, _)| c.0 == stack)
        .map(|(_, fv)| fv.path.clone())
        .collect();
    assert_eq!(found, vec![PathBuf::from("/etc/hostname")]);
}

#[test]
fn ignores_non_files_url() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    let task = app
        .world_mut()
        .spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://terminal/".to_string(),
            request_id: None,
        })
        .id();
    app.update();
    assert!(app.world().get::<PageOpenHandled>(task).is_none());
}

#[test]
fn navigate_relists_when_path_changes() {
    use std::fs;
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a");
    fs::create_dir(&a).unwrap();
    fs::write(a.join("f1"), "").unwrap();
    let b = tmp.path().join("b");
    fs::create_dir(&b).unwrap();
    fs::write(b.join("f2"), "").unwrap();

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, load_file_buffers);
    let e = app
        .world_mut()
        .spawn((
            FileView { path: a.clone() },
            FileViewport {
                top_row: 0,
                rows: 0,
                wrap_columns: 0,
                word_wrap: vmux_core::editor::WordWrap::default(),
                word_wrap_column: 80,
            },
        ))
        .id();
    app.update();
    assert!(
        app.world()
            .get::<FileDir>(e)
            .unwrap()
            .entries
            .iter()
            .any(|x| x.name == "f1")
    );

    app.world_mut().get_mut::<FileView>(e).unwrap().path = b.clone();
    app.world_mut().entity_mut(e).remove::<FileDir>();
    app.update();
    let dir = app.world().get::<FileDir>(e).unwrap();
    assert!(dir.entries.iter().any(|x| x.name == "f2"));
    assert!(!dir.entries.iter().any(|x| x.name == "f1"));
}
