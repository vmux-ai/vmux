use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_api::BinEvent;
use vmux_core::event::*;

use super::*;
use crate::host::editor::FileView;

impl ExplorerState {
    fn holding(paths: &[&str]) -> Self {
        Self {
            open_editors: paths.iter().map(PathBuf::from).collect(),
            ..Self::default()
        }
    }
}

#[test]
fn closing_an_editor_hands_back_the_tab_that_takes_its_place() {
    let mut state = ExplorerState::holding(&["/a", "/b", "/c"]);
    assert_eq!(
        state.close_editor(Path::new("/b")),
        Some(PathBuf::from("/c")),
        "closing a middle tab moves right, as the tab strip reads"
    );
    assert_eq!(
        state.close_editor(Path::new("/c")),
        Some(PathBuf::from("/a")),
        "closing the last tab falls back to the one on its left"
    );
    assert_eq!(state.close_editor(Path::new("/a")), None);
    assert!(state.open_editors.is_empty());
}

#[test]
fn closing_an_editor_that_was_never_open_changes_nothing() {
    let mut state = ExplorerState::holding(&["/a"]);
    assert_eq!(state.close_editor(Path::new("/zzz")), None);
    assert_eq!(state.open_editors, vec![PathBuf::from("/a")]);
}

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

impl ExplorerTree {
    fn in_app<'a>(app: &'a App, root: &Path) -> &'a Self {
        app.world()
            .resource::<ExplorerTrees>()
            .by_root
            .get(root)
            .unwrap_or_else(|| panic!("no explorer tree for {}", root.display()))
    }
}

struct ExplorerApp;

impl ExplorerApp {
    fn hidden() -> App {
        Self::with(false)
    }

    fn visible() -> App {
        Self::with(true)
    }

    fn with(default_visible: bool) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TreePlugin))
            .insert_resource(ExplorerPanelDefaults {
                default_visible,
                width: 240,
            });
        app
    }
}

fn wait_for_children(app: &mut App, root: &Path, path: &Path) {
    for _ in 0..1000 {
        app.update();
        let loaded = app
            .world()
            .resource::<ExplorerTrees>()
            .by_root
            .get(root)
            .is_some_and(|tree| tree.children.contains_key(path));
        if loaded {
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
    let mut app = ExplorerApp::hidden();
    let e = app
        .world_mut()
        .spawn((FileView { path: file }, ExplorerState::default()))
        .id();
    wait_for_children(&mut app, tmp.path(), tmp.path());
    assert_eq!(
        app.world().get::<ExplorerState>(e).unwrap().root.as_path(),
        tmp.path()
    );
    let tree = ExplorerTree::in_app(&app, tmp.path());
    assert!(tree.expanded.contains(&tmp.path().to_path_buf()));
    assert!(
        tree.children
            .get(tmp.path())
            .unwrap()
            .iter()
            .any(|x| x.name == "src")
    );
    assert!(app.world().get::<ExplorerTreeDirty>(e).is_some());
}

#[test]
fn a_second_page_on_a_warm_root_reuses_the_loaded_tree() {
    let tmp = git_repo();
    let src = tmp.path().join("src");
    let mut app = ExplorerApp::hidden();
    let first = app
        .world_mut()
        .spawn((
            FileView {
                path: tmp.path().join("README.md"),
            },
            ExplorerState::default(),
        ))
        .id();
    wait_for_children(&mut app, tmp.path(), &src);
    app.world_mut()
        .entity_mut(first)
        .remove::<ExplorerTreeDirty>();
    let second = app
        .world_mut()
        .spawn((
            FileView {
                path: src.join("lib.rs"),
            },
            ExplorerState::default(),
        ))
        .id();
    app.update();
    assert!(
        app.world().get::<ExplorerTreeDirty>(second).is_some(),
        "a page joining a warm root must still be asked to draw its tree"
    );
    assert!(
        app.world().get::<ExplorerTreeDirty>(first).is_none(),
        "a page joining a warm root must not re-walk the directories others already hold"
    );
}

#[test]
fn expansion_outlives_the_page_that_made_it() {
    let tmp = git_repo();
    let src = tmp.path().join("src");
    let mut app = ExplorerApp::hidden();
    let first = app
        .world_mut()
        .spawn((
            FileView {
                path: tmp.path().join("README.md"),
            },
            ExplorerState::default(),
        ))
        .id();
    wait_for_children(&mut app, tmp.path(), tmp.path());
    toggle(&mut app, first, &src);
    wait_for_children(&mut app, tmp.path(), &src);
    app.world_mut().entity_mut(first).despawn();
    app.update();
    assert!(
        ExplorerTree::in_app(&app, tmp.path())
            .expanded
            .contains(&src),
        "closing a page must not collapse the workspace tree the next one opens on"
    );
}

#[test]
fn pruning_drops_the_stalest_idle_trees_and_never_a_live_one() {
    let mut trees = ExplorerTrees::default();
    let roots: Vec<PathBuf> = (0..IDLE_TREE_CAPACITY + 2)
        .map(|n| PathBuf::from(format!("/project{n}")))
        .collect();
    for root in &roots {
        trees.at(root);
    }
    let live: HashSet<PathBuf> = [roots[0].clone()].into_iter().collect();

    trees.prune(&live);

    assert!(
        trees.by_root.contains_key(&roots[0]),
        "a root a page still shows must survive however stale it is"
    );
    assert!(
        !trees.by_root.contains_key(&roots[1]),
        "the stalest idle root is the one that goes"
    );
    assert!(trees.by_root.contains_key(roots.last().unwrap()));
    assert_eq!(trees.by_root.len(), IDLE_TREE_CAPACITY + 1);
}

#[test]
fn expanding_warms_the_next_level_and_stops_there() {
    let tmp = git_repo();
    let deep = tmp.path().join("src").join("deep");
    fs::create_dir_all(&deep).unwrap();
    let mut app = ExplorerApp::hidden();
    app.world_mut().spawn((
        FileView {
            path: tmp.path().join("README.md"),
        },
        ExplorerState::default(),
    ));

    wait_for_children(&mut app, tmp.path(), tmp.path());
    wait_for_children(&mut app, tmp.path(), &tmp.path().join("src"));

    for _ in 0..200 {
        app.update();
        std::thread::yield_now();
    }
    let tree = ExplorerTree::in_app(&app, tmp.path());
    assert!(
        !tree.expanded.contains(&tmp.path().join("src")),
        "warming must not expand anything on the user's behalf"
    );
    assert!(
        !tree.children.contains_key(&deep),
        "a warmed directory must not warm its own children, or a deep tree loads itself"
    );
}

#[test]
fn toggle_expands_then_collapses_subdir() {
    let tmp = git_repo();
    let file = tmp.path().join("README.md");
    let mut app = ExplorerApp::hidden();
    let e = app
        .world_mut()
        .spawn((FileView { path: file }, ExplorerState::default()))
        .id();
    wait_for_children(&mut app, tmp.path(), tmp.path());
    let src = tmp.path().join("src");
    toggle(&mut app, e, &src);
    wait_for_children(&mut app, tmp.path(), &src);
    let tree = ExplorerTree::in_app(&app, tmp.path());
    assert!(tree.expanded.contains(&src));
    assert!(
        tree.children
            .get(&src)
            .unwrap()
            .iter()
            .any(|x| x.name == "lib.rs")
    );
    toggle(&mut app, e, &src);
    assert!(
        !ExplorerTree::in_app(&app, tmp.path())
            .expanded
            .contains(&src)
    );
}

#[test]
fn reveal_current_expands_ancestors_and_focuses_file() {
    let tmp = git_repo();
    let file = tmp.path().join("src").join("lib.rs");
    let mut app = ExplorerApp::hidden();
    let e = app
        .world_mut()
        .spawn((FileView { path: file.clone() }, ExplorerState::default()))
        .id();
    wait_for_children(&mut app, tmp.path(), tmp.path());
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerRevealCurrent,
    });
    let src = tmp.path().join("src");
    wait_for_children(&mut app, tmp.path(), &src);
    let tree = ExplorerTree::in_app(&app, tmp.path());
    assert!(tree.expanded.contains(tmp.path()));
    assert!(tree.expanded.contains(&src));
    assert_eq!(
        app.world()
            .get::<ExplorerState>(e)
            .unwrap()
            .focus_path
            .as_deref(),
        Some(file.as_path())
    );
}

#[test]
fn repeated_reveal_skips_unchanged_tree_rebuild() {
    let tmp = git_repo();
    let file = tmp.path().join("src").join("lib.rs");
    let mut app = ExplorerApp::hidden();
    let e = app
        .world_mut()
        .spawn((FileView { path: file }, ExplorerState::default()))
        .id();
    wait_for_children(&mut app, tmp.path(), tmp.path());
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerRevealCurrent,
    });
    wait_for_children(&mut app, tmp.path(), &tmp.path().join("src"));
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
fn opening_a_file_reveals_it_without_an_explicit_request() {
    let tmp = git_repo();
    let src = tmp.path().join("src");
    let file = src.join("lib.rs");
    let mut app = ExplorerApp::visible();
    let e = app
        .world_mut()
        .spawn((
            FileView {
                path: tmp.path().join("README.md"),
            },
            ExplorerState::default(),
        ))
        .id();
    wait_for_children(&mut app, tmp.path(), tmp.path());
    assert!(
        !ExplorerTree::in_app(&app, tmp.path())
            .expanded
            .contains(&src)
    );
    app.world_mut().get_mut::<FileView>(e).unwrap().path = file.clone();
    wait_for_children(&mut app, tmp.path(), &src);
    assert!(
        ExplorerTree::in_app(&app, tmp.path())
            .expanded
            .contains(&src)
    );
    assert_eq!(
        app.world()
            .get::<ExplorerState>(e)
            .unwrap()
            .focus_path
            .as_deref(),
        Some(file.as_path())
    );
}

#[test]
fn opening_a_file_leaves_a_hidden_explorer_collapsed() {
    let tmp = git_repo();
    let src = tmp.path().join("src");
    let mut app = ExplorerApp::hidden();
    let e = app
        .world_mut()
        .spawn((
            FileView {
                path: tmp.path().join("README.md"),
            },
            ExplorerState::default(),
        ))
        .id();
    wait_for_children(&mut app, tmp.path(), tmp.path());
    app.world_mut().get_mut::<FileView>(e).unwrap().path = src.join("lib.rs");
    for _ in 0..200 {
        app.update();
        std::thread::yield_now();
    }
    assert!(
        !ExplorerTree::in_app(&app, tmp.path())
            .expanded
            .contains(&src)
    );
    assert!(
        app.world()
            .get::<ExplorerState>(e)
            .unwrap()
            .focus_path
            .is_none()
    );
}

#[derive(Resource, Default)]
struct SentReveals(Vec<ExplorerReveal>);

impl SentReveals {
    fn watch(app: &mut App, webview: Entity) {
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(webview);
        app.insert_non_send(browsers)
            .init_resource::<Self>()
            .add_observer(Self::record);
    }

    fn record(emit: On<BinHostEmitEvent>, mut sent: ResMut<Self>) {
        if emit.id() != ExplorerFocusEvent::id() {
            return;
        }
        let decoded = rkyv::from_bytes::<ExplorerFocusEvent, rkyv::rancor::Error>(emit.payload());
        let Ok(event) = decoded else {
            return;
        };
        sent.0.push(event.reveal);
    }

    fn drain(app: &mut App) -> Vec<ExplorerReveal> {
        std::mem::take(&mut app.world_mut().resource_mut::<Self>().0)
    }
}

#[test]
fn only_an_asked_for_reveal_may_take_focus_from_the_editor() {
    let tmp = git_repo();
    let src = tmp.path().join("src");
    let mut app = ExplorerApp::visible();
    let e = app
        .world_mut()
        .spawn((
            FileView {
                path: tmp.path().join("README.md"),
            },
            ExplorerState::default(),
        ))
        .id();
    SentReveals::watch(&mut app, e);
    wait_for_children(&mut app, tmp.path(), tmp.path());
    let _ = SentReveals::drain(&mut app);
    app.world_mut().get_mut::<FileView>(e).unwrap().path = src.join("lib.rs");
    wait_for_children(&mut app, tmp.path(), &src);
    assert_eq!(
        SentReveals::drain(&mut app),
        vec![ExplorerReveal::Followed],
        "opening a file must not pull the caret out of the editor"
    );
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerRevealCurrent,
    });
    app.update();
    assert_eq!(
        SentReveals::drain(&mut app),
        vec![ExplorerReveal::Requested]
    );
}

#[test]
fn collapse_all_leaves_the_root_expanded_and_nothing_else() {
    let tmp = git_repo();
    let src = tmp.path().join("src");
    let mut app = ExplorerApp::visible();
    let e = app
        .world_mut()
        .spawn((
            FileView {
                path: src.join("lib.rs"),
            },
            ExplorerState::default(),
        ))
        .id();
    wait_for_children(&mut app, tmp.path(), &src);
    assert!(ExplorerTree::in_app(&app, tmp.path()).expanded.len() > 1);
    app.world_mut().entity_mut(e).remove::<ExplorerTreeDirty>();
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerCollapseAll,
    });
    app.update();
    assert_eq!(
        ExplorerTree::in_app(&app, tmp.path()).expanded,
        HashSet::from([tmp.path().to_path_buf()]),
        "dropping the root makes the next reveal look like a tree change and re-scroll"
    );
    assert!(app.world().get::<ExplorerTreeDirty>(e).is_some());
}

#[test]
fn showing_the_panel_reveals_without_taking_the_caret() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, PanelPlugin))
        .init_resource::<ExplorerTrees>()
        .insert_resource(ExplorerPanelDefaults {
            default_visible: false,
            width: 240,
        });
    let stack = app
        .world_mut()
        .spawn(StackExplorerVisibility { visible: false })
        .id();
    let view = app
        .world_mut()
        .spawn((
            FileView {
                path: PathBuf::from("/a.rs"),
            },
            ExplorerState::default(),
            ChildOf(stack),
        ))
        .id();
    SentReveals::watch(&mut app, view);

    app.world_mut().trigger(BinReceive {
        webview: view,
        payload: ExplorerPanelSetVisible {
            visible: true,
            client_id: 1,
            request_id: 1,
        },
    });
    app.update();

    assert_eq!(
        SentReveals::drain(&mut app),
        vec![ExplorerReveal::Followed],
        "opening the panel shows where you are; it does not move the keyboard there"
    );
}

#[test]
fn panel_visibility_is_shared_only_within_stack() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, PanelPlugin))
        .init_resource::<ExplorerTrees>();
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
            ExplorerPanelSent,
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
            ExplorerPanelSent,
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
            ExplorerPanelSent,
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
    assert!(app.world().get::<ExplorerPanelSent>(first).is_some());
    assert!(app.world().get::<ExplorerPanelSent>(peer).is_none());
    assert!(app.world().get::<ExplorerPanelSent>(other).is_some());

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
fn panel_open_reveals_current_file() {
    let tmp = git_repo();
    let file = tmp.path().join("src").join("lib.rs");
    let mut app = ExplorerApp::hidden();
    app.add_plugins(PanelPlugin);
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
    wait_for_children(&mut app, tmp.path(), tmp.path());
    app.world_mut().trigger(BinReceive {
        webview: e,
        payload: ExplorerPanelSetVisible {
            visible: true,
            client_id: 9,
            request_id: 1,
        },
    });
    wait_for_children(&mut app, tmp.path(), &tmp.path().join("src"));
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
    app.add_plugins((MinimalPlugins, PanelPlugin))
        .insert_resource(ExplorerPanelDefaults {
            default_visible: true,
            width: 240,
        });
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
    assert_eq!(app.world().resource::<ExplorerPanelDefaults>().width, 600);
}

#[test]
fn open_editors_track_on_navigate_and_close() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("src");
    std::fs::create_dir(&dir).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TabsPlugin))
        .insert_resource(crate::lsp::manager::LspManager::new(
            crate::lsp::LspOutbox::default(),
            crate::lsp::server_request::ServerEvents::default().sender(),
        ));
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
    assert_eq!(st.open_editors, vec![b.clone()]);
    app.world_mut().get_mut::<FileView>(e).unwrap().path = dir.clone();
    app.update();
    let st = app.world().get::<ExplorerState>(e).unwrap();
    assert_eq!(
        st.open_editors,
        vec![b.clone(), dir],
        "a directory the reader navigated to needs a tab of its own, or the only way out \
         of the navigator is to open another file"
    );
    let c = PathBuf::from("/proj/c.rs");
    app.world_mut().get_mut::<FileView>(e).unwrap().path = c.clone();
    app.update();
    let st = app.world().get::<ExplorerState>(e).unwrap();
    assert_eq!(
        st.open_editors,
        vec![b, c],
        "opening a file from the directory navigator replaces that navigator tab"
    );
}
