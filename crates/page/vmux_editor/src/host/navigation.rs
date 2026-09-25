use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_core::event::{FileErrorEvent, FileOpenEvent, KnowledgeLinkOpen};

use crate::edit::Selection;
use crate::host::editor::{
    Editor, FileDocumentRevision, FileNavigateRequest, FileView, ParkedEdit, ParkedEdits,
};
use crate::host::file_lifecycle::{FileBuffer, FileDir, canon};
use crate::host::note::NoteRevealLine;
use crate::host::note::NoteSent;
use crate::host::status::FileInitialMetaSent;
use crate::host::viewport::{CursorRenderRequest, FileViewport, ViewportRenderRequest};
use crate::media::FileMedia;

pub(crate) struct NavigationPlugin;

impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<crate::lsp::manager::LspGoto>()
            .add_observer(on_file_open)
            .add_observer(on_knowledge_link_open)
            .add_observer(apply_file_navigation)
            .add_systems(Update, (apply_goto, apply_pending_goto));
    }
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

fn on_file_open(trigger: On<UiInput<FileOpenEvent>>, mut commands: Commands) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    commands.trigger(FileNavigateRequest::new(entity, path, 0));
}

fn apply_file_navigation(
    trigger: On<FileNavigateRequest>,
    mut views: Query<(
        &mut FileView,
        &mut FileDocumentRevision,
        &mut FileViewport,
        &mut PageMetadata,
    )>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    let request = trigger.event();
    let Ok((mut view, mut revision, mut viewport, mut metadata)) = views.get_mut(request.entity)
    else {
        return;
    };
    let previous = view.replace_path(request.path.clone(), &mut revision);
    manager.close(&previous);
    metadata.title = view
        .path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| view.path.to_string_lossy().to_string());
    metadata.url = view.url();
    if let Some(page_url) = &request.page_url {
        metadata.title.clone_from(page_url);
        metadata.url.clone_from(page_url);
        metadata.icon = vmux_core::PageIcon::None;
    }
    viewport.top_row = request.top_line;
    let entity = request.entity;
    commands.queue(move |world: &mut World| {
        let Ok(mut entity) = world.get_entity_mut(entity) else {
            return;
        };
        if !entity.contains::<Editor>() || !entity.contains::<vmux_git::GitDiffSource>() {
            return;
        }
        let Some(edit) = entity.take::<Editor>() else {
            return;
        };
        let Some(diff) = entity.take::<vmux_git::GitDiffSource>() else {
            return;
        };
        let parked = ParkedEdit {
            edit,
            diff,
            modified: ParkedEdits::modified_at(&previous),
        };
        let mut edits = entity.take::<ParkedEdits>().unwrap_or_default();
        edits.insert(previous, parked);
        entity.insert(edits);
    });
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
}

fn on_knowledge_link_open(
    trigger: On<UiInput<KnowledgeLinkOpen>>,
    mut goto: MessageWriter<crate::lsp::manager::LspGoto>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    let root = vmux_core::knowledge::KnowledgeVault::user().into_root();
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
            match vmux_core::knowledge::KnowledgeVault::user().write_note(
                Some(&relative),
                &request.title,
                &format!("# {}", request.title),
            ) {
                Ok(path) => path,
                Err(error) => {
                    if browsers.can_emit_to(&entity) {
                        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
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
    goto.write(crate::lsp::manager::LspGoto {
        entity,
        path,
        line: request.line.unwrap_or(0),
        utf16_col: 0,
    });
}

fn goto_caret(
    entity: Entity,
    edit: &mut Editor,
    line: u32,
    utf16_col: u32,
    viewport: &mut FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    let line = (line as usize).min(edit.core.buffer.len_lines().saturating_sub(1));
    let line_text = edit
        .core
        .buffer
        .rope
        .line(line)
        .chars()
        .filter(|character| *character != '\n' && *character != '\r')
        .collect::<String>();
    let character_column = crate::lsp::manager::utf16_to_char_col(&line_text, utf16_col);
    let caret = edit
        .core
        .buffer
        .coords_to_char(line, character_column as usize);
    edit.core.set_caret(caret);
    if let Some(top) = viewport.autoscroll(edit) {
        if let Some(scroll) = viewport.set_top(top)
            && browsers.can_emit_to(&entity)
        {
            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                entity, &scroll,
            ));
        }
        edit.core.top_row = viewport.top_row;
    }
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
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let Some(browsers) = browsers.as_deref() else {
        return;
    };
    for goto in messages.read() {
        let Ok((mut edit, mut viewport, mut view, mut metadata)) = views.get_mut(goto.entity)
        else {
            continue;
        };
        if canon(&view.path) == canon(&goto.path) {
            goto_caret(
                goto.entity,
                &mut edit,
                goto.line,
                goto.utf16_col,
                &mut viewport,
                browsers,
                &mut commands,
            );
            commands.trigger(ViewportRenderRequest::new(goto.entity));
            commands.trigger(CursorRenderRequest::new(goto.entity));
            continue;
        }
        manager.close(&view.path);
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
        commands
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
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let Some(browsers) = browsers.as_deref() else {
        return;
    };
    for (entity, mut edit, mut viewport, pending) in &mut views {
        goto_caret(
            entity,
            &mut edit,
            pending.line,
            pending.utf16_col,
            &mut viewport,
            browsers,
            &mut commands,
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
            let start =
                crate::lsp::manager::utf16_to_char_col(&line_text, pending.utf16_col) as usize;
            let end = crate::lsp::manager::utf16_to_char_col(&line_text, end) as usize;
            let anchor = edit.core.buffer.coords_to_char(line, start);
            let head = edit.core.buffer.coords_to_char(line, end);
            edit.core.selections = vec![Selection { anchor, head }];
        }
        commands.trigger(ViewportRenderRequest::new(entity));
        commands.trigger(CursorRenderRequest::new(entity));
        commands.entity(entity).remove::<PendingGoto>();
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
    use vmux_core::event::{FileUiState, FileUiStatePatch};

    #[derive(Resource, Default)]
    struct Emitted(Vec<FileUiStatePatch>);

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
                    vmux_core::host::UiStatePlugin::<vmux_core::event::FileUiState>::default(),
                ))
                .init_resource::<Emitted>()
                .add_observer(
                    |trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Emitted>| {
                        if trigger.event().id() != FileUiState::id() {
                            return;
                        }
                        let event = rkyv::from_bytes::<FileUiState, rkyv::rancor::Error>(
                            trigger.event().payload(),
                        )
                        .unwrap();
                        emitted.0.extend(event.patches);
                    },
                );
            app.world_mut()
                .insert_resource(crate::lsp::manager::LspManager::new(
                    crate::lsp::LspOutbox::default(),
                    crate::lsp::server_request::ServerEvents::default().sender(),
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
                        word_wrap: vmux_core::editor::WordWrap::Off,
                        word_wrap_column: 80,
                        scroll_revision: 0,
                    },
                    EditorKeymap(vmux_core::editor::KeymapKind::Vscode.make(&[], "\\")),
                    PageMetadata {
                        title: String::new(),
                        url: String::new(),
                        icon: vmux_core::PageIcon::None,
                        bg_color: None,
                    },
                ))
                .id();
            let mut browsers = Browsers::default();
            browsers.set_externally_hosted(view);
            app.world_mut().insert_non_send(browsers);
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
                .any(|patch| matches!(patch, FileUiStatePatch::ScrollBy(_))),
            "the window was repainted at row {top}, so a page still parked at row 0 \
             would render the band off screen unless the move is announced: {:?}",
            session.app.world().resource::<Emitted>().0
        );
    }
}
