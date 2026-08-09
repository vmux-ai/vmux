use super::*;
use crate::keymap::{KeyInput, KeymapKindExt, Mods};

#[test]
fn file_view_mode_is_shared_across_editors() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SharedFileViewMode>()
        .add_observer(on_file_view_mode_set);
    let first = app
        .world_mut()
        .spawn(FileView {
            path: PathBuf::from("/a.rs"),
        })
        .id();
    let second = app
        .world_mut()
        .spawn(FileView {
            path: PathBuf::from("/b.rs"),
        })
        .id();

    app.world_mut().trigger(BinReceive {
        webview: first,
        payload: FileViewModeSet {
            mode: FileViewMode::Diff,
        },
    });

    assert_eq!(
        app.world().resource::<SharedFileViewMode>().0,
        FileViewMode::Diff
    );
    assert!(app.world().get::<FileView>(second).is_some());
}

#[test]
fn switching_to_note_reveals_the_current_cursor_line() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SharedFileViewMode>()
        .add_observer(on_file_view_mode_set);
    app.world_mut().resource_mut::<SharedFileViewMode>().0 = FileViewMode::Editor;

    let path = PathBuf::from("/note.md");
    let mut core = EditCore::new(
        path.clone(),
        "Markdown".into(),
        "one\ntwo\nthree\n",
        crate::edit::EditMode::Normal,
    );
    core.apply(EditCommand::Move(Motion::GotoLine(2)));
    let entity = app
        .world_mut()
        .spawn((
            FileView { path: path.clone() },
            EditState::new(
                core,
                HighlightCache::new(&path),
                crate::fold::FoldState::default(),
            ),
        ))
        .id();

    app.world_mut().trigger(BinReceive {
        webview: entity,
        payload: FileViewModeSet {
            mode: FileViewMode::Note,
        },
    });
    app.update();

    assert_eq!(
        app.world().get::<NoteRevealLine>(entity).map(|line| line.0),
        Some(2)
    );
}

#[test]
fn missing_file_view_loads_when_file_is_created() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("created-after-open");
    let path = parent.join("file.txt");
    let (tx, rx) = mpsc::channel();
    let watcher = notify::recommended_watcher(|_| {}).unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_systems(
        Update,
        (
            reconcile_file_watches,
            drain_file_changes,
            reload_changed_files,
            load_file_buffers,
        )
            .chain(),
    );
    app.world_mut().insert_non_send(FileWatch {
        watcher,
        rx,
        dirs: HashSet::new(),
    });
    app.world_mut().insert_non_send(SelfWrites::default());
    app.world_mut().insert_non_send(Browsers::default());
    app.world_mut()
        .insert_non_send(crate::lsp::manager::LspManager::default());
    let entity = app
        .world_mut()
        .spawn((
            FileView { path: path.clone() },
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
            .get::<FileBuffer>(entity)
            .unwrap()
            .language
            .starts_with("__error__:cannot open")
    );

    std::fs::create_dir(&parent).unwrap();
    std::fs::write(&path, "created\n").unwrap();
    tx.send(Ok(
        notify::Event::new(notify::EventKind::Any).add_path(parent)
    ))
    .unwrap();
    app.update();

    assert_eq!(
        app.world()
            .get::<EditState>(entity)
            .unwrap()
            .core
            .buffer
            .text(),
        "created\n"
    );
}

#[test]
fn file_view_mode_request_updates_shared_mode() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SharedFileViewMode>()
        .add_message::<FileViewModeRequest>()
        .add_systems(Update, apply_file_view_mode_requests);

    app.world_mut()
        .resource_mut::<Messages<FileViewModeRequest>>()
        .write(FileViewModeRequest(FileViewMode::Diff));
    app.update();

    assert_eq!(
        app.world().resource::<SharedFileViewMode>().0,
        FileViewMode::Diff
    );
}

#[test]
fn non_editor_cannot_change_file_view_mode() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SharedFileViewMode>()
        .add_observer(on_file_view_mode_set);
    let other = app.world_mut().spawn_empty().id();

    app.world_mut().trigger(BinReceive {
        webview: other,
        payload: FileViewModeSet {
            mode: FileViewMode::Diff,
        },
    });

    assert_eq!(
        app.world().resource::<SharedFileViewMode>().0,
        FileViewMode::Note
    );
}

#[test]
fn file_view_mode_defaults_to_note() {
    assert_eq!(SharedFileViewMode::default().0, FileViewMode::Note);
}

#[test]
fn parse_goto_fragment_line_and_select() {
    let g = parse_goto_fragment("file:///a/b.rs#L10").unwrap();
    assert_eq!((g.line, g.utf16_col, g.select_end_col), (9, 0, None));
    let g = parse_goto_fragment("file:///a/b.rs#L10:5-12").unwrap();
    assert_eq!((g.line, g.utf16_col, g.select_end_col), (9, 5, Some(12)));
    assert!(parse_goto_fragment("file:///a/b.rs").is_none());
    assert!(parse_goto_fragment("file:///a/b.rs#x").is_none());
}

#[test]
fn vim_dd_deletes_line_via_keymap_and_core() {
    let mut km = vmux_core::KeymapKind::Vim.make(&[], " ");
    let mut core = EditCore::new(
        std::path::PathBuf::from("a.txt"),
        "Plain Text".into(),
        "one\ntwo\nthree\n",
        crate::edit::EditMode::Normal,
    );
    for key in ["d", "d"] {
        for cmd in km.handle(&KeyInput {
            key: key.into(),
            mods: Mods::default(),
            repeat: false,
        }) {
            core.apply(cmd);
        }
    }
    assert_eq!(core.buffer.text(), "two\nthree\n");
}

#[test]
fn vscode_typing_inserts_and_marks_dirty() {
    let mut core = EditCore::new(
        std::path::PathBuf::from("a.txt"),
        "Plain Text".into(),
        "",
        crate::edit::EditMode::Insert,
    );
    core.apply(EditCommand::InsertText("hello".into()));
    assert_eq!(core.buffer.text(), "hello");
    assert!(core.dirty);
}

#[test]
fn repeated_navigation_advances_two_steps_without_accelerating_edits() {
    assert_eq!(
        accelerate_repeated_navigation(vec![EditCommand::Move(Motion::Down)], true),
        [
            EditCommand::Move(Motion::Down),
            EditCommand::Move(Motion::Down)
        ]
    );
    assert_eq!(
        accelerate_repeated_navigation(vec![EditCommand::DeleteBack], true),
        [EditCommand::DeleteBack]
    );
}

#[test]
fn repeated_note_navigation_skips_a_separator_after_the_first_step() {
    let blocks = crate::markdown::parse_note("- one\n- two\n\nnext\n");
    let commands = remap_note_vertical_commands(
        accelerate_repeated_navigation(vec![EditCommand::Move(Motion::Down)], true),
        &blocks,
        0,
    );
    assert_eq!(
        commands,
        [
            EditCommand::Move(Motion::Down),
            EditCommand::Move(Motion::Down),
            EditCommand::Move(Motion::Down),
        ]
    );
}
