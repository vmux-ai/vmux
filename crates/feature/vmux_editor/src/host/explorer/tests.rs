use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_api::{BinEvent, UiEventPermissions};
#[cfg(ui)]
use vmux_ecs::event::*;
use vmux_ecs::event::{ExplorerReveal, FileUiState};

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

struct ExplorerApp;

impl ExplorerApp {
    fn git_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("README.md"), "# hi\n").unwrap();
        fs::write(tmp.path().join("src").join("lib.rs"), "fn main(){}\n").unwrap();
        tmp
    }

    fn toggle(app: &mut App, e: Entity, path: &Path) {
        app.world_mut().trigger(UiInput {
            webview: e,
            payload: ExplorerTreeToggle {
                path: path.to_string_lossy().to_string(),
            },
        });
    }

    fn tree_entity(app: &App, root: &Path) -> Entity {
        app.world()
            .iter_entities()
            .find_map(|entity| {
                let tree = entity.get::<ExplorerTree>()?;
                tree.answers_for(root).then_some(entity.id())
            })
            .unwrap_or_else(|| panic!("no explorer tree for {}", root.display()))
    }

    fn directory_entity(app: &App, root: &Path, path: &Path) -> Option<Entity> {
        let tree = Self::tree_entity(app, root);
        app.world().iter_entities().find_map(|entity| {
            let parent = entity.get::<ChildOf>()?;
            let dir = entity.get::<ExplorerDirectory>()?;
            (parent.parent() == tree && dir.path == path).then_some(entity.id())
        })
    }

    fn is_expanded(app: &App, root: &Path, path: &Path) -> bool {
        Self::directory_entity(app, root, path).is_some_and(|entity| {
            app.world()
                .get::<ExplorerDirectoryExpanded>(entity)
                .is_some()
        })
    }

    fn expanded_paths(app: &App, root: &Path) -> HashSet<PathBuf> {
        let tree = Self::tree_entity(app, root);
        let mut expanded = HashSet::new();
        for entity in app.world().iter_entities() {
            if entity.get::<ChildOf>().is_some_and(|p| p.parent() == tree)
                && entity.get::<ExplorerDirectoryExpanded>().is_some()
                && let Some(dir) = entity.get::<ExplorerDirectory>()
            {
                expanded.insert(dir.path.clone());
            }
        }
        expanded
    }

    fn entries<'a>(app: &'a App, root: &Path, path: &Path) -> Option<&'a [FileDirEntry]> {
        let entity = Self::directory_entity(app, root, path)?;
        app.world()
            .get::<ExplorerDirectoryEntries>(entity)
            .map(|entries| entries.0.as_slice())
    }

    fn any_loading(app: &App, root: &Path) -> bool {
        let tree = Self::tree_entity(app, root);
        app.world().iter_entities().any(|entity| {
            entity.get::<ChildOf>().is_some_and(|p| p.parent() == tree)
                && entity.get::<ExplorerDirectoryLoading>().is_some()
        })
    }

    fn wait_for_children(app: &mut App, root: &Path, path: &Path) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            app.update();
            let loaded = app.world().iter_entities().any(|entity| {
                entity
                    .get::<ExplorerTree>()
                    .is_some_and(|tree| tree.answers_for(root))
            }) && Self::entries(app, root, path).is_some()
                && !Self::any_loading(app, root);
            if loaded {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("directory load did not finish: {}", path.display());
    }

    fn hidden() -> App {
        Self::with(false)
    }

    fn visible() -> App {
        Self::with(true)
    }

    fn with(default_visible: bool) -> App {
        let mut app = App::new();
        app.world_mut().spawn(ExplorerPanelDefaults {
            default_visible,
            width: 240,
            loaded: true,
        });
        app.add_plugins((MinimalPlugins, TreePlugin));
        app
    }
}

#[test]
#[cfg(ui)]
fn editor_page_allows_explorer_section_toggles() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, crate::ui::FilePage::plugin()));
    app.world_mut().run_schedule(PreStartup);
    let mut permissions = app.world_mut().query::<&UiEventPermissions>();
    let permissions = permissions.iter(app.world()).collect::<Vec<_>>();

    for permission in [
        ExplorerOpenEditorsToggle::PERMISSION,
        ExplorerFilesToggle::PERMISSION,
        ExplorerOutlineToggle::PERMISSION,
    ] {
        assert!(UiEventPermissions::allows_page(
            permissions.iter().copied(),
            "file:///project/main.rs",
            permission,
        ));
    }
}

#[test]
#[cfg(ui)]
fn projects_page_allows_every_shared_editor_action() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        crate::ui::FilePage::plugin(),
        crate::ui::ProjectsPage::plugin(),
    ));
    app.world_mut().run_schedule(PreStartup);
    let mut query = app.world_mut().query::<&UiEventPermissions>();
    let pages = query.iter(app.world()).copied().collect::<Vec<_>>();
    let file = pages.iter().find(|page| page.url == "file://").unwrap();
    let projects = pages
        .iter()
        .find(|page| page.url == "vmux://projects/")
        .unwrap();

    assert!(projects.owns_subtree);
    for permission in file.permissions {
        assert!(
            projects.permissions.contains(permission),
            "Projects page is missing shared editor permission {permission}"
        );
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

#[test]
fn noting_an_open_editor_deduplicates_and_preserves_order() {
    let mut state = ExplorerState::default();
    state.note_open(Path::new("/a"));
    state.note_open(Path::new("/b"));
    state.note_open(Path::new("/a"));
    assert_eq!(
        state.open_editors,
        vec![PathBuf::from("/a"), PathBuf::from("/b")]
    );
}

#[test]
fn explorer_tree_rows_follow_expanded_directories() {
    let tmp = ExplorerApp::git_repo();
    let src = tmp.path().join("src");
    let mut app = ExplorerApp::hidden();
    let view = app
        .world_mut()
        .spawn((
            FileView {
                path: tmp.path().join("README.md"),
            },
            ExplorerState::default(),
            vmux_ecs::page::PageReady {},
        ))
        .id();
    SentReveals::watch(&mut app, view);
    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    ExplorerApp::toggle(&mut app, view, &src);
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &src);
    app.update();
    let tree = app
        .world()
        .resource::<SentReveals>()
        .state
        .as_ref()
        .unwrap()
        .explorer
        .explorer_tree
        .as_ref()
        .unwrap();
    let actual: Vec<(&str, u16)> = tree
        .rows
        .iter()
        .map(|row| (row.name.as_str(), row.depth))
        .collect();
    assert_eq!(
        actual,
        vec![(".git", 0), ("src", 0), ("lib.rs", 1), ("README.md", 0)]
    );
    assert!(tree.rows[1].expanded);
    assert!(!tree.rows[1].loading);
}

#[test]
fn a_new_entry_uses_the_selected_directory_or_file_parent() {
    let root = PathBuf::from("/r");
    let source = root.join("src");
    let mut app = ExplorerApp::hidden();
    let tree = app.world_mut().spawn(ExplorerTree::new(root.clone())).id();
    app.world_mut().spawn((
        ChildOf(tree),
        ExplorerDirectory { path: root.clone() },
        ExplorerDirectoryEntries(vec![
            FileDirEntry {
                name: "src".into(),
                path: source.to_string_lossy().into_owned(),
                is_dir: true,
            },
            FileDirEntry {
                name: "lib.rs".into(),
                path: source.join("lib.rs").to_string_lossy().into_owned(),
                is_dir: false,
            },
        ]),
    ));
    let mut state = bevy::ecs::system::SystemState::<ExplorerTreeLookup>::new(app.world_mut());
    let lookup = state.get(app.world()).unwrap();
    assert_eq!(lookup.create_parent(tree, &source), Some(source.clone()));
    assert_eq!(
        lookup.create_parent(tree, &source.join("lib.rs")),
        Some(source)
    );
}

#[test]
fn a_new_entry_uses_the_root_without_a_live_selection() {
    let root = PathBuf::from("/r");
    let mut app = ExplorerApp::hidden();
    let tree = app.world_mut().spawn(ExplorerTree::new(root.clone())).id();
    let mut state = bevy::ecs::system::SystemState::<ExplorerTreeLookup>::new(app.world_mut());
    let lookup = state.get(app.world()).unwrap();
    assert_eq!(
        lookup.create_parent(tree, Path::new("")),
        Some(root.clone())
    );
    assert_eq!(
        lookup.create_parent(tree, Path::new("/r/dropped")),
        Some(root)
    );
}

#[test]
fn init_builds_root_listing_and_marks_dirty() {
    let tmp = ExplorerApp::git_repo();
    let file = tmp.path().join("src").join("lib.rs");
    let mut app = ExplorerApp::hidden();
    let e = app
        .world_mut()
        .spawn((FileView { path: file }, ExplorerState::default()))
        .id();
    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    let tree_of = app.world().get::<UsesExplorerTree>(e).unwrap();
    assert_eq!(
        app.world().get::<ExplorerTree>(tree_of.0).unwrap().root(),
        tmp.path()
    );
    assert!(ExplorerApp::is_expanded(&app, tmp.path(), tmp.path()));
    assert!(
        ExplorerApp::entries(&app, tmp.path(), tmp.path())
            .unwrap()
            .iter()
            .any(|x| x.name == "src")
    );
    assert!(app.world().get::<ExplorerTreeDirty>(e).is_some());
}

#[test]
fn a_second_page_on_a_warm_root_reuses_the_loaded_tree() {
    let tmp = ExplorerApp::git_repo();
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
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &src);
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
    let first_tree = app.world().get::<UsesExplorerTree>(first).unwrap().0;
    let second_tree = app.world().get::<UsesExplorerTree>(second).unwrap().0;
    assert_eq!(first_tree, second_tree);
    let users = app.world().get::<ExplorerTreeUsers>(first_tree).unwrap();
    assert!(users.0.contains(&first));
    assert!(users.0.contains(&second));
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
    let tmp = ExplorerApp::git_repo();
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
    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    ExplorerApp::toggle(&mut app, first, &src);
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &src);
    app.world_mut().entity_mut(first).despawn();
    app.update();
    assert!(
        ExplorerApp::is_expanded(&app, tmp.path(), &src),
        "closing a page must not collapse the workspace tree the next one opens on"
    );
}

#[test]
fn pruning_drops_the_stalest_idle_trees_and_never_a_live_one() {
    let mut app = App::new();
    app.world_mut().spawn(ExplorerPanelDefaults {
        default_visible: false,
        width: 240,
        loaded: true,
    });
    app.add_plugins((MinimalPlugins, TreePlugin));
    let roots: Vec<PathBuf> = (0..ExplorerTree::IDLE_CAPACITY + 2)
        .map(|n| PathBuf::from(format!("/project{n}")))
        .collect();
    let mut tree_entities = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let mut tree = ExplorerTree::new(root.clone());
        tree.used -= std::time::Duration::from_secs((roots.len() - index) as u64);
        let entity = app.world_mut().spawn(tree).id();
        app.world_mut()
            .spawn((ChildOf(entity), ExplorerDirectory { path: root.clone() }));
        tree_entities.push(entity);
    }
    let live = app
        .world_mut()
        .spawn((ExplorerState::default(), UsesExplorerTree(tree_entities[0])))
        .id();
    let closed = app.world_mut().spawn(ExplorerState::default()).id();
    app.world_mut().entity_mut(closed).remove::<ExplorerState>();

    app.update();

    assert!(
        app.world()
            .iter_entities()
            .filter_map(|entity| entity.get::<ExplorerTree>())
            .any(|tree| tree.answers_for(&roots[0])),
        "a root a page still shows must survive however stale it is"
    );
    assert!(
        !app.world()
            .iter_entities()
            .filter_map(|entity| entity.get::<ExplorerTree>())
            .any(|tree| tree.answers_for(&roots[1])),
        "the stalest idle root is the one that goes"
    );
    assert!(
        !app.world()
            .iter_entities()
            .filter_map(|entity| entity.get::<ExplorerDirectory>())
            .any(|dir| dir.path == roots[1]),
        "despawning an idle tree must remove its canonical child directories"
    );
    assert!(
        app.world()
            .iter_entities()
            .filter_map(|entity| entity.get::<ExplorerTree>())
            .any(|tree| tree.answers_for(roots.last().unwrap()))
    );
    assert_eq!(
        app.world()
            .iter_entities()
            .filter_map(|entity| entity.get::<ExplorerTree>())
            .count(),
        ExplorerTree::IDLE_CAPACITY + 1
    );
    assert!(app.world().get_entity(live).is_ok());
}

#[test]
fn refreshing_the_root_also_refreshes_expanded_subdirectories() {
    let tmp = ExplorerApp::git_repo();
    let src = tmp.path().join("src");
    let mut app = ExplorerApp::hidden();
    let view = app
        .world_mut()
        .spawn((
            FileView {
                path: tmp.path().join("README.md"),
            },
            ExplorerState::default(),
        ))
        .id();
    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    ExplorerApp::toggle(&mut app, view, &src);
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &src);
    fs::write(src.join("added.rs"), "pub fn added() {}\n").unwrap();
    app.world_mut().trigger(UiInput {
        webview: view,
        payload: ExplorerTreeRefresh {
            path: tmp.path().to_string_lossy().into_owned(),
        },
    });
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &src);
    assert!(
        ExplorerApp::entries(&app, tmp.path(), &src)
            .unwrap()
            .iter()
            .any(|entry| entry.name == "added.rs")
    );
}

#[test]
fn expanding_warms_the_next_level_and_stops_there() {
    let tmp = ExplorerApp::git_repo();
    let deep = tmp.path().join("src").join("deep");
    fs::create_dir_all(&deep).unwrap();
    let mut app = ExplorerApp::hidden();
    app.world_mut().spawn((
        FileView {
            path: tmp.path().join("README.md"),
        },
        ExplorerState::default(),
    ));

    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &tmp.path().join("src"));

    for _ in 0..200 {
        app.update();
        std::thread::yield_now();
    }
    assert!(
        !ExplorerApp::is_expanded(&app, tmp.path(), &tmp.path().join("src")),
        "warming must not expand anything on the user's behalf"
    );
    assert!(
        ExplorerApp::entries(&app, tmp.path(), &deep).is_none(),
        "a warmed directory must not warm its own children, or a deep tree loads itself"
    );
}

#[test]
fn toggle_expands_then_collapses_subdir() {
    let tmp = ExplorerApp::git_repo();
    let file = tmp.path().join("README.md");
    let mut app = ExplorerApp::hidden();
    let e = app
        .world_mut()
        .spawn((FileView { path: file }, ExplorerState::default()))
        .id();
    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    let src = tmp.path().join("src");
    ExplorerApp::toggle(&mut app, e, &src);
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &src);
    assert!(ExplorerApp::is_expanded(&app, tmp.path(), &src));
    assert!(
        ExplorerApp::entries(&app, tmp.path(), &src)
            .unwrap()
            .iter()
            .any(|x| x.name == "lib.rs")
    );
    ExplorerApp::toggle(&mut app, e, &src);
    app.update();
    assert!(!ExplorerApp::is_expanded(&app, tmp.path(), &src));
}

#[test]
fn reveal_current_expands_ancestors_and_focuses_file() {
    let tmp = ExplorerApp::git_repo();
    let file = tmp.path().join("src").join("lib.rs");
    let mut app = ExplorerApp::hidden();
    let e = app
        .world_mut()
        .spawn((FileView { path: file.clone() }, ExplorerState::default()))
        .id();
    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    app.world_mut().trigger(UiInput {
        webview: e,
        payload: ExplorerRevealCurrent,
    });
    let src = tmp.path().join("src");
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &src);
    assert!(ExplorerApp::is_expanded(&app, tmp.path(), tmp.path()));
    assert!(ExplorerApp::is_expanded(&app, tmp.path(), &src));
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
    let tmp = ExplorerApp::git_repo();
    let file = tmp.path().join("src").join("lib.rs");
    let mut app = ExplorerApp::hidden();
    let e = app
        .world_mut()
        .spawn((FileView { path: file }, ExplorerState::default()))
        .id();
    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    app.world_mut().trigger(UiInput {
        webview: e,
        payload: ExplorerRevealCurrent,
    });
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &tmp.path().join("src"));
    app.world_mut().entity_mut(e).remove::<ExplorerTreeDirty>();
    app.world_mut()
        .get_mut::<ExplorerState>(e)
        .unwrap()
        .focus_path = None;
    app.world_mut().trigger(UiInput {
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
    let tmp = ExplorerApp::git_repo();
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
    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    assert!(!ExplorerApp::is_expanded(&app, tmp.path(), &src));
    app.world_mut().get_mut::<FileView>(e).unwrap().path = file.clone();
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &src);
    assert!(ExplorerApp::is_expanded(&app, tmp.path(), &src));
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
    let tmp = ExplorerApp::git_repo();
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
    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    app.world_mut().get_mut::<FileView>(e).unwrap().path = src.join("lib.rs");
    for _ in 0..200 {
        app.update();
        std::thread::yield_now();
    }
    assert!(!ExplorerApp::is_expanded(&app, tmp.path(), &src));
    assert!(
        app.world()
            .get::<ExplorerState>(e)
            .unwrap()
            .focus_path
            .is_none()
    );
}

#[derive(Resource, Default)]
struct SentReveals {
    revision: u64,
    reveals: Vec<ExplorerReveal>,
    state: Option<FileUiState>,
}

impl SentReveals {
    fn watch(app: &mut App, webview: Entity) {
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(webview);
        app.add_plugins(vmux_ecs::UiStatePlugin::<FileUiState>::default())
            .insert_non_send(browsers)
            .init_resource::<Self>()
            .add_observer(Self::record);
        app.world_mut().entity_mut(webview).insert((
            bevy_cef::prelude::BinIpcSurfaceGeneration(1),
            bevy_cef::prelude::BinIpcNavigationRevision(1),
            bevy_cef::prelude::BinIpcEpoch(1),
            vmux_ecs::PageMetadata {
                url: "file:///repo".to_string(),
                ..Default::default()
            },
        ));
        app.world_mut().spawn(UiEventPermissions {
            url: "file:///repo",
            owns_subtree: false,
            permissions: &["FileUiState"],
        });
    }

    fn record(emit: On<BinHostEmitEvent>, mut sent: ResMut<Self>) {
        if emit.id() != FileUiState::id() {
            return;
        }
        emit.mark_delivered();
        let frame = rkyv::from_bytes::<
            vmux_api::UiStateFrame<FileUiState, <FileUiState as vmux_api::UiState>::Update>,
            rkyv::rancor::Error,
        >(emit.payload())
        .unwrap();
        let event = match frame {
            vmux_api::UiStateFrame::Snapshot(state) => state,
            vmux_api::UiStateFrame::Updates(updates) => {
                <FileUiState as vmux_api::UiState>::from_updates(sent.state.take(), updates)
            }
        };
        sent.state = Some(event.clone());
        let Some(focus) = event.explorer.explorer_focus else {
            return;
        };
        if focus.revision <= sent.revision {
            return;
        }
        sent.revision = focus.revision;
        sent.reveals.push(focus.reveal);
    }

    fn drain(app: &mut App) -> Vec<ExplorerReveal> {
        std::mem::take(&mut app.world_mut().resource_mut::<Self>().reveals)
    }
}

#[test]
fn only_an_asked_for_reveal_may_take_focus_from_the_editor() {
    let tmp = ExplorerApp::git_repo();
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
    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    let _ = SentReveals::drain(&mut app);
    app.world_mut().get_mut::<FileView>(e).unwrap().path = src.join("lib.rs");
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &src);
    assert_eq!(
        SentReveals::drain(&mut app),
        vec![ExplorerReveal::Followed],
        "opening a file must not pull the caret out of the editor"
    );
    app.world_mut().trigger(UiInput {
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
    let tmp = ExplorerApp::git_repo();
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
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &src);
    assert!(ExplorerApp::expanded_paths(&app, tmp.path()).len() > 1);
    app.world_mut().entity_mut(e).remove::<ExplorerTreeDirty>();
    app.world_mut().trigger(UiInput {
        webview: e,
        payload: ExplorerCollapseAll,
    });
    app.update();
    assert_eq!(
        ExplorerApp::expanded_paths(&app, tmp.path()),
        HashSet::from([tmp.path().to_path_buf()]),
        "dropping the root makes the next reveal look like a tree change and re-scroll"
    );
    assert!(app.world().get::<ExplorerTreeDirty>(e).is_some());
}

#[test]
fn showing_the_panel_reveals_without_taking_the_caret() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TreePlugin, PanelPlugin));
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
    app.update();

    app.world_mut().trigger(UiInput {
        webview: view,
        payload: ExplorerPanelSetVisible { visible: true },
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
    app.add_plugins((MinimalPlugins, TreePlugin, PanelPlugin));
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
    app.update();
    for entity in [first, peer, other] {
        app.world_mut().entity_mut(entity).insert(ExplorerPanelSent);
    }
    app.world_mut().trigger(UiInput {
        webview: first,
        payload: ExplorerPanelSetVisible { visible: false },
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
    assert!(app.world().get::<ExplorerPanelSent>(first).is_none());
    assert!(app.world().get::<ExplorerPanelSent>(peer).is_none());
    assert!(app.world().get::<ExplorerPanelSent>(other).is_some());
}

#[test]
fn panel_open_reveals_current_file() {
    let tmp = ExplorerApp::git_repo();
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
    ExplorerApp::wait_for_children(&mut app, tmp.path(), tmp.path());
    app.world_mut().trigger(UiInput {
        webview: e,
        payload: ExplorerPanelSetVisible { visible: true },
    });
    ExplorerApp::wait_for_children(&mut app, tmp.path(), &tmp.path().join("src"));
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
    app.add_plugins((MinimalPlugins, PanelPlugin));
    app.update();
    let e = app
        .world_mut()
        .spawn(FileView {
            path: PathBuf::from("/x"),
        })
        .id();
    app.world_mut().trigger(UiInput {
        webview: e,
        payload: ExplorerPanelWidth { px: 9000 },
    });
    let mut defaults = app.world_mut().query::<&ExplorerPanelDefaults>();
    assert_eq!(defaults.single(app.world()).unwrap().width, 600);
}

#[test]
fn open_editors_track_on_navigate_and_close() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("src");
    fs::create_dir(&dir).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TabsPlugin));

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
    app.world_mut().trigger(UiInput {
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
