use super::*;
use std::fs;

fn git_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join(".git")).unwrap();
    fs::create_dir(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("README.md"), "# hi\n").unwrap();
    fs::write(tmp.path().join("src").join("lib.rs"), "fn main(){}\n").unwrap();
    tmp
}

fn toggle(app: &mut App, e: Entity, path: &Path) {
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerTreeToggle {
            path: path.to_string_lossy().to_string(),
        },
    });
}

fn wait_for_children(app: &mut App, e: Entity, path: &Path) {
    for _ in 0..1000 {
        app.update();
        if app
            .world()
            .get::<ExplorerState>(e)
            .is_some_and(|st| st.children.contains_key(path))
        {
            return;
        }
        std::thread::yield_now();
    }
    panic!("directory load did not finish: {}", path.display());
}

#[test]
fn init_builds_root_listing_and_marks_dirty() {
    let tmp = git_repo();
    let file = tmp.path().join("src").join("lib.rs");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, (init_explorer_state, drain_explorer_dir_loads));
    let e = app
        .world_mut()
        .spawn((FileView { path: file }, ExplorerState::default()))
        .id();
    wait_for_children(&mut app, e, tmp.path());
    let st = app.world().get::<ExplorerState>(e).unwrap();
    assert_eq!(st.root.as_path(), tmp.path());
    assert!(st.expanded.contains(&tmp.path().to_path_buf()));
    assert!(
        st.children
            .get(tmp.path())
            .unwrap()
            .iter()
            .any(|x| x.name == "src")
    );
    assert!(app.world().get::<ExplorerTreeDirty>(e).is_some());
}

#[test]
fn toggle_expands_then_collapses_subdir() {
    let tmp = git_repo();
    let file = tmp.path().join("README.md");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, (init_explorer_state, drain_explorer_dir_loads))
        .add_observer(on_explorer_tree_toggle);
    let e = app
        .world_mut()
        .spawn((FileView { path: file }, ExplorerState::default()))
        .id();
    wait_for_children(&mut app, e, tmp.path());
    let src = tmp.path().join("src");
    toggle(&mut app, e, &src);
    wait_for_children(&mut app, e, &src);
    let st = app.world().get::<ExplorerState>(e).unwrap();
    assert!(st.expanded.contains(&src));
    assert!(
        st.children
            .get(&src)
            .unwrap()
            .iter()
            .any(|x| x.name == "lib.rs")
    );
    toggle(&mut app, e, &src);
    let st = app.world().get::<ExplorerState>(e).unwrap();
    assert!(!st.expanded.contains(&src));
}

#[test]
fn reveal_current_expands_ancestors_and_focuses_file() {
    let tmp = git_repo();
    let file = tmp.path().join("src").join("lib.rs");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, (init_explorer_state, drain_explorer_dir_loads))
        .add_observer(on_explorer_reveal_current);
    let e = app
        .world_mut()
        .spawn((FileView { path: file.clone() }, ExplorerState::default()))
        .id();
    wait_for_children(&mut app, e, tmp.path());
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerRevealCurrent,
    });
    let src = tmp.path().join("src");
    wait_for_children(&mut app, e, &src);
    let st = app.world().get::<ExplorerState>(e).unwrap();
    assert!(st.expanded.contains(tmp.path()));
    assert!(st.expanded.contains(&src));
    assert_eq!(st.focus_path.as_deref(), Some(file.as_path()));
}

#[test]
fn repeated_reveal_skips_unchanged_tree_rebuild() {
    let tmp = git_repo();
    let file = tmp.path().join("src").join("lib.rs");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, (init_explorer_state, drain_explorer_dir_loads))
        .add_observer(on_explorer_reveal_current);
    let e = app
        .world_mut()
        .spawn((FileView { path: file }, ExplorerState::default()))
        .id();
    wait_for_children(&mut app, e, tmp.path());
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerRevealCurrent,
    });
    wait_for_children(&mut app, e, &tmp.path().join("src"));
    app.world_mut().entity_mut(e).remove::<ExplorerTreeDirty>();
    app.world_mut()
        .get_mut::<ExplorerState>(e)
        .unwrap()
        .focus_path = None;
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerRevealCurrent,
    });
    assert!(app.world().get::<ExplorerTreeDirty>(e).is_none());
    assert!(
        app.world()
            .get::<ExplorerState>(e)
            .unwrap()
            .focus_path
            .is_none()
    );
}

#[test]
fn panel_visibility_is_shared_only_within_stack() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_observer(on_explorer_panel_set_visible);
    let first_stack = app
        .world_mut()
        .spawn(StackExplorerVisibility { visible: true })
        .id();
    let second_stack = app
        .world_mut()
        .spawn(StackExplorerVisibility { visible: true })
        .id();
    let first = app
        .world_mut()
        .spawn((
            FileView {
                path: PathBuf::from("/a.rs"),
            },
            ExplorerState::default(),
            ExplorerChromeSent,
            ChildOf(first_stack),
        ))
        .id();
    let peer = app
        .world_mut()
        .spawn((
            FileView {
                path: PathBuf::from("/b.rs"),
            },
            ExplorerState::default(),
            ExplorerChromeSent,
            ChildOf(first_stack),
        ))
        .id();
    let other = app
        .world_mut()
        .spawn((
            FileView {
                path: PathBuf::from("/c.rs"),
            },
            ExplorerState::default(),
            ExplorerChromeSent,
            ChildOf(second_stack),
        ))
        .id();
    app.world_mut().trigger(BinReceive {
        webview: first,
        payload: ExplorerPanelSetVisible {
            visible: false,
            client_id: 7,
            request_id: 1,
        },
    });
    app.update();
    assert!(
        !app.world()
            .get::<StackExplorerVisibility>(first_stack)
            .unwrap()
            .visible
    );
    assert!(
        app.world()
            .get::<StackExplorerVisibility>(second_stack)
            .unwrap()
            .visible
    );
    assert!(app.world().get::<ExplorerChromeSent>(first).is_some());
    assert!(app.world().get::<ExplorerChromeSent>(peer).is_none());
    assert!(app.world().get::<ExplorerChromeSent>(other).is_some());

    app.world_mut().trigger(BinReceive {
        webview: first,
        payload: ExplorerPanelSetVisible {
            visible: false,
            client_id: 7,
            request_id: 2,
        },
    });
    app.update();
    let revision = app
        .world()
        .get::<StackExplorerRevision>(first_stack)
        .unwrap();
    assert_eq!(revision.client_id, 7);
    assert_eq!(revision.request_id, 2);
}

#[test]
fn global_search_opens_only_the_target_stack_explorer() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(ExplorerChrome {
            default_visible: false,
            width: 240,
        })
        .init_resource::<PendingGlobalSearch>()
        .add_message::<GlobalSearchRequest>()
        .add_systems(Update, apply_global_search_requests);
    let first_stack = app
        .world_mut()
        .spawn(StackExplorerVisibility { visible: false })
        .id();
    let second_stack = app
        .world_mut()
        .spawn(StackExplorerVisibility { visible: false })
        .id();
    let target = PathBuf::from("/project/a.rs");
    let first = app
        .world_mut()
        .spawn((
            FileView {
                path: target.clone(),
            },
            ChildOf(first_stack),
        ))
        .id();
    let second = app
        .world_mut()
        .spawn((
            FileView {
                path: PathBuf::from("/project/b.rs"),
            },
            ChildOf(second_stack),
        ))
        .id();
    app.world_mut()
        .resource_mut::<Messages<GlobalSearchRequest>>()
        .write(GlobalSearchRequest {
            target_path: target,
            root: "/project".to_string(),
            query: "needle".to_string(),
            matches: Vec::new(),
        });
    app.update();

    assert!(
        app.world()
            .get::<StackExplorerVisibility>(first_stack)
            .unwrap()
            .visible
    );
    assert!(
        !app.world()
            .get::<StackExplorerVisibility>(second_stack)
            .unwrap()
            .visible
    );
    assert!(app.world().get::<GlobalSearchState>(first).is_some());
    assert!(app.world().get::<GlobalSearchState>(second).is_none());
}

#[test]
fn panel_open_reveals_current_file() {
    let tmp = git_repo();
    let file = tmp.path().join("src").join("lib.rs");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, (init_explorer_state, drain_explorer_dir_loads))
        .add_observer(on_explorer_panel_set_visible);
    let stack = app
        .world_mut()
        .spawn(StackExplorerVisibility { visible: false })
        .id();
    let e = app
        .world_mut()
        .spawn((
            FileView { path: file.clone() },
            ExplorerState::default(),
            ChildOf(stack),
        ))
        .id();
    wait_for_children(&mut app, e, tmp.path());
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerPanelSetVisible {
            visible: true,
            client_id: 9,
            request_id: 1,
        },
    });
    wait_for_children(&mut app, e, &tmp.path().join("src"));
    assert!(
        app.world()
            .get::<StackExplorerVisibility>(stack)
            .unwrap()
            .visible
    );
    let st = app.world().get::<ExplorerState>(e).unwrap();
    assert_eq!(st.focus_path.as_deref(), Some(file.as_path()));
}

#[test]
fn panel_width_clamps() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(ExplorerChrome {
            default_visible: true,
            width: 240,
        })
        .add_observer(on_explorer_panel_width);
    let e = app
        .world_mut()
        .spawn(FileView {
            path: PathBuf::from("/x"),
        })
        .id();
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerPanelWidth { px: 9000 },
    });
    assert_eq!(app.world().resource::<ExplorerChrome>().width, 600);
}

#[test]
fn open_editors_track_on_navigate_and_close() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, sync_open_editors)
        .add_observer(on_explorer_close_editor);
    let a = PathBuf::from("/proj/a.rs");
    let b = PathBuf::from("/proj/b.rs");
    let e = app
        .world_mut()
        .spawn((FileView { path: a.clone() }, ExplorerState::default()))
        .id();
    app.update();
    app.world_mut().get_mut::<FileView>(e).unwrap().path = b.clone();
    app.update();
    let st = app.world().get::<ExplorerState>(e).unwrap();
    assert_eq!(st.open_editors, vec![a.clone(), b.clone()]);
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerCloseEditor {
            path: a.to_string_lossy().to_string(),
        },
    });
    let st = app.world().get::<ExplorerState>(e).unwrap();
    assert_eq!(st.open_editors, vec![b]);
}

#[test]
fn explorer_goto_writes_lsp_goto_message() {
    use crate::lsp::manager::LspGoto;
    use bevy::ecs::message::Messages;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<LspGoto>()
        .add_observer(on_explorer_goto);
    let e = app
        .world_mut()
        .spawn(FileView {
            path: PathBuf::from("/x.rs"),
        })
        .id();
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerGoto {
            path: "/x.rs".to_string(),
            line: 12,
        },
    });
    let mut msgs = app.world_mut().resource_mut::<Messages<LspGoto>>();
    let got: Vec<_> = msgs.drain().collect();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].line, 12);
    assert_eq!(got[0].path, PathBuf::from("/x.rs"));
}
