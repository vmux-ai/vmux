use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::PageMetadata;
use vmux_ecs::event::{FileErrorEvent, FileOpenEvent, KnowledgeLinkOpen, RecordVisitRequest};

use super::media::FileMedia;
use crate::edit::Selection;
use crate::host::editor::{
    Editor, FileDocumentRevision, FileNavigateRequest, FileView, ParkedEdit, ParkedEdits,
};
use crate::host::file_lifecycle::{FileBuffer, FileDir};
use crate::host::note::NoteRevealLine;
use crate::host::note::NoteSent;
use crate::host::status::FileInitialMetaSent;
use crate::host::viewport::{CursorRenderRequest, FileViewport, ViewportRenderRequest};
use crate::lsp::manager::LspLine;

pub(crate) struct NavigationPlugin;

impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<crate::lsp::manager::LspGoto>()
            .add_message::<RecordVisitRequest>()
            .add_observer(file_open)
            .add_observer(preview_checkout)
            .add_observer(knowledge_link_open)
            .add_observer(apply_file)
            .add_systems(Update, (refresh_checkout, apply_goto, apply_pending_goto));
    }
}

#[derive(Component, Clone, PartialEq)]
pub(crate) struct CheckoutPreview(pub(crate) vmux_ecs::event::FileCheckoutState);

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct CheckoutContext<'w, 's> {
    info: Query<'w, 's, &'static vmux_git::RepoInfoCache>,
    hierarchy: vmux_layout::tab::TabHierarchy<'w, 's>,
    tabs: Query<
        'w,
        's,
        (
            Ref<'static, vmux_layout::tab::Tab>,
            Option<&'static vmux_layout::tab::TabWorktree>,
            Option<&'static vmux_layout::tab::TabWorkspace>,
        ),
    >,
}

impl CheckoutContext<'_, '_> {
    pub(crate) fn preview(
        &self,
        entity: Entity,
        path: &Path,
    ) -> Option<vmux_ecs::event::FileCheckoutState> {
        let tab = self.hierarchy.entity(entity)?;
        let (tab, worktree, workspace) = self.tabs.get(tab).ok()?;
        let cwd = PathBuf::from(tab.startup_dir.as_ref()?);
        let repository = worktree
            .map(|worktree| PathBuf::from(&worktree.repo_root))
            .or_else(|| workspace.map(|workspace| PathBuf::from(&workspace.project_dir)))?;
        let cwd = vmux_path::PathIdentity::resolve(&cwd);
        let repository = vmux_path::PathIdentity::resolve(&repository);
        let file = vmux_path::PathIdentity::resolve(path);
        let known_project = self
            .info
            .single()
            .ok()
            .and_then(|info| info.cached_project_root(file.as_path()));
        if !file.as_path().starts_with(repository.as_path())
            && known_project.as_deref() != Some(repository.as_path())
        {
            return None;
        }
        let checkout = file
            .as_path()
            .ancestors()
            .find(|path| path.join(".git").exists())?;
        let current = cwd
            .as_path()
            .ancestors()
            .find(|path| path.join(".git").exists())?;
        if checkout == current {
            return None;
        }
        Some(vmux_ecs::event::FileCheckoutState {
            path: path.to_string_lossy().into_owned(),
            repository: repository.as_path().to_string_lossy().into_owned(),
            checkout: checkout.to_string_lossy().into_owned(),
            read_only: true,
        })
    }

    fn is_current(&self, entity: Entity, checkout: &Path) -> bool {
        let Some(cwd) = self.hierarchy.startup_dir(entity) else {
            return false;
        };
        let cwd = vmux_path::PathIdentity::resolve(Path::new(&cwd));
        let checkout = vmux_path::PathIdentity::resolve(checkout);
        cwd.as_path()
            .ancestors()
            .find(|path| path.join(".git").exists())
            == Some(checkout.as_path())
    }

    fn changed(&self, entity: Entity) -> bool {
        self.hierarchy
            .entity(entity)
            .and_then(|tab| self.tabs.get(tab).ok())
            .is_some_and(|(tab, _, _)| tab.is_changed())
    }
}

fn refresh_checkout(
    views: Query<(Entity, Ref<FileView>, Option<&CheckoutPreview>)>,
    context: CheckoutContext,
    mut commands: Commands,
) {
    for (entity, view, previous) in &views {
        if !view.is_changed() && !context.changed(entity) {
            continue;
        }
        let retained = previous.filter(|preview| {
            view.path.starts_with(&preview.0.checkout)
                && !context.is_current(entity, Path::new(&preview.0.checkout))
        });
        let next = if let Some(preview) = retained {
            let mut state = preview.0.clone();
            state.path = view.path.to_string_lossy().into_owned();
            Some(state)
        } else {
            context.preview(entity, &view.path)
        };
        if previous.map(|preview| &preview.0) == next.as_ref() {
            continue;
        }
        match next {
            Some(state) => {
                commands
                    .entity(entity)
                    .insert(CheckoutPreview(state.clone()));
                commands.trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &state));
            }
            None => {
                commands.entity(entity).remove::<CheckoutPreview>();
                commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
                    entity,
                    &vmux_ecs::event::FileCheckoutState {
                        path: view.path.to_string_lossy().into_owned(),
                        ..Default::default()
                    },
                ));
            }
        }
    }
}

fn preview_checkout(
    input: On<UiInput<vmux_ecs::event::FileCheckoutPreviewRequest>>,
    context: CheckoutContext,
    mut commands: Commands,
) {
    let entity = input.event().webview;
    let request = &input.event().payload;
    let path = vmux_path::PathIdentity::resolve(Path::new(&request.path));
    let checkout = vmux_path::PathIdentity::resolve(Path::new(&request.checkout));
    if !path.as_path().starts_with(checkout.as_path()) {
        return;
    }
    if !context.is_current(entity, checkout.as_path()) {
        let state = vmux_ecs::event::FileCheckoutState {
            path: path.as_path().to_string_lossy().into_owned(),
            repository: request.repository.clone(),
            checkout: checkout.as_path().to_string_lossy().into_owned(),
            read_only: true,
        };
        commands
            .entity(entity)
            .insert(CheckoutPreview(state.clone()));
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &state));
    }
    commands.trigger(FileNavigateRequest::new(entity, path.into_path_buf(), 0));
}

#[derive(Component)]
pub(super) struct PendingGoto {
    line: u32,
    utf16_col: u32,
    select_end_col: Option<u32>,
}

impl PendingGoto {
    #[cfg(test)]
    pub(super) fn line(&self) -> u32 {
        self.line
    }

    pub(super) fn from_url(url: &str) -> Option<Self> {
        let body = url.split_once('#')?.1.strip_prefix('L')?;
        let (line, selection) = match body.split_once(':') {
            Some((line, selection)) => (line, Some(selection)),
            None => (body, None),
        };
        let line = line.parse::<u32>().ok()?.saturating_sub(1);
        let (utf16_col, select_end_col) = match selection.and_then(|value| value.split_once('-')) {
            Some((start, end)) => (start.parse().unwrap_or(0), end.parse::<u32>().ok()),
            None => (0, None),
        };
        Some(Self {
            line,
            utf16_col,
            select_end_col,
        })
    }

    pub(super) fn selection(line: u32, utf16_col: u32, select_end_col: u32) -> Self {
        Self {
            line,
            utf16_col,
            select_end_col: Some(select_end_col),
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct GotoActions<'w, 's> {
    browsers: Option<NonSend<'w, Browsers>>,
    commands: Commands<'w, 's>,
}

impl GotoActions<'_, '_> {
    fn caret(
        &mut self,
        entity: Entity,
        edit: &mut Editor,
        line: u32,
        utf16_col: u32,
        viewport: &mut FileViewport,
    ) {
        let Some(browsers) = self.browsers.as_deref() else {
            return;
        };
        let line = (line as usize).min(edit.core.buffer.len_lines().saturating_sub(1));
        let line_text = edit
            .core
            .buffer
            .rope
            .line(line)
            .chars()
            .filter(|character| *character != '\n' && *character != '\r')
            .collect::<String>();
        let character_column = LspLine::new(line_text).char_col(utf16_col);
        let caret = edit
            .core
            .buffer
            .coords_to_char(line, character_column as usize);
        edit.core.set_caret(caret);
        if let Some(top) = viewport.autoscroll(edit)
            && let Some(scroll) = viewport.set_top(top)
            && browsers.can_emit_to(&entity)
        {
            self.commands
                .trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &scroll));
        }
        edit.core.top_row = viewport.top_row;
    }
}

fn file_open(
    trigger: On<UiInput<FileOpenEvent>>,
    mut visits: FileVisitRecorder,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    visits.record(&path);
    commands.trigger(FileNavigateRequest::new(entity, path, 0));
}

#[derive(bevy::ecs::system::SystemParam)]
struct FileVisitRecorder<'w> {
    visits: MessageWriter<'w, RecordVisitRequest>,
}

impl FileVisitRecorder<'_> {
    fn record(&mut self, path: &Path) {
        if !path.is_file() {
            return;
        }
        let title = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        self.visits.write(RecordVisitRequest {
            url: vmux_path::FileUrl::from_path(path, None, None, None),
            title,
        });
    }
}

fn apply_file(
    trigger: On<FileNavigateRequest>,
    mut views: Query<(
        &mut FileView,
        &mut FileDocumentRevision,
        &mut FileViewport,
        &mut PageMetadata,
    )>,
    mut lsp_closes: Option<MessageWriter<crate::lsp::manager::LspDocumentCloseRequest>>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut commands: Commands,
) {
    let request = trigger.event();
    let Ok((mut view, mut revision, mut viewport, mut metadata)) = views.get_mut(request.entity)
    else {
        return;
    };
    let previous = view.replace_path(request.path.clone(), &mut revision);
    if let Some(lsp_closes) = lsp_closes.as_mut() {
        lsp_closes.write(crate::lsp::manager::LspDocumentCloseRequest {
            path: previous.clone(),
        });
    }
    metadata.title = view.page_title();
    metadata.icon = view.page_icon();
    metadata.url = view.url();
    if let Some(page_url) = &request.page_url {
        metadata.title.clone_from(page_url);
        metadata.url.clone_from(page_url);
        metadata.icon = vmux_api::PageIcon::None;
    }
    viewport.top_row = request.top_line;
    let entity = request.entity;
    let requested = request.path.clone();
    let resumed_keymap = super::keymap::KeymapConfig::resolve(settings.as_deref()).keymap();
    commands
        .entity(entity)
        .remove::<FileDir>()
        .remove::<FileBuffer>()
        .remove::<FileMedia>()
        .remove::<crate::host::file_lifecycle::FileLoadTask>()
        .remove::<crate::host::keymap::EditorKeymap>()
        .remove::<NoteSent>()
        .remove::<crate::host::language::LspEditDirty>()
        .remove::<FileInitialMetaSent>()
        .remove::<crate::lsp::manager::LspOpened>()
        .remove::<crate::lsp::manager::LintRan>();
    commands.queue(move |world: &mut World| {
        let Ok(mut document) = world.get_entity_mut(entity) else {
            return;
        };
        if document
            .get::<FileView>()
            .is_none_or(|view| view.path != requested)
        {
            return;
        }
        let mut edits = document.take::<ParkedEdits>().unwrap_or_default();
        if document.contains::<Editor>() && document.contains::<vmux_git::GitDiffSource>() {
            let edit = document.take::<Editor>().unwrap();
            let diff = document.take::<vmux_git::GitDiffSource>().unwrap();
            let actual_previous = edit.core.buffer.path.clone();
            let modified = ParkedEdits::modified_at(&actual_previous);
            edits.insert(
                actual_previous,
                ParkedEdit {
                    edit,
                    diff,
                    modified,
                },
            );
        }
        let forced = document
            .get::<super::file_lifecycle::ForcedEncoding>()
            .is_some_and(|encoding| encoding.path == requested);
        let resumed = if forced {
            None
        } else {
            edits.resume(&requested)
        };
        document.insert(edits);
        if let Some(resumed) = resumed {
            document.insert((resumed.edit, resumed.diff, resumed_keymap));
            if super::markdown::ParsedNote::supports(&requested) {
                document.insert(super::explorer::OutlineDirty);
            }
            world.trigger(super::status::DocumentRenderRequest::new(entity));
        }
    });
}

fn knowledge_link_open(
    trigger: On<UiInput<KnowledgeLinkOpen>>,
    mut goto: MessageWriter<crate::lsp::manager::LspGoto>,
    mut visits: FileVisitRecorder,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    let root = vmux_knowledge::KnowledgeVault::user().into_root();
    let requested = PathBuf::from(&request.path);
    let path = if request.create {
        let Ok(relative) = requested.strip_prefix(&root) else {
            return;
        };
        if requested.exists() {
            let Ok(canonical_root) = root.canonicalize() else {
                return;
            };
            let Ok(metadata) = std::fs::symlink_metadata(&requested) else {
                return;
            };
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return;
            }
            let Ok(path) = requested.canonicalize() else {
                return;
            };
            if !path.starts_with(canonical_root) {
                return;
            }
            path
        } else {
            let relative = relative.to_string_lossy();
            match vmux_knowledge::KnowledgeVault::user().write_note(
                Some(&relative),
                &request.title,
                &format!("# {}", request.title),
            ) {
                Ok(path) => path,
                Err(error) => {
                    if browsers.can_emit_to(&entity) {
                        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
                            entity,
                            &FileErrorEvent {
                                message: error,
                                undecodable: false,
                            },
                        ));
                    }
                    return;
                }
            }
        }
    } else {
        let Ok(root) = root.canonicalize() else {
            return;
        };
        if std::fs::symlink_metadata(&requested)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return;
        }
        let Ok(path) = requested.canonicalize() else {
            return;
        };
        if !path.starts_with(root) {
            return;
        }
        path
    };
    if let Some(line) = request.line {
        commands.entity(entity).insert(NoteRevealLine(line));
    }
    visits.record(&path);
    goto.write(crate::lsp::manager::LspGoto {
        entity,
        path,
        line: request.line.unwrap_or(0),
        utf16_col: 0,
    });
}

#[allow(clippy::type_complexity)]
fn apply_goto(
    mut messages: MessageReader<crate::lsp::manager::LspGoto>,
    mut views: Query<(
        &mut Editor,
        &mut FileViewport,
        &mut FileView,
        &mut PageMetadata,
    )>,
    mut lsp_closes: Option<MessageWriter<crate::lsp::manager::LspDocumentCloseRequest>>,
    mut actions: GotoActions,
) {
    if actions.browsers.is_none() {
        return;
    }
    for goto in messages.read() {
        let Ok((mut edit, mut viewport, mut view, mut metadata)) = views.get_mut(goto.entity)
        else {
            continue;
        };
        if vmux_path::PathIdentity::resolve(&view.path)
            == vmux_path::PathIdentity::resolve(&goto.path)
        {
            actions.caret(
                goto.entity,
                &mut edit,
                goto.line,
                goto.utf16_col,
                &mut viewport,
            );
            actions
                .commands
                .trigger(ViewportRenderRequest::new(goto.entity));
            actions
                .commands
                .trigger(CursorRenderRequest::new(goto.entity));
            continue;
        }
        if let Some(lsp_closes) = lsp_closes.as_mut() {
            lsp_closes.write(crate::lsp::manager::LspDocumentCloseRequest {
                path: view.path.clone(),
            });
        }
        metadata.title = goto
            .path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        metadata.url = url::Url::from_file_path(&goto.path)
            .map(|url| url.to_string())
            .unwrap_or_else(|_| format!("file://{}", goto.path.to_string_lossy()));
        view.path = goto.path.clone();
        viewport.top_row = 0;
        actions
            .commands
            .entity(goto.entity)
            .remove::<Editor>()
            .remove::<vmux_git::GitDiffSource>()
            .remove::<FileBuffer>()
            .remove::<FileMedia>()
            .remove::<FileDir>()
            .remove::<NoteSent>()
            .remove::<FileInitialMetaSent>()
            .remove::<crate::lsp::manager::LspOpened>()
            .remove::<crate::lsp::manager::LintRan>()
            .insert(PendingGoto {
                line: goto.line,
                utf16_col: goto.utf16_col,
                select_end_col: None,
            });
    }
}

fn apply_pending_goto(
    mut views: Query<(Entity, &mut Editor, &mut FileViewport, &PendingGoto)>,
    mut actions: GotoActions,
) {
    if actions.browsers.is_none() {
        return;
    }
    for (entity, mut edit, mut viewport, pending) in &mut views {
        actions.caret(
            entity,
            &mut edit,
            pending.line,
            pending.utf16_col,
            &mut viewport,
        );
        if let Some(end) = pending.select_end_col {
            let line = (pending.line as usize).min(edit.core.buffer.len_lines().saturating_sub(1));
            let line_text = edit
                .core
                .buffer
                .rope
                .line(line)
                .chars()
                .filter(|character| *character != '\n' && *character != '\r')
                .collect::<String>();
            let line_text = LspLine::new(line_text);
            let start = line_text.char_col(pending.utf16_col) as usize;
            let end = line_text.char_col(end) as usize;
            let anchor = edit.core.buffer.coords_to_char(line, start);
            let head = edit.core.buffer.coords_to_char(line, end);
            edit.core.selections = vec![Selection { anchor, head }];
        }
        actions.commands.trigger(ViewportRenderRequest::new(entity));
        actions.commands.trigger(CursorRenderRequest::new(entity));
        actions.commands.entity(entity).remove::<PendingGoto>();
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::edit::EditCore;
    use crate::edit::highlight_cache::HighlightCache;
    use crate::keymap::EditorKeymap;
    use crate::keymap::KeymapKindExt;
    use vmux_api::BinEvent;
    use vmux_ecs::event::FileUiState;

    #[derive(Resource, Default)]
    struct Emitted(Vec<FileUiState>);

    struct GotoSession {
        app: App,
        view: Entity,
    }

    impl GotoSession {
        fn scrolled_to_the_top_of(path: &Path) -> Self {
            let text = (0..600)
                .map(|index| format!("let v{index} = {index};\n"))
                .collect::<String>();
            let core = EditCore::new(
                path.to_path_buf(),
                "Rust".into(),
                &text,
                crate::edit::EditMode::Normal,
            );
            let edit = Editor::new(
                core,
                HighlightCache::new(path),
                crate::fold::FoldState::default(),
            );
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<crate::lsp::manager::LspGoto>()
                .add_plugins((
                    NavigationPlugin,
                    vmux_ecs::UiStatePlugin::<FileUiState>::default(),
                ))
                .init_resource::<Emitted>()
                .add_observer(
                    |trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Emitted>| {
                        if trigger.event().id() != FileUiState::id() {
                            return;
                        }
                        trigger.event().mark_delivered();
                        let frame = rkyv::from_bytes::<
                            vmux_api::UiStateFrame<
                                FileUiState,
                                <FileUiState as vmux_api::UiState>::Update,
                            >,
                            rkyv::rancor::Error,
                        >(trigger.event().payload())
                        .unwrap();
                        let event = match frame {
                            vmux_api::UiStateFrame::Snapshot(state) => state,
                            vmux_api::UiStateFrame::Updates(updates) => {
                                <FileUiState as vmux_api::UiState>::from_updates(
                                    emitted.0.last().cloned(),
                                    updates,
                                )
                            }
                        };
                        emitted.0.push(event);
                    },
                );
            app.world_mut().spawn(crate::lsp::manager::LspManager::new(
                crate::lsp::LspDiagnosticsSender::default(),
                crate::lsp::server_request::ServerInputSender::default(),
            ));
            let view = app
                .world_mut()
                .spawn((
                    edit,
                    FileView {
                        path: path.to_path_buf(),
                    },
                    FileViewport {
                        top_row: 0,
                        rows: 40,
                        wrap_columns: 0,
                        word_wrap: vmux_api::editor::WordWrap::Off,
                        word_wrap_column: 80,
                        scroll_revision: 0,
                    },
                    EditorKeymap(vmux_api::editor::KeymapKind::Vscode.make(&[], "\\")),
                    bevy_cef::prelude::BinIpcSurfaceGeneration(1),
                    bevy_cef::prelude::BinIpcNavigationRevision(1),
                    bevy_cef::prelude::BinIpcEpoch(1),
                    PageMetadata {
                        title: String::new(),
                        url: "file:///tmp/goto.rs".to_string(),
                        icon: vmux_api::PageIcon::None,
                        bg_color: None,
                    },
                ))
                .id();
            let mut browsers = Browsers::default();
            browsers.set_externally_hosted(view);
            app.world_mut().insert_non_send(browsers);
            app.world_mut().spawn(vmux_api::UiEventPermissions {
                url: "file:///tmp/goto.rs",
                owns_subtree: false,
                permissions: &["FileUiState"],
            });
            Self { app, view }
        }

        fn goto(&mut self, line: u32) {
            let entity = self.view;
            let path = self
                .app
                .world()
                .get::<FileView>(entity)
                .expect("a file view")
                .path
                .clone();
            self.app
                .world_mut()
                .write_message(crate::lsp::manager::LspGoto {
                    entity,
                    path,
                    line,
                    utf16_col: 0,
                });
            self.app.update();
        }
    }

    #[test]
    fn goto_fragment_carries_line_and_selection() {
        let goto = PendingGoto::from_url("file:///a/b.rs#L10").unwrap();
        assert_eq!(
            (goto.line, goto.utf16_col, goto.select_end_col),
            (9, 0, None)
        );
        let goto = PendingGoto::from_url("file:///a/b.rs#L10:5-12").unwrap();
        assert_eq!(
            (goto.line, goto.utf16_col, goto.select_end_col),
            (9, 5, Some(12))
        );
        assert!(PendingGoto::from_url("file:///a/b.rs").is_none());
        assert!(PendingGoto::from_url("file:///a/b.rs#x").is_none());
    }

    #[test]
    fn a_jump_that_moves_the_viewport_tells_the_page_where_it_went() {
        let mut session = GotoSession::scrolled_to_the_top_of(Path::new("/tmp/goto.rs"));
        session.goto(500);

        let top = session
            .app
            .world()
            .get::<FileViewport>(session.view)
            .expect("a viewport")
            .top_row;
        assert!(top > 0, "jumping to line 500 has to move the window");
        assert!(
            session
                .app
                .world()
                .resource::<Emitted>()
                .0
                .iter()
                .any(|state| state.viewport.scroll_by.is_some()),
            "the window was repainted at row {top}, so a page still parked at row 0 \
             would render the band off screen unless the move is announced: {:?}",
            session.app.world().resource::<Emitted>().0
        );
    }
}
