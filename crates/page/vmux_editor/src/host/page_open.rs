use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_core::page_open::{PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};
use vmux_flex::prelude::*;
use vmux_layout::Browser;

use super::editing::{FileView, PendingGoto};
use super::explorer::ExplorerState;
use super::viewport::FileViewport;

pub(super) struct EditorPageOpenPlugin;

impl Plugin for EditorPageOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_core::event::RecordVisitRequest>()
            .add_systems(
                Update,
                handle_file_page_open.in_set(PageOpenSet::HandleKnownPages),
            );
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);
type NavigableFileView = (
    &'static mut FileView,
    &'static mut FileViewport,
    &'static mut PageMetadata,
);

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
                word_wrap: vmux_core::editor::WordWrap::default(),
                word_wrap_column: 80,
            },
            ExplorerState::default(),
            Browser,
            WebviewWindowed,
            WebviewWindowedNativeFocus,
            WebviewOpaqueWindowedBackground,
            PageMetadata {
                title,
                url: url.to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            vmux_core::host::page::HostsPage,
            vmux_core::host::page::BindsEditingChords,
            vmux_core::host::page::HostHistory::default(),
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

pub fn restore_file_view_bundle(url: &str) -> Option<impl Bundle> {
    let path = vmux_core::file_url::FileUrl::parse(url)?.path()?;
    Some(new_file_view_bundle(url, path))
}

fn handle_file_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children: Query<&Children>,
    mut views: Query<NavigableFileView>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    effective_startup_dir: Option<Res<vmux_layout::settings::EffectiveStartupDir>>,
    mut commands: Commands,
    mut record_writer: MessageWriter<vmux_core::event::RecordVisitRequest>,
) {
    for (entity, task) in &tasks {
        let project_dir = effective_startup_dir
            .as_deref()
            .and_then(|effective| effective.0.as_ref())
            .and_then(|(_, path)| path.as_deref());
        let knowledge_root = vmux_core::knowledge::KnowledgeVault::user().into_root();
        let Some(target) = FilePageTarget::resolve(&task.url, project_dir, &knowledge_root) else {
            continue;
        };
        let Some(path) = target.path else {
            commands.entity(entity).insert(PageOpenError {
                message: target.error,
            });
            continue;
        };
        let clean_url = task.url.split('#').next().unwrap_or(&task.url).to_string();
        let page_url = if clean_url.trim_end_matches('/')
            == vmux_core::knowledge::KNOWLEDGE_PAGE_URL.trim_end_matches('/')
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
            record_writer.write(vmux_core::event::RecordVisitRequest {
                url: clean_url.clone(),
                title,
            });
        }
        let pending = PendingGoto::from_url(&task.url);
        let view = match FileView::in_stack(task.stack, &children, &views) {
            Some(view) => {
                if let Ok((mut file_view, mut viewport, mut metadata)) = views.get_mut(view)
                    && file_view.path != path
                {
                    file_view.navigate(
                        view,
                        path,
                        0,
                        &mut viewport,
                        &mut metadata,
                        &mut manager,
                        &mut commands,
                    );
                }
                if let Ok((_, _, mut metadata)) = views.get_mut(view)
                    && page_url.starts_with("vmux://")
                {
                    metadata.title.clone_from(&page_url);
                    metadata.url = page_url.clone();
                    metadata.icon = vmux_core::PageIcon::None;
                }
                view
            }
            None => {
                vmux_layout::stack::Stack::clear_children(task.stack, &children, &mut commands);
                commands
                    .spawn((new_file_view_bundle(&page_url, path), ChildOf(task.stack)))
                    .id()
            }
        };
        if let Some(pending) = pending {
            commands.entity(view).insert(pending);
        }
        commands.entity(entity).insert(PageOpenHandled);
    }
}

struct FilePageTarget {
    path: Option<PathBuf>,
    error: String,
}

impl FilePageTarget {
    fn resolve(url: &str, project_dir: Option<&Path>, knowledge_root: &Path) -> Option<Self> {
        if url.trim_end_matches('/') == vmux_api::space::PROJECTS_PAGE_URL.trim_end_matches('/') {
            return Some(Self {
                path: Some(
                    project_dir
                        .map(Path::to_path_buf)
                        .unwrap_or_else(vmux_core::profile::projects_dir),
                ),
                error: String::new(),
            });
        }
        if url.trim_end_matches('/')
            == vmux_core::knowledge::KNOWLEDGE_PAGE_URL.trim_end_matches('/')
        {
            return Some(Self {
                path: Some(knowledge_root.to_path_buf()),
                error: String::new(),
            });
        }
        if !url.starts_with("file:") {
            return None;
        }
        Some(Self {
            path: vmux_core::file_url::FileUrl::parse(url).and_then(|file| file.path()),
            error: format!("malformed file URL '{url}'"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use vmux_core::PageOpenId;
    use vmux_core::event::FileOpenEvent;

    use super::super::explorer::ExplorerState;
    use super::super::explorer::ExplorerTabsPlugin;
    use super::super::file_lifecycle::FileDir;
    use crate::navigation::EditorNavigationPlugin;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            EditorNavigationPlugin,
            ExplorerTabsPlugin,
            EditorPageOpenPlugin,
        ))
        .insert_resource(crate::lsp::manager::LspManager::new(
            crate::lsp::LspOutbox::default(),
            crate::lsp::server_request::ServerEvents::default().sender(),
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
                .open_editors
                .clone()
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
            self.app.world_mut().trigger(BinReceive {
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
            .resource::<Messages<vmux_core::event::RecordVisitRequest>>();
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
    fn knowledge_page_redirects_to_its_directory() {
        let mut stack = EditorStack::showing(vmux_core::knowledge::KNOWLEDGE_PAGE_URL);
        let page = stack.page();

        assert_eq!(
            vmux_core::file_url::FileUrl::parse(&stack.url(page)).and_then(|url| url.path()),
            Some(vmux_core::knowledge::KnowledgeVault::user().into_root())
        );
    }

    #[test]
    fn selecting_a_note_updates_the_file_url() {
        let mut stack = EditorStack::showing(vmux_core::knowledge::KNOWLEDGE_PAGE_URL);
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
