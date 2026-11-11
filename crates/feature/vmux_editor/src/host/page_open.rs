use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::page_open::{PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};
use vmux_ecs::profile::Projects;
use vmux_ecs::store::PageRestore;
use vmux_ecs::{LastVisitedAt, PageMetadata, PageNavigation, Url};
use vmux_flex::prelude::*;
use vmux_layout::Browser;

use crate::project::ProjectRoute;

use super::editor::{FileNavigateRequest, FileView};
use super::explorer::ExplorerState;
use super::navigation::PendingGoto;
use super::project_store::{Registrations, StoreSet};
use super::viewport::FileViewport;

pub(super) struct PageOpenPlugin;

impl Plugin for PageOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_ecs::event::RecordVisitRequest>()
            .add_systems(Startup, prewarm)
            .add_observer(seed_renderer)
            .add_systems(
                Update,
                handle_file
                    .in_set(PageOpenSet::HandleKnownPages)
                    .after(StoreSet::Load)
                    .after(vmux_layout::warm_page::PrewarmPageOpenSet),
            );
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn prewarm(mut commands: Commands) {
    commands.spawn(vmux_ecs::page::PrewarmPage::renderer("file://", "Code", 2));
}

fn seed_renderer(
    added: On<Add, vmux_layout::warm_page::WarmPageSpare>,
    pages: Query<&PageMetadata, Without<vmux_ecs::WorkspaceKind>>,
    mut commands: Commands,
) {
    if pages
        .get(added.entity)
        .is_ok_and(|page| page.url == "file://")
    {
        commands.entity(added.entity).insert((
            vmux_ecs::WorkspaceKind::Editor,
            vmux_ecs::FileUiStateUpdates::default(),
        ));
    }
}

type RecentVisits<'w, 's> =
    Query<'w, 's, (&'static PageMetadata, &'static LastVisitedAt), (With<Url>, Without<FileView>)>;

type ExistingViews<'w, 's> = Query<
    'w,
    's,
    (
        &'static FileView,
        &'static mut PageMetadata,
        Option<&'static vmux_ecs::WorkspaceKind>,
        Has<vmux_layout::hosted_page::SurfaceBlocked>,
        Has<vmux_layout::hosted_page::SurfaceHandoff>,
        Has<vmux_ecs::PageTransition>,
    ),
>;

#[derive(bevy::ecs::system::SystemParam)]
struct FileOpenContext<'w, 's> {
    restored: Query<'w, 's, &'static super::explorer::ArchivedEditorState>,
    workspace: vmux_layout::page_context::Context<'w, 's>,
    navigations: Query<'w, 's, &'static PageNavigation>,
    children: Query<'w, 's, &'static Children>,
    currents: Query<'w, 's, &'static vmux_layout::page_slot::Current>,
    transitions: Query<'w, 's, (), With<vmux_layout::page_slot::Transition>>,
    views: ExistingViews<'w, 's>,
    stack_workspaces: Query<'w, 's, (Option<&'static vmux_ecs::WorkspaceKind>, Has<PageRestore>)>,
    visits: RecentVisits<'w, 's>,
    projects: Projects<'w, 's>,
    registrations: Registrations<'w, 's>,
    time: Res<'w, Time>,
    commands: Commands<'w, 's>,
    record_writer: MessageWriter<'w, vmux_ecs::event::RecordVisitRequest>,
}

impl FileOpenContext<'_, '_> {
    fn code_directory(&self, stack: Entity) -> Option<PathBuf> {
        self.workspace.directory(stack)
    }

    fn reusable_view(&self, stack: Entity) -> Option<Entity> {
        if self.transitions.contains(stack) {
            return None;
        }
        if let Ok(current) = self.currents.get(stack) {
            return current
                .surface()
                .filter(|surface| self.settled_view(*surface));
        }
        let children = self.children.get(stack).ok()?;
        let mut reusable = children.iter().filter(|child| self.settled_view(*child));
        let view = reusable.next()?;
        reusable.next().is_none().then_some(view)
    }

    fn settled_view(&self, entity: Entity) -> bool {
        self.views
            .get(entity)
            .is_ok_and(|(_, _, _, blocked, handoff, transition)| {
                !blocked && !handoff && !transition
            })
    }
}

fn new_file_view_bundle(
    url: &str,
    path: PathBuf,
    workspace: vmux_ecs::WorkspaceKind,
) -> impl Bundle {
    let file = FileView { path };
    let icon = file.page_icon();
    let title = if url.starts_with("vmux://") {
        url.to_string()
    } else {
        file.page_title()
    };
    (
        (
            file,
            workspace,
            FileViewport {
                top_row: 0,
                rows: 0,
                wrap_columns: 0,
                word_wrap: vmux_api::editor::WordWrap::default(),
                word_wrap_column: 80,
                scroll_revision: 0,
            },
            ExplorerState::default(),
            Browser,
            WebviewWindowed,
            WebviewWindowedNativeFocus,
            WebviewOpaqueWindowedBackground,
            PageMetadata {
                title,
                url: url.to_string(),
                icon,
                bg_color: None,
            },
            vmux_ecs::page::HostsPage,
            vmux_ecs::page::BindsEditingChords,
            vmux_ecs::page::HostHistory::default(),
        ),
        (
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Visible,
        ),
    )
}

fn handle_file(
    tasks: Query<(Entity, &PageOpenTask, Has<PageRestore>), PendingPageOpen>,
    mut context: FileOpenContext,
    mut renderers: vmux_layout::warm_page::RendererPool,
) {
    for (entity, task, restoring) in &tasks {
        if !context
            .navigations
            .get(task.stack)
            .is_ok_and(|navigation| navigation.matches(task.id))
        {
            continue;
        }
        let knowledge_root = vmux_knowledge::KnowledgeVault::user().into_root();
        let knowledge_landing = if task.url.trim_end_matches('/')
            == vmux_knowledge::KnowledgePlugin::URL.trim_end_matches('/')
        {
            KnowledgeLanding::select(&knowledge_root, context.visits.iter())
        } else {
            None
        };
        let restored_path = context
            .restored
            .get(task.stack)
            .ok()
            .map(|state| PathBuf::from(&state.active));
        let file_landing = if CodeLanding::is_root(&task.url) {
            context
                .code_directory(task.stack)
                .map(|directory| CodeLanding::select(&directory))
        } else {
            None
        };
        let project = if ProjectRoute::is_page(&task.url) {
            match context.projects.path() {
                Ok(root) => {
                    if matches!(context.registrations.ready(root), Ok(false)) {
                        continue;
                    }
                    context.registrations.resolve(&task.url, root)
                }
                Err(error) => Some(Err(error)),
            }
        } else {
            None
        };
        let target = if let Some(project) = project {
            Some(match project {
                Ok(path) => FilePageTarget {
                    path: Some(path),
                    error: String::new(),
                },
                Err(error) => FilePageTarget { path: None, error },
            })
        } else {
            FilePageTarget::resolve(
                &task.url,
                context.projects.path(),
                &knowledge_root,
                knowledge_landing.as_deref(),
                file_landing.as_deref(),
            )
        };
        let Some(target) = target else {
            continue;
        };
        let Some(path) = target.path else {
            context.commands.entity(entity).insert(PageOpenError {
                message: target.error,
            });
            continue;
        };
        let path = if CodeLanding::requested(&task.url) {
            path
        } else {
            restored_path.unwrap_or(path)
        };
        let (persisted_workspace, stack_restoring) = context
            .stack_workspaces
            .get(task.stack)
            .unwrap_or((None, false));
        let workspace = if task.url.trim_end_matches('/')
            == vmux_knowledge::KnowledgePlugin::URL.trim_end_matches('/')
        {
            vmux_ecs::WorkspaceKind::Knowledge
        } else if restoring || stack_restoring {
            persisted_workspace.copied().unwrap_or_else(|| {
                let knowledge_root = vmux_path::PathIdentity::resolve(&knowledge_root);
                let path = vmux_path::PathIdentity::resolve(&path);
                if path.as_path().starts_with(knowledge_root.as_path()) {
                    vmux_ecs::WorkspaceKind::Knowledge
                } else {
                    vmux_ecs::WorkspaceKind::Editor
                }
            })
        } else {
            vmux_ecs::WorkspaceKind::Editor
        };
        context.commands.entity(task.stack).insert(workspace);
        let clean_url = task.url.split('#').next().unwrap_or(&task.url).to_string();
        let page_url = if CodeLanding::is_root(&clean_url)
            || context.restored.contains(task.stack)
            || clean_url.trim_end_matches('/')
                == vmux_knowledge::KnowledgePlugin::URL.trim_end_matches('/')
        {
            FileView { path: path.clone() }.url()
        } else {
            clean_url.clone()
        };
        if !path.is_dir() {
            let title = FileView { path: path.clone() }.page_title();
            context
                .record_writer
                .write(vmux_ecs::event::RecordVisitRequest {
                    url: page_url.clone(),
                    title,
                });
        }
        let pending = PendingGoto::from_url(&task.url);
        let view = match context.reusable_view(task.stack) {
            Some(view) => {
                if let Ok((file_view, mut metadata, current_workspace, _, _, _)) =
                    context.views.get_mut(view)
                {
                    if current_workspace.copied() != Some(workspace) {
                        context.commands.entity(view).insert(workspace);
                    }
                    if file_view.path != path {
                        let request = FileNavigateRequest::new(view, path, 0);
                        let request = if page_url.starts_with("vmux://") {
                            request.with_page_url(page_url.clone())
                        } else {
                            request
                        };
                        context.commands.trigger(request);
                    } else if page_url.starts_with("vmux://") {
                        metadata.title.clone_from(&page_url);
                        metadata.url = page_url.clone();
                        metadata.icon = vmux_api::PageIcon::None;
                    }
                }
                view
            }
            None => {
                let previous = context
                    .children
                    .get(task.stack)
                    .map(|children| children.iter().collect::<Vec<_>>())
                    .unwrap_or_default();
                for surface in &previous {
                    context
                        .commands
                        .entity(*surface)
                        .try_insert(vmux_layout::hosted_page::SurfaceBlocked);
                }
                let renderer = if workspace == vmux_ecs::WorkspaceKind::Editor {
                    renderers.take("file://", task.stack)
                } else {
                    None
                };
                let bundle = (
                    new_file_view_bundle(&page_url, path, workspace),
                    ChildOf(task.stack),
                );
                let view = match renderer {
                    Some(view) => {
                        context.commands.entity(view).insert(bundle);
                        view
                    }
                    None => context.commands.spawn(bundle).id(),
                };
                context.commands.entity(view).insert((
                    vmux_layout::hosted_page::SurfaceHandoff::new(
                        previous,
                        task.id,
                        0,
                        context.time.elapsed(),
                    ),
                    Visibility::Hidden,
                ));
                view
            }
        };
        let restored_stack = task.stack;
        context.commands.queue(super::explorer::RestoreEditors {
            stack: restored_stack,
            view,
        });
        if let Some(pending) = pending {
            context.commands.entity(view).insert(pending);
        }
        context.commands.entity(entity).insert(PageOpenHandled);
    }
}

struct FilePageTarget {
    path: Option<PathBuf>,
    error: String,
}

impl FilePageTarget {
    fn resolve(
        url: &str,
        projects: Result<&Path, String>,
        knowledge_root: &Path,
        knowledge_landing: Option<&Path>,
        file_landing: Option<&Path>,
    ) -> Option<Self> {
        if ProjectRoute::is_page(url) {
            return Some(match projects {
                Ok(projects) => match ProjectRoute::resolve(url, projects) {
                    Some(Ok(path)) => Self {
                        path: Some(path),
                        error: String::new(),
                    },
                    Some(Err(error)) => Self { path: None, error },
                    None => return None,
                },
                Err(error) => Self { path: None, error },
            });
        }
        if url.trim_end_matches('/') == vmux_knowledge::KnowledgePlugin::URL.trim_end_matches('/') {
            return Some(Self {
                path: Some(knowledge_landing.unwrap_or(knowledge_root).to_path_buf()),
                error: String::new(),
            });
        }
        if !url.starts_with("file:") && !CodeLanding::is_root(url) {
            return None;
        }
        if CodeLanding::is_root(url) {
            if let Some(path) = file_landing {
                return Some(Self {
                    path: Some(path.to_path_buf()),
                    error: String::new(),
                });
            }
            return Some(match projects {
                Ok(projects) => Self {
                    path: Some(projects.to_path_buf()),
                    error: String::new(),
                },
                Err(error) => Self { path: None, error },
            });
        }
        let mut path = vmux_path::FileUrl::parse(url).and_then(|file| file.path());
        if CodeLanding::requested(url)
            && let Some(directory) = path.as_ref().filter(|path| path.is_dir())
        {
            path = Some(CodeLanding::select(directory));
        }
        Some(Self {
            path,
            error: format!("malformed file URL '{url}'"),
        })
    }
}

struct CodeLanding;

impl CodeLanding {
    fn is_root(value: &str) -> bool {
        vmux_path::NavigationText::new(value).native_root() == Some("file://")
    }

    fn requested(value: &str) -> bool {
        url::Url::parse(value).is_ok_and(|url| {
            url.query_pairs()
                .any(|(key, value)| key == "vmux-entry" && value == "1")
        })
    }

    fn select(directory: &Path) -> PathBuf {
        for entry in [
            "README.md",
            "readme.md",
            "README.mdx",
            "readme.mdx",
            "README.rst",
            "README.txt",
            "README",
            "src/main.rs",
            "src/lib.rs",
            "src/main.tsx",
            "src/main.ts",
            "src/index.tsx",
            "src/index.ts",
            "src/index.js",
            "main.py",
            "main.go",
            "index.html",
        ] {
            let path = directory.join(entry);
            if path.is_file() {
                return path;
            }
        }
        directory.to_path_buf()
    }
}

struct KnowledgeLanding;

impl KnowledgeLanding {
    fn select<'a>(
        root: &Path,
        visits: impl Iterator<Item = (&'a PageMetadata, &'a LastVisitedAt)>,
    ) -> Option<PathBuf> {
        let mut recent = None;
        for (metadata, visited) in visits {
            let Some(path) = vmux_path::FileUrl::parse(&metadata.url).and_then(|url| url.path())
            else {
                continue;
            };
            if !path.starts_with(root) || !path.is_file() || !Self::is_note(&path) {
                continue;
            }
            if recent
                .as_ref()
                .is_none_or(|(_, timestamp)| visited.0 > *timestamp)
            {
                recent = Some((path, visited.0));
            }
        }
        if let Some((path, _)) = recent {
            return Some(path);
        }
        let welcome = root.join("welcome.md");
        if welcome.is_file() {
            return Some(welcome);
        }
        ignore::WalkBuilder::new(root)
            .hidden(true)
            .build()
            .filter_map(Result::ok)
            .map(|entry| entry.into_path())
            .find(|path| path.is_file() && Self::is_note(path))
    }

    fn is_note(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "md" | "markdown" | "mdx"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use vmux_ecs::PageOpenId;
    use vmux_ecs::event::FileOpenEvent;

    use super::super::explorer::{ExplorerState, TabsPlugin};
    use super::super::file_lifecycle::FileDir;
    use crate::host::navigation::NavigationPlugin;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            NavigationPlugin,
            TabsPlugin,
            PageOpenPlugin,
            vmux_layout::hosted_page::HostedPagePlugin,
        ));
        app.world_mut().spawn(crate::lsp::manager::LspManager::new(
            crate::lsp::LspDiagnosticsSender::default(),
            crate::lsp::server_request::ServerInputSender::default(),
        ));
        app
    }

    struct PageOpenFixture;

    impl PageOpenFixture {
        fn task(app: &mut App, stack: Entity, url: &str) -> PageOpenTask {
            let id = PageOpenId::new();
            app.world_mut()
                .entity_mut(stack)
                .insert(PageNavigation::new(id));
            PageOpenTask {
                id,
                stack,
                url: url.to_string(),
                request_id: None,
            }
        }

        fn spawn(app: &mut App, stack: Entity, url: &str) -> Entity {
            let task = Self::task(app, stack, url);
            app.world_mut().spawn(task).id()
        }
    }

    struct EditorStack {
        app: App,
        stack: Entity,
    }

    impl EditorStack {
        fn empty() -> Self {
            let mut app = app();
            let stack = app.world_mut().spawn_empty().id();
            Self { app, stack }
        }

        fn showing(url: &str) -> Self {
            let mut stack = Self::empty();
            stack.open(url);
            stack
        }

        fn open(&mut self, url: &str) {
            PageOpenFixture::spawn(&mut self.app, self.stack, url);
            self.app.update();
            let incoming = self
                .app
                .world()
                .get::<Children>(self.stack)
                .unwrap()
                .iter()
                .find(|child| {
                    self.app
                        .world()
                        .get::<vmux_layout::hosted_page::SurfaceHandoff>(*child)
                        .is_some()
                });
            if let Some(incoming) = incoming {
                self.app.world_mut().entity_mut(incoming).insert((
                    vmux_ecs::page::PageReady {},
                    vmux_layout::hosted_page::SurfacePaintReady,
                    vmux_layout::hosted_page::SurfacePresentationReady,
                ));
                self.app.update();
            }
            self.app.update();
        }

        fn pages(&mut self) -> Vec<Entity> {
            let mut pages = Vec::new();
            let mut query = self
                .app
                .world_mut()
                .query::<(Entity, &ChildOf, &FileView)>();
            for (entity, child_of, _) in query.iter(self.app.world()) {
                if child_of.0 == self.stack {
                    pages.push(entity);
                }
            }
            pages
        }

        fn page(&mut self) -> Entity {
            let pages = self.pages();
            assert_eq!(pages.len(), 1);
            pages[0]
        }

        fn path(&self, page: Entity) -> PathBuf {
            self.app.world().get::<FileView>(page).unwrap().path.clone()
        }

        fn goto_line(&self, page: Entity) -> Option<u32> {
            let goto = self.app.world().get::<PendingGoto>(page)?;
            Some(goto.line())
        }

        fn open_editors(&self, page: Entity) -> Vec<PathBuf> {
            self.app
                .world()
                .get::<ExplorerState>(page)
                .unwrap()
                .open_editors()
                .to_vec()
        }

        fn url(&self, page: Entity) -> String {
            self.app
                .world()
                .get::<PageMetadata>(page)
                .unwrap()
                .url
                .clone()
        }

        fn select(&mut self, page: Entity, path: &Path) {
            self.app.world_mut().trigger(UiInput {
                webview: page,
                payload: FileOpenEvent {
                    path: path.to_string_lossy().into_owned(),
                },
            });
            self.app.update();
        }
    }

    #[test]
    fn file_open_records_history_visit() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        PageOpenFixture::spawn(&mut app, stack, "file:///etc/hostname#L3");
        app.update();
        let messages = app
            .world()
            .resource::<Messages<vmux_ecs::event::RecordVisitRequest>>();
        let mut cursor = messages.get_cursor();
        let recorded: Vec<_> = cursor.read(messages).collect();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].url, "file:///etc/hostname");
        assert_eq!(recorded[0].title, "hostname");
    }

    #[test]
    fn inner_editor_close_and_reopen_keep_the_page_and_page_archive_restores_all_tabs() {
        let mut stack = EditorStack::showing("file:///tmp/first.rs");
        stack.open("file:///tmp/second.rs");
        let page = stack.page();
        let close = vmux_ecs::page::CloseItemRequest::new(page);
        stack.app.world_mut().trigger(close.clone());
        stack.app.world_mut().flush();
        stack.app.update();
        assert!(close.handled());
        assert_eq!(stack.pages(), vec![page]);
        assert_eq!(
            stack.open_editors(page),
            vec![PathBuf::from("/tmp/first.rs")]
        );
        let reopen = vmux_ecs::page::ReopenItemRequest::new(page, 0);
        stack.app.world_mut().trigger(reopen.clone());
        stack.app.world_mut().flush();
        stack.app.update();
        assert!(reopen.handled());
        assert_eq!(
            stack.open_editors(page),
            vec![
                PathBuf::from("/tmp/first.rs"),
                PathBuf::from("/tmp/second.rs")
            ]
        );
        let archive = stack.app.world_mut().spawn_empty().id();
        stack
            .app
            .world_mut()
            .trigger(vmux_ecs::page::ArchiveCaptureRequest {
                archive,
                source: page,
            });
        stack.app.world_mut().flush();
        stack.app.world_mut().entity_mut(page).despawn();
        stack
            .app
            .world_mut()
            .trigger(vmux_ecs::page::ArchiveRestoreRequest {
                stack: stack.stack,
                archive,
            });
        stack.app.world_mut().flush();
        stack.open("file:///tmp/second.rs");
        let restored = stack.page();
        assert_eq!(
            stack.open_editors(restored),
            vec![
                PathBuf::from("/tmp/first.rs"),
                PathBuf::from("/tmp/second.rs")
            ]
        );
        assert_eq!(stack.path(restored), PathBuf::from("/tmp/second.rs"));
    }

    #[test]
    fn files_root_uses_projects_only_without_a_working_directory() {
        let projects = tempfile::tempdir().unwrap();
        let knowledge = tempfile::tempdir().unwrap();

        let target =
            FilePageTarget::resolve("file://", Ok(projects.path()), knowledge.path(), None, None)
                .unwrap();

        assert_eq!(target.path.as_deref(), Some(projects.path()));
        assert!(target.error.is_empty());
    }

    #[test]
    fn code_page_opens_the_readme_in_the_owning_spaces_working_directory() {
        let working = tempfile::tempdir().unwrap();
        let readme = working.path().join("README.md");
        std::fs::write(&readme, "# Workspace\n").unwrap();
        std::fs::create_dir(working.path().join("src")).unwrap();
        std::fs::write(working.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        let mut stack = EditorStack::empty();
        let tab = stack
            .app
            .world_mut()
            .spawn(vmux_layout::tab::Tab {
                name: "Workspace".to_string(),
                startup_dir: Some(working.path().to_string_lossy().into_owned()),
            })
            .id();
        stack
            .app
            .world_mut()
            .entity_mut(stack.stack)
            .insert(ChildOf(tab));

        stack.open("file://");

        let page = stack.page();
        assert_eq!(stack.path(page), readme);
        assert_eq!(stack.url(page), FileView { path: readme }.url());
        stack.open("vmux://files/");
        let page = stack.page();
        assert_eq!(stack.path(page), working.path().join("README.md"));
    }

    #[test]
    fn code_bookmark_uses_the_spaces_project_instead_of_the_legacy_root_directory() {
        let project = tempfile::tempdir().unwrap();
        let readme = project.path().join("README.md");
        std::fs::write(&readme, "# Project\n").unwrap();
        let mut stack = EditorStack::empty();
        let space = stack
            .app
            .world_mut()
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::EffectiveStartupDir(Some(project.path().to_path_buf())),
            ))
            .id();
        let tab = stack
            .app
            .world_mut()
            .spawn((
                vmux_layout::tab::Tab {
                    name: "Project".into(),
                    startup_dir: Some("/".into()),
                },
                ChildOf(space),
            ))
            .id();
        stack
            .app
            .world_mut()
            .entity_mut(stack.stack)
            .insert(ChildOf(tab));
        stack.open("file:///");
        let page = stack.page();
        assert_eq!(stack.path(page), readme);
    }

    #[test]
    fn code_bookmark_opens_the_selected_worktrees_readme() {
        let repository = tempfile::tempdir().unwrap();
        let checkout = tempfile::tempdir().unwrap();
        std::fs::write(repository.path().join("README.md"), "# Main\n").unwrap();
        let readme = checkout.path().join("README.md");
        std::fs::write(&readme, "# Feature\n").unwrap();
        let mut stack = EditorStack::empty();
        let tab = stack
            .app
            .world_mut()
            .spawn((
                vmux_layout::tab::Tab {
                    name: "Project".into(),
                    startup_dir: Some("/".into()),
                },
                vmux_layout::tab::TabWorkspace {
                    project_dir: repository.path().to_string_lossy().into_owned(),
                },
                vmux_layout::tab::TabWorktree {
                    repo_root: repository.path().to_string_lossy().into_owned(),
                    checkout_dir: checkout.path().to_string_lossy().into_owned(),
                    branch: "feature".into(),
                    base_ref: "main".into(),
                },
            ))
            .id();
        stack
            .app
            .world_mut()
            .entity_mut(stack.stack)
            .insert(ChildOf(tab));
        stack.open("file:///");
        let page = stack.page();
        assert_eq!(stack.path(page), readme);
    }

    #[test]
    fn project_code_entry_prefers_readme_without_changing_directory_navigation() {
        let directory = tempfile::tempdir().unwrap();
        let readme = directory.path().join("README.md");
        std::fs::write(&readme, "# Project").unwrap();
        let url = vmux_path::FileUrl::from_path(directory.path(), None, None, None);
        let normal =
            FilePageTarget::resolve(&url, Ok(directory.path()), directory.path(), None, None)
                .unwrap();
        assert_eq!(normal.path.as_deref(), Some(directory.path()));
        let entry = FilePageTarget::resolve(
            &format!("{url}?vmux-entry=1"),
            Ok(directory.path()),
            directory.path(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(entry.path, Some(readme));
    }

    #[test]
    fn code_landing_uses_an_entry_file_then_the_directory_without_a_readme() {
        let working = tempfile::tempdir().unwrap();
        assert_eq!(CodeLanding::select(working.path()), working.path());
        std::fs::create_dir(working.path().join("src")).unwrap();
        let entry = working.path().join("src/main.rs");
        std::fs::write(&entry, "fn main() {}\n").unwrap();
        assert_eq!(CodeLanding::select(working.path()), entry);
    }

    #[test]
    fn claims_files_url_and_attaches_fileview() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        let task = PageOpenFixture::spawn(&mut app, stack, "file:///etc/hostname");
        app.update();
        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        let mut query = app.world_mut().query::<(&ChildOf, &FileView)>();
        let found: Vec<_> = query
            .iter(app.world())
            .filter(|(child_of, _)| child_of.0 == stack)
            .map(|(_, file)| file.path.clone())
            .collect();
        assert_eq!(found, vec![PathBuf::from("/etc/hostname")]);
    }

    #[test]
    fn opening_editor_retains_the_previous_surface_until_paint() {
        let mut app = app();
        let navigation = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn(vmux_ecs::PageNavigation::new(navigation))
            .id();
        let previous = app.world_mut().spawn((Browser, ChildOf(stack))).id();
        app.world_mut().spawn(PageOpenTask {
            id: navigation,
            stack,
            url: "file:///etc/hostname".to_string(),
            request_id: None,
        });

        app.update();

        assert!(app.world().get_entity(previous).is_ok());
        assert!(
            app.world()
                .get::<vmux_layout::hosted_page::SurfaceBlocked>(previous)
                .is_some()
        );
        let incoming = app
            .world()
            .get::<Children>(stack)
            .unwrap()
            .iter()
            .find(|child| *child != previous)
            .unwrap();
        assert!(app.world().get::<FileView>(incoming).is_some());
        assert!(
            app.world()
                .get::<vmux_layout::hosted_page::SurfaceHandoff>(incoming)
                .is_some()
        );
        assert_eq!(
            app.world().get::<Visibility>(incoming),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn ignores_non_files_url() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        let task = PageOpenFixture::spawn(&mut app, stack, "vmux://terminal/");
        app.update();
        assert!(app.world().get::<PageOpenHandled>(task).is_none());
    }

    #[test]
    fn opening_another_file_navigates_the_editor_already_in_the_stack() {
        let mut stack = EditorStack::showing("file:///etc/hostname");
        let page = stack.page();
        stack.open("file:///etc/hosts#L12");
        assert_eq!(stack.pages(), vec![page]);
        assert_eq!(stack.path(page), PathBuf::from("/etc/hosts"));
        assert_eq!(stack.goto_line(page), Some(11));
        assert_eq!(
            stack.open_editors(page),
            vec![PathBuf::from("/etc/hostname"), PathBuf::from("/etc/hosts")]
        );
    }

    #[test]
    fn knowledge_page_opens_its_landing_note() {
        let mut stack = EditorStack::showing(vmux_knowledge::KnowledgePlugin::URL);
        let page = stack.page();
        let root = vmux_knowledge::KnowledgeVault::user().into_root();
        let expected = KnowledgeLanding::select(&root, std::iter::empty()).unwrap_or(root);

        assert_eq!(
            vmux_path::FileUrl::parse(&stack.url(page)).and_then(|url| url.path()),
            Some(expected)
        );
        assert_eq!(
            stack.app.world().get::<vmux_ecs::WorkspaceKind>(page),
            Some(&vmux_ecs::WorkspaceKind::Knowledge)
        );
    }

    #[test]
    fn opening_the_same_knowledge_file_as_code_updates_its_workspace_context() {
        let mut stack = EditorStack::showing(vmux_knowledge::KnowledgePlugin::URL);
        let page = stack.page();
        let file_url = stack.url(page);

        stack.open(&file_url);

        assert_eq!(stack.pages(), vec![page]);
        assert_eq!(
            stack.app.world().get::<vmux_ecs::WorkspaceKind>(page),
            Some(&vmux_ecs::WorkspaceKind::Editor)
        );
        assert_eq!(
            stack
                .app
                .world()
                .get::<vmux_ecs::WorkspaceKind>(stack.stack),
            Some(&vmux_ecs::WorkspaceKind::Editor)
        );
    }

    #[test]
    fn restoring_a_knowledge_file_accepts_task_or_stack_restore_ownership() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.md");
        std::fs::write(&path, "# Note\n").unwrap();
        for restore_on_stack in [false, true] {
            let mut stack = EditorStack::empty();
            stack
                .app
                .world_mut()
                .entity_mut(stack.stack)
                .insert(vmux_ecs::WorkspaceKind::Knowledge);
            let url = vmux_path::FileUrl::from_path(&path, None, None, None);
            let task = PageOpenFixture::task(&mut stack.app, stack.stack, &url);
            if restore_on_stack {
                stack
                    .app
                    .world_mut()
                    .entity_mut(stack.stack)
                    .insert(PageRestore);
                stack.app.world_mut().spawn(task);
            } else {
                stack.app.world_mut().spawn((task, PageRestore));
            }

            stack.app.update();
            stack.app.update();

            let page = stack.page();
            assert_eq!(
                stack.app.world().get::<vmux_ecs::WorkspaceKind>(page),
                Some(&vmux_ecs::WorkspaceKind::Knowledge)
            );
        }
    }

    #[test]
    fn knowledge_landing_prefers_the_most_recent_note() {
        let root = tempfile::tempdir().unwrap();
        let older = root.path().join("older.md");
        let recent = root.path().join("recent.md");
        std::fs::write(&older, "older").unwrap();
        std::fs::write(&recent, "recent").unwrap();
        let older_metadata = PageMetadata {
            url: vmux_path::FileUrl::from_path(&older, None, None, None),
            ..Default::default()
        };
        let recent_metadata = PageMetadata {
            url: vmux_path::FileUrl::from_path(&recent, None, None, None),
            ..Default::default()
        };
        let older_visit = LastVisitedAt(10);
        let recent_visit = LastVisitedAt(20);

        assert_eq!(
            KnowledgeLanding::select(
                root.path(),
                [
                    (&older_metadata, &older_visit),
                    (&recent_metadata, &recent_visit),
                ]
                .into_iter(),
            ),
            Some(recent)
        );
    }

    #[test]
    fn knowledge_landing_uses_welcome_before_the_directory() {
        let root = tempfile::tempdir().unwrap();
        let welcome = root.path().join("welcome.md");
        std::fs::write(&welcome, "welcome").unwrap();

        assert_eq!(
            KnowledgeLanding::select(root.path(), std::iter::empty()),
            Some(welcome)
        );
    }

    #[test]
    fn selecting_a_note_updates_the_file_url() {
        let mut stack = EditorStack::showing(vmux_knowledge::KnowledgePlugin::URL);
        let page = stack.page();

        stack.select(page, Path::new("/tmp/note.md"));

        assert_eq!(stack.url(page), "file:///tmp/note.md");
    }

    #[test]
    fn reopening_the_file_already_shown_keeps_the_page_loaded_and_adds_no_tab() {
        let mut stack = EditorStack::showing("file:///etc/hostname");
        let page = stack.page();
        stack.app.world_mut().entity_mut(page).insert(FileDir {
            entries: Vec::new(),
        });
        stack.open("file:///etc/hostname#L7");
        assert_eq!(stack.pages(), vec![page]);
        assert!(stack.app.world().get::<FileDir>(page).is_some());
        assert_eq!(stack.goto_line(page), Some(6));
        assert_eq!(
            stack.open_editors(page),
            vec![PathBuf::from("/etc/hostname")]
        );
    }

    #[test]
    fn a_stack_holding_no_editor_page_retires_its_surface_after_the_new_one_paints() {
        let mut stack = EditorStack::empty();
        let occupant = stack.app.world_mut().spawn(ChildOf(stack.stack)).id();
        stack.open("file:///etc/hostname");
        assert!(stack.app.world().get_entity(occupant).is_err());
        let page = stack.page();
        assert_eq!(stack.path(page), PathBuf::from("/etc/hostname"));
    }
}
