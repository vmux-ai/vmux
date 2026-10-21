use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::page_open::{PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};
use vmux_ecs::profile::Projects;
use vmux_ecs::{LastVisitedAt, PageMetadata, Url};
use vmux_flex::prelude::*;
use vmux_layout::Browser;

use crate::project::ProjectRoute;

use super::editor::{FileNavigateRequest, FileView};
use super::explorer::ExplorerState;
use super::navigation::PendingGoto;
use super::viewport::FileViewport;

pub(super) struct PageOpenPlugin;

impl Plugin for PageOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_ecs::event::RecordVisitRequest>()
            .add_systems(Update, handle_file.in_set(PageOpenSet::HandleKnownPages));
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);
type RecentVisits<'w, 's> =
    Query<'w, 's, (&'static PageMetadata, &'static LastVisitedAt), (With<Url>, Without<FileView>)>;

#[derive(bevy::ecs::system::SystemParam)]
struct FileOpenContext<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    views: Query<'w, 's, (&'static FileView, &'static mut PageMetadata)>,
    visits: RecentVisits<'w, 's>,
    projects: Projects<'w, 's>,
    commands: Commands<'w, 's>,
    record_writer: MessageWriter<'w, vmux_ecs::event::RecordVisitRequest>,
}

fn new_file_view_bundle(url: &str, path: PathBuf) -> impl Bundle {
    let title = if url.starts_with("vmux://") {
        url.to_string()
    } else {
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string())
    };
    (
        (
            FileView { path },
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
                icon: vmux_api::PageIcon::None,
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
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    mut context: FileOpenContext,
) {
    for (entity, task) in &tasks {
        let knowledge_root = vmux_knowledge::KnowledgeVault::user().into_root();
        let knowledge_landing = if task.url.trim_end_matches('/')
            == vmux_knowledge::KnowledgePlugin::URL.trim_end_matches('/')
        {
            KnowledgeLanding::select(&knowledge_root, context.visits.iter())
        } else {
            None
        };
        let Some(target) = FilePageTarget::resolve(
            &task.url,
            context.projects.path(),
            &knowledge_root,
            knowledge_landing.as_deref(),
        ) else {
            continue;
        };
        let Some(path) = target.path else {
            context.commands.entity(entity).insert(PageOpenError {
                message: target.error,
            });
            continue;
        };
        let clean_url = task.url.split('#').next().unwrap_or(&task.url).to_string();
        let page_url = if clean_url.trim_end_matches('/')
            == vmux_knowledge::KnowledgePlugin::URL.trim_end_matches('/')
        {
            FileView { path: path.clone() }.url()
        } else {
            clean_url.clone()
        };
        if !path.is_dir() {
            let title = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            context
                .record_writer
                .write(vmux_ecs::event::RecordVisitRequest {
                    url: page_url.clone(),
                    title,
                });
        }
        let pending = PendingGoto::from_url(&task.url);
        let view = match context
            .children
            .get(task.stack)
            .ok()
            .and_then(|children| children.iter().find(|&child| context.views.contains(child)))
        {
            Some(view) => {
                if let Ok((file_view, mut metadata)) = context.views.get_mut(view) {
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
                context.commands.entity(task.stack).despawn_children();
                context
                    .commands
                    .spawn((new_file_view_bundle(&page_url, path), ChildOf(task.stack)))
                    .id()
            }
        };
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
        if !url.starts_with("file:") {
            return None;
        }
        Some(Self {
            path: vmux_path::FileUrl::parse(url).and_then(|file| file.path()),
            error: format!("malformed file URL '{url}'"),
        })
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
        app.add_plugins((MinimalPlugins, NavigationPlugin, TabsPlugin, PageOpenPlugin));
        app.world_mut().spawn(crate::lsp::manager::LspManager::new(
            crate::lsp::LspDiagnosticsSender::default(),
            crate::lsp::server_request::ServerInputSender::default(),
        ));
        app
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
            self.app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack: self.stack,
                url: url.to_string(),
                request_id: None,
            });
            self.app.update();
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
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "file:///etc/hostname#L3".to_string(),
            request_id: None,
        });
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
        let mut query = app.world_mut().query::<(&ChildOf, &FileView)>();
        let found: Vec<_> = query
            .iter(app.world())
            .filter(|(child_of, _)| child_of.0 == stack)
            .map(|(_, file)| file.path.clone())
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
    fn a_stack_holding_no_editor_page_gets_a_fresh_one() {
        let mut stack = EditorStack::empty();
        let occupant = stack.app.world_mut().spawn(ChildOf(stack.stack)).id();
        stack.open("file:///etc/hostname");
        assert!(stack.app.world().get_entity(occupant).is_err());
        let page = stack.page();
        assert_eq!(stack.path(page), PathBuf::from("/etc/hostname"));
    }
}
