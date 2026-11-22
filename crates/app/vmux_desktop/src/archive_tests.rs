use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::{
    BinHostEmitEvent, BinIpcEpoch, BinIpcNavigationRevision, BinIpcSurfaceGeneration, Browsers,
    HostWindow, UiInput,
};
use vmux_api::input::KeyStroke;
use vmux_ecs::event::{FileResizeEvent, FileUiState};
use vmux_ecs::{ArchivedPage, PageMetadata, UiState};
use vmux_layout::{
    pane::Pane,
    space::{Space, SpaceId},
    stack::Stack,
    tab::Tab,
};

struct ArchiveEditorFixture {
    app: App,
    directory: tempfile::TempDir,
    stack: Entity,
    window: Entity,
}

impl ArchiveEditorFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("archive.txt");
        std::fs::write(&path, "original\n").unwrap();
        let mut settings = vmux_setting::AppSettings::default();
        settings.editor.keymap = vmux_api::editor::KeymapKind::Vim;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(ArchiveTestHarnessPlugin)
            .add_message::<vmux_setting::ColorSchemeChanged>()
            .insert_resource(settings)
            .insert_non_send(Browsers::default())
            .insert_resource(vmux_ecs::BlockingRuntime::new(
                vmux_ecs::BlockingPool::new("archive-files", 1, 1),
                vmux_ecs::BlockingPool::new("archive-long", 0, 0),
                vmux_ecs::BlockingPool::new("archive-process", 0, 0),
            ))
            .add_plugins(vmux_layout::LayoutRuntimePlugin::manifest().plugin())
            .add_plugins(vmux_layout::tool::LayoutToolPlugin)
            .add_plugins(vmux_app::project::ProjectPlugin)
            .add_plugins(vmux_app::lsp::LspPlugin)
            .add_plugins((
                vmux_layout::archive::ArchivePlugin,
                vmux_layout::stack::StackPlugin,
                vmux_layout::presentation::PresentationPlugin,
                vmux_layout::hosted_page::HostedPagePlugin,
                vmux_editor::EditorPlugin,
            ));
        app.world_mut().spawn(vmux_api::UiEventPermissions {
            url: "file://",
            owns_subtree: true,
            permissions: &["FileUiState"],
        });
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let space = app
            .world_mut()
            .spawn((
                Space,
                SpaceId("archive-test".into()),
                HostWindow(window),
                vmux_ecs::Active,
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                ChildOf(space),
                vmux_ecs::LastActivatedAt::now(),
            ))
            .id();
        let pane = app
            .world_mut()
            .spawn((
                Pane::bundle(),
                ChildOf(tab),
                HostWindow(window),
                vmux_ecs::LastActivatedAt::now(),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::bundle(),
                ChildOf(pane),
                vmux_ecs::LastActivatedAt::now(),
            ))
            .id();
        let id = vmux_ecs::PageOpenId::new();
        app.world_mut()
            .entity_mut(stack)
            .insert(vmux_ecs::PageNavigation::new(id));
        app.world_mut().spawn(vmux_ecs::page_open::PageOpenTask {
            id,
            stack,
            url: url::Url::from_file_path(path).unwrap().into(),
            request_id: None,
        });
        Self {
            app,
            directory,
            stack,
            window,
        }
    }

    fn view(&mut self) -> Option<Entity> {
        let mut views = self
            .app
            .world_mut()
            .query::<(Entity, &vmux_editor::FileView)>();
        views.iter(self.app.world()).find_map(|(view, file)| {
            if !file.path.starts_with(self.directory.path()) {
                return None;
            }
            let mut current = view;
            let mut belongs = false;
            while let Some(parent) = self.app.world().get::<ChildOf>(current) {
                if self
                    .app
                    .world()
                    .get::<vmux_layout::presentation::Retiring>(current)
                    .is_some()
                {
                    return None;
                }
                current = parent.parent();
                if current == self.stack {
                    belongs = true;
                }
            }
            belongs.then_some(view)
        })
    }

    fn ready(&mut self) -> Entity {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut viewport_height = 320.0;
        loop {
            self.app.update();
            if let Some(view) = self.view() {
                self.app
                    .world_mut()
                    .non_send_mut::<Browsers>()
                    .set_externally_hosted(view);
                self.app.world_mut().entity_mut(view).insert((
                    BinIpcSurfaceGeneration(1),
                    BinIpcNavigationRevision(1),
                    BinIpcEpoch(1),
                    vmux_ecs::page::PageReady {},
                    vmux_layout::hosted_page::SurfacePaintReady,
                    vmux_layout::hosted_page::SurfacePresentationReady,
                ));
                self.input(
                    view,
                    FileResizeEvent {
                        char_height: 16.0,
                        viewport_height,
                        wrap_columns: 80,
                    },
                );
                viewport_height = if viewport_height == 320.0 {
                    336.0
                } else {
                    320.0
                };
                if self.text(view).is_some() {
                    return view;
                }
            }
            assert!(
                Instant::now() < deadline,
                "production editor did not publish its document"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn input<T: Send + Sync + 'static>(&mut self, view: Entity, payload: T) {
        self.app.world_mut().trigger(UiInput {
            webview: view,
            payload,
        });
        self.app.update();
    }

    fn key(&mut self, view: Entity, key: &str) {
        self.input(
            view,
            KeyStroke {
                key: key.into(),
                ..Default::default()
            },
        );
    }

    fn text(&self, view: Entity) -> Option<String> {
        let state = self
            .app
            .world()
            .get::<UiState<FileUiState>>(view)?
            .current()?;
        let content = state.viewport.content.as_ref()?;
        let mut text = String::new();
        for line in &content.lines {
            for span in &line.spans {
                text.push_str(&span.text);
            }
            text.push('\n');
        }
        Some(text)
    }

    fn edit(&mut self, view: Entity) {
        self.key(view, "y");
        self.key(view, "y");
        self.key(view, "p");
        self.key(view, "0");
        self.key(view, "l");
        self.key(view, "v");
        self.key(view, "l");
        self.key(view, "l");
        self.key(view, "l");
    }

    fn close(&mut self) -> Entity {
        self.app
            .world_mut()
            .write_message(vmux_layout::CloseStackRequest::by_user(self.stack));
        self.app.update();
        self.app
            .world_mut()
            .query_filtered::<Entity, With<ArchivedPage>>()
            .single(self.app.world())
            .unwrap()
    }

    fn reopen(&mut self) -> usize {
        let mut cursor = self
            .app
            .world()
            .resource::<Messages<vmux_ecs::PageOpenRequest>>()
            .get_cursor();
        cursor
            .read(
                self.app
                    .world()
                    .resource::<Messages<vmux_ecs::PageOpenRequest>>(),
            )
            .count();
        self.app
            .world_mut()
            .write_message(vmux_command::CommandInvocation::new(
                self.window,
                "stack_reopen",
            ));
        self.app.update();
        self.app.update();
        let requests = cursor
            .read(
                self.app
                    .world()
                    .resource::<Messages<vmux_ecs::PageOpenRequest>>(),
            )
            .cloned()
            .collect::<Vec<_>>();
        let count = requests.len();
        for request in requests {
            let vmux_ecs::PageOpenTarget::Stack(stack) = request.target else {
                panic!("archive must emit an explicit Stack target");
            };
            assert_eq!(
                request.url,
                url::Url::from_file_path(self.directory.path().join("archive.txt"))
                    .unwrap()
                    .to_string()
            );
            assert!(self.app.world().get::<Stack>(stack).is_some());
            assert_eq!(
                self.app.world().get::<PageMetadata>(stack).unwrap().url,
                request.url
            );
            self.stack = stack;
            let id = vmux_ecs::PageOpenId::new();
            self.app
                .world_mut()
                .entity_mut(stack)
                .insert(vmux_ecs::PageNavigation::new(id));
            let restore = self
                .app
                .world_mut()
                .entity_mut(stack)
                .take::<vmux_ecs::store::PageRestore>();
            let mut task = self
                .app
                .world_mut()
                .spawn(vmux_ecs::page_open::PageOpenTask {
                    id,
                    stack,
                    url: request.url,
                    request_id: request.request_id,
                });
            if let Some(restore) = restore {
                task.insert(restore);
            }
        }
        self.app.update();
        count
    }
}

struct ArchiveTestHarnessPlugin;

impl Plugin for ArchiveTestHarnessPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(deliver).add_observer(refuse);
    }
}

fn deliver(event: On<BinHostEmitEvent>) {
    event.event().mark_delivered();
}

#[test]
fn archive_reopen_preserves_unsaved_text_selection_and_undo() {
    let mut fixture = ArchiveEditorFixture::new();
    let view = fixture.ready();
    let initial = fixture.text(view).unwrap();
    fixture.edit(view);
    let edited = fixture.text(view).unwrap();
    assert!(edited.starts_with("original\noriginal\n"));
    let selection = fixture
        .app
        .world()
        .get::<UiState<FileUiState>>(view)
        .unwrap()
        .current()
        .unwrap()
        .viewport
        .cursor
        .as_ref()
        .unwrap()
        .source_selections
        .clone();
    assert!(!selection.is_empty());
    let archive = fixture.close();
    assert!(fixture.app.world().get::<ArchivedPage>(archive).is_some());
    fixture.reopen();
    let reopened = fixture.ready();
    assert!(fixture.app.world().get_entity(archive).is_err());
    assert_eq!(fixture.text(reopened).unwrap(), edited);
    let restored = fixture
        .app
        .world()
        .get::<UiState<FileUiState>>(reopened)
        .unwrap()
        .current()
        .unwrap();
    assert_eq!(
        restored.viewport.cursor.as_ref().unwrap().source_selections,
        selection
    );
    fixture.key(reopened, "Escape");
    fixture.key(reopened, "u");
    assert_eq!(fixture.text(reopened).unwrap(), initial);
    assert_eq!(
        std::fs::read_to_string(fixture.directory.path().join("archive.txt")).unwrap(),
        "original\n"
    );
}

#[derive(Resource)]
struct RefuseWindow(Option<Entity>);

fn refuse(
    added: On<Add, vmux_ecs::Operation>,
    operations: Query<&vmux_ecs::OperationKind>,
    target: Option<ResMut<RefuseWindow>>,
    mut commands: Commands,
) {
    let Some(mut target) = target else {
        return;
    };
    if operations
        .get(added.entity)
        .is_ok_and(|kind| kind.as_str() == "archive.restore")
        && let Some(window) = target.0.take()
    {
        commands.entity(window).despawn();
    }
}

#[test]
fn retired_restore_window_preserves_the_real_archived_editor_content() {
    let mut fixture = ArchiveEditorFixture::new();
    let view = fixture.ready();
    fixture.edit(view);
    let edited = fixture.text(view).unwrap();
    let archive = fixture.close();
    fixture
        .app
        .insert_resource(RefuseWindow(Some(fixture.window)));
    assert_eq!(fixture.reopen(), 0);
    assert!(fixture.app.world().get::<ArchivedPage>(archive).is_some());
    fixture.window = fixture
        .app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    let space = fixture
        .app
        .world_mut()
        .query_filtered::<Entity, With<Space>>()
        .single(fixture.app.world())
        .unwrap();
    fixture
        .app
        .world_mut()
        .entity_mut(space)
        .insert(HostWindow(fixture.window));
    assert_eq!(fixture.reopen(), 1);
    let reopened = fixture.ready();
    assert_eq!(fixture.text(reopened).unwrap(), edited);
    let metadata = fixture.app.world().get::<PageMetadata>(reopened).unwrap();
    assert!(metadata.url.starts_with("file:"));
}
