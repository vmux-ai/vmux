use crate::event::{
    DocumentFindInput, DocumentKeyInput, DocumentPointerInput, DocumentPropertyInput,
    DocumentTextInput,
};
use crate::host::editor::Editor;
use crate::host::editor::{Edit, EditReadOnly};
use bevy::prelude::*;
use bevy::prelude::{
    App, Commands, Component, Entity, EntityEvent, Message, MessageReader, MessageWriter, On,
    Plugin, Query, Res, With,
};
use bevy_cef::prelude::*;
use vmux_api::command_bar::ExRequest;
use vmux_api::editor::KeymapKind;
use vmux_api::input::KeyStroke;
use vmux_command::CommandInvocation;
use vmux_command::{CommandBarDismiss, CommandBarOpenRequest, WriteCommandBarRequests};
use vmux_command::{KeyCombo, KeyContext, Keymap};
use vmux_ecs::event::*;
use vmux_setting::AppSettings;

use crate::edit::{EditCommand, Motion, Selection};
use crate::host::explorer::{OpenEditorsDirty, OutlineDirty};
use crate::host::file_lifecycle::SelfWrites;
use crate::host::fold::FoldState;
use crate::host::keymap::{EditorKeymap, KeymapConfig};
use crate::host::language::{LspEditDirty, WikiCompletionRequest};
use crate::host::markdown::ParsedNote;
use crate::host::note::NoteSent;
#[cfg(test)]
use crate::host::status::SharedFileViewMode;
use crate::host::viewport::{CursorRenderRequest, FileViewport, FoldsDirty, ViewportRenderRequest};
use crate::keymap::{KeyInput, Mods};
use vmux_ecs::scroll::ScrollWindow;

pub(crate) mod document;
pub(crate) mod effects;
pub(crate) mod input;
mod projection;

pub(super) struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EditExecutionPlugin)
            .add_message::<CommandBarOpenRequest>()
            .add_plugins(UiEventPlugin::<(
                FileOpenEvent,
                DocumentTextInput,
                DocumentKeyInput,
                DocumentPointerInput,
                FileCheckoutPreviewRequest,
            )>::default())
            .add_plugins(UiEventPlugin::<(
                KnowledgeLinkOpen,
                DocumentPropertyInput,
                DocumentFindInput,
                ExRequest,
            )>::default())
            .add_message::<ExLineSubmitted>()
            .add_message::<OpenExRequest>()
            .add_systems(Update, open_ex.in_set(WriteCommandBarRequests))
            .add_observer(file_key)
            .add_observer(document_key)
            .add_observer(file_text_input)
            .add_observer(file_pointer)
            .add_systems(
                Update,
                (
                    reapply_keymap_on_change,
                    run_submitted_ex_lines.in_set(super::file_writes::FileWriteSet),
                ),
            )
            .add_observer(open_file_find)
            .add_observer(close_file_find)
            .add_observer(file_find_request)
            .add_observer(file_property_edit)
            .add_observer(submit_ex);
    }
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct OpenExRequest;

impl TryFrom<&CommandInvocation> for OpenExRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "browser_open_ex_bar")
            .then_some(Self)
            .ok_or(())
    }
}

#[derive(Message, Clone)]
pub(super) struct ExLineSubmitted {
    pub(super) stack: Option<Entity>,
    pub(super) line: String,
}

fn open_ex(
    mut requests: MessageReader<OpenExRequest>,
    mut open: MessageWriter<CommandBarOpenRequest>,
) {
    if requests.read().count() > 0 {
        open.write(CommandBarOpenRequest::query(":"));
    }
}

fn submit_ex(
    trigger: On<UiInput<ExRequest>>,
    focus: vmux_layout::stack::FocusedStack,
    mut submitted: MessageWriter<ExLineSubmitted>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    submitted.write(ExLineSubmitted {
        stack: focus.stack,
        line: trigger.event().payload.line.clone(),
    });
    commands.trigger(CommandBarDismiss::new(webview, true));
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileFindState {
    open: bool,
    forward: bool,
    query: String,
    regex: bool,
    revision: u64,
}

impl Default for FileFindState {
    fn default() -> Self {
        Self {
            open: false,
            forward: true,
            query: String::new(),
            regex: false,
            revision: 0,
        }
    }
}

#[derive(EntityEvent)]
pub(crate) struct FileFindOpenRequest {
    #[event_target]
    entity: Entity,
    forward: bool,
}

impl FileFindOpenRequest {
    pub(crate) fn new(entity: Entity, forward: bool) -> Self {
        Self { entity, forward }
    }
}

#[derive(EntityEvent)]
struct FileFindCloseRequest {
    #[event_target]
    entity: Entity,
}

impl From<Entity> for FileFindCloseRequest {
    fn from(entity: Entity) -> Self {
        Self { entity }
    }
}

fn open_file_find(
    trigger: On<FileFindOpenRequest>,
    settings: Option<Res<AppSettings>>,
    mut states: Query<&mut FileFindState>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(mut state) = states.get_mut(entity) else {
        return;
    };
    state.open = true;
    state.forward = trigger.event().forward;
    state.query.clear();
    state.regex = settings
        .as_deref()
        .is_some_and(|settings| settings.editor.keymap == KeymapKind::Vim);
    state.revision = state.revision.wrapping_add(1).max(1);
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        entity,
        &FileFindEvent {
            open: state.open,
            forward: state.forward,
            query: state.query.clone(),
            regex: state.regex,
            revision: state.revision,
        },
    ));
}

fn close_file_find(
    trigger: On<FileFindCloseRequest>,
    mut states: Query<&mut FileFindState>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(mut state) = states.get_mut(entity) else {
        return;
    };
    state.open = false;
    state.query.clear();
    state.revision = state.revision.wrapping_add(1).max(1);
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        entity,
        &FileFindEvent {
            open: state.open,
            forward: state.forward,
            query: state.query.clone(),
            regex: state.regex,
            revision: state.revision,
        },
    ));
}

pub(super) struct EditExecutionPlugin;

impl Plugin for EditExecutionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((document::DocumentPlugin, input::InputPlugin))
            .insert_non_send(ClipboardHandle(arboard::Clipboard::new().ok()))
            .add_plugins((projection::ProjectionPlugin, effects::EffectsPlugin));
    }
}

pub(super) struct ClipboardHandle(pub(super) Option<arboard::Clipboard>);

#[cfg(test)]
pub(super) struct EditRequest {
    entity: Entity,
    commands: Vec<EditCommand>,
}

#[cfg(test)]
impl EditRequest {
    #[cfg(test)]
    pub(super) fn emit(self, document: uuid::Uuid, commands: &mut Commands) {
        commands.trigger(input::ParsedFixtureRequest {
            entity: self.entity,
            document,
            commands: self.commands,
        });
    }

    #[cfg(test)]
    pub(super) fn emit_world(self, world: &mut World) {
        let Some(revision) = world.get::<crate::edit::document::Revision>(self.entity) else {
            return;
        };
        let document = revision.identity;
        self.emit(document, &mut world.commands());
        world.flush();
    }

    pub(super) fn new(entity: Entity, commands: Vec<EditCommand>) -> Self {
        Self { entity, commands }
    }

    fn accelerated_navigation(mut self, repeat: bool) -> Self {
        let input = input::KeyInputRequest {
            entity: self.entity,
            document: uuid::Uuid::nil(),
            input: KeyInput {
                key: String::new(),
                mods: Mods::default(),
                repeat,
            },
        };
        self.commands = input::SyntaxInput::accelerated(&input, self.commands);
        self
    }

    fn remapped_for_note(mut self, blocks: &[NoteBlock], start_line: u32) -> Self {
        let input = input::KeyInputRequest {
            entity: self.entity,
            document: uuid::Uuid::nil(),
            input: KeyInput {
                key: String::new(),
                mods: Mods::default(),
                repeat: false,
            },
        };
        self.commands = input::SyntaxInput::remapped(&input, self.commands, blocks, start_line);
        self
    }
}

fn reapply_keymap_on_change(
    settings: Option<Res<AppSettings>>,
    mut q: Query<(
        Entity,
        Edit,
        &mut EditorKeymap,
        &mut crate::keymap::vim::VimKeymap,
        Option<&FileViewport>,
    )>,
    mut commands: Commands,
) {
    let next = KeymapConfig::resolve(settings.as_deref());
    for (entity, mut edit, mut keymap, mut parser, viewport) in &mut q {
        if parser.applied_config.as_ref() == Some(&next) && keymap.0 == next.kind() {
            continue;
        }
        let previous = parser
            .applied_config
            .as_ref()
            .map_or(keymap.0, KeymapConfig::kind);
        *keymap = next.keymap();
        *parser = next.parser();
        if previous != next.kind() {
            edit.core.set_mode(next.initial_mode());
        }
        if viewport.is_some() {
            commands.trigger(CursorRenderRequest::new(entity));
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct PreviewAccess<'w, 's> {
    checkout: crate::host::navigation::CheckoutContext<'w, 's>,
    previews: Query<'w, 's, (), With<crate::host::navigation::CheckoutPreview>>,
}
impl PreviewAccess<'_, '_> {
    fn read_only(&self, entity: Entity, path: &std::path::Path) -> bool {
        self.previews.contains(entity) || self.checkout.preview(entity, path).is_some()
    }
}

fn file_key(
    trigger: On<UiInput<KeyStroke>>,
    documents: Query<&crate::edit::document::Revision, (With<Editor>, With<EditorKeymap>)>,
    app_keymaps: Query<&Keymap>,
    app_contexts: Query<&KeyContext>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    if let (Ok(keymap), Ok(context), Some(pressed)) = (
        app_keymaps.single(),
        app_contexts.get(entity),
        KeyCombo::from_stroke(evt),
    ) && keymap.in_context(context).scoped(&pressed).is_some()
    {
        return;
    }
    let Ok(revision) = documents.get(entity) else {
        return;
    };
    commands.trigger(input::KeyInputRequest {
        entity,
        document: revision.identity,
        input: KeyInput {
            key: evt.key.clone(),
            mods: Mods {
                ctrl: evt.mods.ctrl,
                alt: evt.mods.alt,
                shift: evt.mods.shift,
                meta: evt.mods.super_key,
            },
            repeat: evt.repeat,
        },
    });
}

fn file_text_input(
    trigger: On<UiInput<DocumentTextInput>>,
    q: Query<(EditReadOnly, &crate::host::editor::FileDocumentRevision)>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let text = trigger.event().payload.text.clone();
    if text.is_empty() {
        return;
    }
    let Ok((edit, revision)) = q.get(entity) else {
        return;
    };
    if revision.get() != trigger.event().payload.revision {
        return;
    }
    if !edit.core.mode.value.accepts_text() {
        return;
    }
    commands.trigger(document::CommittedTextRequest {
        entity,
        document: Some(edit.core.revision.identity),
        text,
    });
}

fn file_property_edit(
    trigger: On<UiInput<DocumentPropertyInput>>,
    q: Query<(EditReadOnly, &crate::host::editor::FileDocumentRevision)>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((edit, revision)) = q.get(entity) else {
        return;
    };
    if revision.get() != trigger.event().payload.revision {
        return;
    }
    if !ParsedNote::supports(&edit.core.buffer.path) {
        return;
    }
    let text = edit.core.buffer.text();
    let updated = match vmux_knowledge::Frontmatter::from(text.as_str())
        .apply(&trigger.event().payload.edit)
    {
        Ok(updated) => updated,
        Err(message) => {
            commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
                entity,
                &FileErrorEvent {
                    message,
                    undecodable: false,
                },
            ));
            return;
        }
    };
    if updated == text {
        return;
    }
    commands.trigger(document::ReplaceTextRequest {
        entity,
        document: Some(edit.core.revision.identity),
        text: updated,
    });
}

fn run_submitted_ex_lines(
    mut submitted: MessageReader<ExLineSubmitted>,
    children: Query<&Children>,
    q: Query<&crate::edit::document::Revision, (With<Editor>, With<EditorKeymap>)>,
    mut commands: Commands,
) {
    for message in submitted.read() {
        let Some(stack) = message.stack else {
            continue;
        };
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(entity) = kids.iter().find(|child| q.contains(*child)) else {
            continue;
        };
        let Ok(revision) = q.get(entity) else {
            continue;
        };
        commands.trigger(input::ExInputRequest {
            entity,
            document: revision.identity,
            line: message.line.clone(),
        });
    }
}

fn file_find_request(
    trigger: On<UiInput<DocumentFindInput>>,
    mut q: Query<(
        Edit,
        &mut FileFindState,
        &crate::host::editor::FileDocumentRevision,
    )>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload.clone();
    let Ok((mut edit, mut state, revision)) = q.get_mut(entity) else {
        return;
    };
    if revision.get() != request.revision {
        return;
    }
    let changed = state.query != request.query
        || state.regex != request.regex
        || state.forward != request.forward;
    state.query.clone_from(&request.query);
    state.regex = request.regex;
    state.forward = request.forward;
    if request.done || request.query.is_empty() {
        edit.core.clear_search();
        if request.done {
            commands.trigger(FileFindCloseRequest::from(entity));
        }
    } else if request.step {
        edit.core.move_caret(Motion::SearchNext {
            reverse: request.reverse,
        });
    } else {
        let pattern = match request.regex {
            true => crate::edit::search::Search::translate(&request.query),
            false => regex::escape(&request.query),
        };
        edit.core.set_search(pattern, request.forward);
    }
    if changed && !request.done {
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
            entity,
            &FileFindEvent {
                open: state.open,
                forward: state.forward,
                query: state.query.clone(),
                regex: state.regex,
                revision: state.revision,
            },
        ));
    }
    commands.trigger(CursorRenderRequest::new(entity));
}

fn file_pointer(
    trigger: On<UiInput<DocumentPointerInput>>,
    mut q: Query<(
        Edit,
        &EditorKeymap,
        &mut crate::keymap::vim::VimKeymap,
        &crate::host::editor::FileDocumentRevision,
    )>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let p = trigger.event().payload;
    let Ok((mut edit, keymap, mut parser, revision)) = q.get_mut(entity) else {
        return;
    };
    if revision.get() != p.revision {
        return;
    }
    let col = edit.core.char_at_cell(p.line as usize, p.col);
    let at = edit.core.buffer.coords_to_char(p.line as usize, col);
    if p.add {
        edit.core.toggle_caret(at);
    } else if p.extend {
        let anchor = edit.core.primary().anchor;
        edit.core.carets.selections = vec![Selection { anchor, head: at }];
    } else {
        edit.core.collapse_carets();
        edit.core.set_caret(at);
    }
    if let Some(EditCommand::SetMode(mode)) = match keymap.0 {
        KeymapKind::Vim => parser.pointer(edit.core.mode.value, p.extend),
        KeymapKind::Vscode => None,
    } {
        edit.core.set_mode(mode);
    }
    if ParsedNote::supports(&edit.core.buffer.path) {
        commands.entity(entity).remove::<NoteSent>();
    }
    commands.trigger(CursorRenderRequest::new(entity));
}

#[cfg(test)]
mod edit_flow_tests {
    use super::*;
    use crate::keymap::{KeyInput, Mods};

    #[test]
    fn find_requests_update_the_targeted_editor_state() {
        let mut app = App::new();
        app.add_observer(open_file_find)
            .add_observer(close_file_find);
        let first = app.world_mut().spawn(FileFindState::default()).id();
        let second = app.world_mut().spawn(FileFindState::default()).id();

        app.world_mut()
            .trigger(FileFindOpenRequest::new(first, false));

        assert_eq!(
            app.world().get::<FileFindState>(first).unwrap(),
            &FileFindState {
                open: true,
                forward: false,
                query: String::new(),
                regex: false,
                revision: 1,
            }
        );
        assert_eq!(
            app.world().get::<FileFindState>(second).unwrap(),
            &FileFindState::default()
        );

        app.world_mut().trigger(FileFindCloseRequest::from(first));

        assert_eq!(
            app.world().get::<FileFindState>(first).unwrap(),
            &FileFindState {
                open: false,
                forward: false,
                query: String::new(),
                regex: false,
                revision: 2,
            }
        );
    }

    #[test]
    fn vim_dd_deletes_line_via_keymap_and_core() {
        let mut app = App::new();
        app.add_plugins((document::DocumentPlugin, input::InputPlugin));
        let entity = app
            .world_mut()
            .spawn(crate::edit::Document::new(
                std::path::PathBuf::from("a.txt"),
                "Plain Text".into(),
                "one\ntwo\nthree\n",
                crate::edit::EditMode::Normal,
            ))
            .id();
        for key in ["d", "d"] {
            let mode = app
                .world()
                .get::<crate::edit::document::Mode>(entity)
                .unwrap()
                .value;
            let commands = app
                .world_mut()
                .get_mut::<crate::keymap::vim::VimKeymap>(entity)
                .unwrap()
                .decode(
                    mode,
                    &KeyInput {
                        key: key.into(),
                        mods: Mods::default(),
                        repeat: false,
                    },
                )
                .0;
            EditRequest::new(entity, commands).emit_world(app.world_mut());
        }
        assert_eq!(
            app.world()
                .get::<crate::edit::buffer::TextBuffer>(entity)
                .unwrap()
                .text(),
            "two\nthree\n"
        );
    }

    #[test]
    fn vscode_typing_inserts_and_marks_dirty() {
        let mut app = App::new();
        app.add_plugins((document::DocumentPlugin, input::InputPlugin));
        let entity = app
            .world_mut()
            .spawn(crate::edit::Document::new(
                std::path::PathBuf::from("a.txt"),
                "Plain Text".into(),
                "",
                crate::edit::EditMode::Insert,
            ))
            .id();
        EditRequest::new(entity, vec![EditCommand::InsertText("hello".into())])
            .emit_world(app.world_mut());
        assert_eq!(
            app.world()
                .get::<crate::edit::buffer::TextBuffer>(entity)
                .unwrap()
                .text(),
            "hello"
        );
        assert!(
            app.world()
                .get::<crate::edit::document::Revision>(entity)
                .unwrap()
                .dirty
        );
    }

    #[test]
    fn repeated_navigation_advances_two_steps_without_accelerating_edits() {
        assert_eq!(
            EditRequest::new(Entity::PLACEHOLDER, vec![EditCommand::Move(Motion::Down)])
                .accelerated_navigation(true)
                .commands,
            [
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down)
            ]
        );
        assert_eq!(
            EditRequest::new(Entity::PLACEHOLDER, vec![EditCommand::DeleteBack])
                .accelerated_navigation(true)
                .commands,
            [EditCommand::DeleteBack]
        );
    }

    #[test]
    fn a_held_scroll_key_covers_the_same_ground_as_a_held_motion_key() {
        let held = |command| {
            EditRequest::new(Entity::PLACEHOLDER, vec![command])
                .accelerated_navigation(true)
                .commands
        };
        let rows = |cmds: Vec<EditCommand>| {
            cmds.iter()
                .map(|cmd| match cmd {
                    EditCommand::ScrollViewport(lines) => *lines,
                    EditCommand::Move(Motion::Down) => 1,
                    EditCommand::Move(Motion::Up) => -1,
                    other => panic!("unexpected {other:?}"),
                })
                .sum::<i32>()
        };

        assert_eq!(
            rows(held(EditCommand::ScrollViewport(1))),
            rows(held(EditCommand::Move(Motion::Down)))
        );
        assert_eq!(
            rows(held(EditCommand::ScrollViewport(-1))),
            rows(held(EditCommand::Move(Motion::Up)))
        );
    }

    #[test]
    fn repeated_note_navigation_skips_a_separator_after_the_first_step() {
        let blocks = ParsedNote::parse("- one\n- two\n\nnext\n").blocks;
        let commands = EditRequest::new(Entity::PLACEHOLDER, vec![EditCommand::Move(Motion::Down)])
            .accelerated_navigation(true)
            .remapped_for_note(&blocks, 0)
            .commands;
        assert_eq!(
            commands,
            [
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down),
            ]
        );
    }
}

fn document_key(
    trigger: On<UiInput<DocumentKeyInput>>,
    revisions: Query<&crate::host::editor::FileDocumentRevision>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok(revision) = revisions.get(event.webview) else {
        return;
    };
    if revision.get() != event.payload.revision {
        return;
    }
    commands.trigger(UiInput {
        webview: event.webview,
        payload: event.payload.stroke.clone(),
    });
}

#[cfg(test)]
mod input_lifecycle_tests {
    use super::*;
    use crate::host::editor::{FileDocumentRevision, FileView};
    use crate::host::file_lifecycle::{FileLifecyclePlugin, FileLoadTask};

    struct Fixture {
        app: App,
        entity: Entity,
        directory: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("input.rs");
            std::fs::write(&path, "original\n").unwrap();
            let mut settings = AppSettings::default();
            settings.editor.keymap = KeymapKind::Vim;
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(settings)
                .add_plugins((
                    FileLifecyclePlugin,
                    crate::host::navigation::NavigationPlugin,
                    EditorPlugin,
                    crate::encoding::EncodingPlugin,
                ))
                .init_resource::<BinIpcEventRawBuffer>();
            app.world_mut().insert_non_send(Browsers::default());
            app.world_mut().insert_non_send(ClipboardHandle(None));
            app.world_mut()
                .spawn(SharedFileViewMode(FileViewMode::Editor));
            let entity = app
                .world_mut()
                .spawn((
                    FileView { path },
                    FileViewport {
                        top_row: 0,
                        rows: 0,
                        wrap_columns: 0,
                        word_wrap: vmux_api::editor::WordWrap::Off,
                        word_wrap_column: 80,
                        scroll_revision: 0,
                    },
                    vmux_ecs::PageMetadata::default(),
                ))
                .id();
            FileLoadTask::settle(&mut app, entity);
            Self {
                app,
                entity,
                directory,
            }
        }

        fn revision(&self) -> u64 {
            self.app
                .world()
                .get::<FileDocumentRevision>(self.entity)
                .unwrap()
                .get()
        }

        fn key(&mut self, revision: u64, key: &str) {
            self.app.world_mut().commands().trigger(UiInput {
                webview: self.entity,
                payload: DocumentKeyInput {
                    revision,
                    stroke: KeyStroke {
                        key: key.into(),
                        ..Default::default()
                    },
                },
            });
        }

        fn text(&mut self, revision: u64, text: &str) {
            self.app.world_mut().commands().trigger(UiInput {
                webview: self.entity,
                payload: DocumentTextInput {
                    revision,
                    text: text.into(),
                },
            });
        }

        fn flush(&mut self) {
            self.app.world_mut().flush();
        }

        fn buffer(&self) -> String {
            self.app
                .world()
                .get::<crate::edit::buffer::TextBuffer>(self.entity)
                .unwrap()
                .text()
        }

        fn reopen(&mut self) {
            self.app.world_mut().trigger(UiInput {
                webview: self.entity,
                payload: FileEncodingReopenRequest {
                    encoding: FileEncoding::Utf8,
                },
            });
            FileLoadTask::settle(&mut self.app, self.entity);
        }
    }

    #[test]
    fn entering_insert_and_committing_ime_in_one_input_frame_is_undoable() {
        let mut fixture = Fixture::new();
        let revision = fixture.revision();
        fixture.key(revision, "i");
        fixture.text(revision, "あいう");
        fixture.flush();
        assert_eq!(fixture.buffer(), "あいうoriginal\n");
        fixture.key(revision, "Escape");
        fixture.flush();
        fixture.key(revision, "u");
        fixture.flush();
        assert_eq!(fixture.buffer(), "original\n");
    }

    #[test]
    fn changing_an_empty_line_accepts_committed_text_in_the_canonical_mode() {
        let mut fixture = Fixture::new();
        let revision = fixture.revision();
        fixture.key(revision, "G");
        fixture.flush();
        fixture.key(revision, "C");
        fixture.text(revision, "new");
        fixture.flush();
        assert_eq!(fixture.buffer(), "original\nnew");
    }

    #[test]
    fn editing_a_new_branch_after_undo_does_not_reuse_the_saved_state() {
        let mut fixture = Fixture::new();
        let revision = fixture.revision();
        fixture.key(revision, "i");
        fixture.text(revision, "saved:");
        fixture.flush();
        fixture.key(revision, "Escape");
        fixture.flush();
        EditRequest::new(fixture.entity, vec![EditCommand::Save])
            .emit_world(fixture.app.world_mut());
        assert_eq!(
            std::fs::read_to_string(fixture.directory.path().join("input.rs")).unwrap(),
            fixture.buffer()
        );
        fixture.key(revision, "u");
        fixture.flush();
        fixture.key(revision, "i");
        fixture.text(revision, "branch:");
        fixture.flush();
        let saved = std::fs::read_to_string(fixture.directory.path().join("input.rs")).unwrap();
        assert_ne!(fixture.buffer(), saved);
        assert!(
            fixture
                .app
                .world()
                .get::<crate::edit::document::Revision>(fixture.entity)
                .unwrap()
                .dirty
        );
    }

    #[test]
    fn same_path_replacement_refuses_the_previous_documents_dom_input() {
        let mut fixture = Fixture::new();
        let previous = fixture.revision();
        fixture.reopen();
        let current = fixture.revision();
        assert_ne!(current, previous);
        fixture.key(current, "i");
        fixture.flush();
        fixture.text(previous, "stale");
        fixture.key(previous, "ArrowRight");
        fixture.app.world_mut().commands().trigger(UiInput {
            webview: fixture.entity,
            payload: DocumentPointerInput {
                revision: previous,
                line: 0,
                col: 5,
                extend: false,
                add: false,
            },
        });
        fixture.app.world_mut().commands().trigger(UiInput {
            webview: fixture.entity,
            payload: DocumentFindInput {
                revision: previous,
                query: "original".into(),
                forward: true,
                ..Default::default()
            },
        });
        fixture.flush();
        assert_eq!(fixture.buffer(), "original\n");
        assert_eq!(
            Editor::read(fixture.app.world(), fixture.entity)
                .unwrap()
                .core
                .primary()
                .head,
            0
        );
        assert!(
            fixture
                .app
                .world()
                .get::<crate::edit::document::SearchState>(fixture.entity)
                .unwrap()
                .query
                .is_none()
        );
        fixture.text(current, "fresh");
        fixture.flush();
        assert_eq!(fixture.buffer(), "freshoriginal\n");
    }

    #[test]
    fn decoded_operations_cannot_mutate_a_replaced_document_incarnation() {
        let mut fixture = Fixture::new();
        let previous = fixture
            .app
            .world()
            .get::<crate::edit::document::Revision>(fixture.entity)
            .unwrap()
            .identity;
        EditRequest::new(
            fixture.entity,
            vec![EditCommand::InsertText("stale".into())],
        )
        .emit(previous, &mut fixture.app.world_mut().commands());
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.entity)
            .insert(crate::edit::Document::new(
                fixture.directory.path().join("input.rs"),
                "Rust".into(),
                "replacement\n",
                crate::edit::EditMode::Insert,
            ));
        fixture.flush();
        assert_eq!(fixture.buffer(), "replacement\n");
        EditRequest::new(
            fixture.entity,
            vec![EditCommand::InsertText("fresh".into())],
        )
        .emit_world(fixture.app.world_mut());
        assert_eq!(fixture.buffer(), "freshreplacement\n");
    }

    #[test]
    fn refused_preview_ime_does_not_enter_replay_or_undo_history() {
        let mut fixture = Fixture::new();
        let revision = fixture.revision();
        let path = fixture.directory.path().join("input.rs");
        fixture.app.world_mut().entity_mut(fixture.entity).insert(
            crate::host::navigation::CheckoutPreview(FileCheckoutState {
                path: path.to_string_lossy().into_owned(),
                repository: fixture.directory.path().to_string_lossy().into_owned(),
                checkout: fixture.directory.path().to_string_lossy().into_owned(),
                read_only: true,
            }),
        );
        fixture.key(revision, "i");
        fixture.text(revision, "refused");
        fixture.flush();
        assert_eq!(fixture.buffer(), "original\n");
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.entity)
            .remove::<crate::host::navigation::CheckoutPreview>();
        fixture.key(revision, "Escape");
        fixture.flush();
        fixture.key(revision, ".");
        fixture.flush();
        fixture.key(revision, "u");
        fixture.flush();
        assert_eq!(fixture.buffer(), "original\n");
        assert!(
            !fixture
                .app
                .world()
                .get::<crate::edit::document::Revision>(fixture.entity)
                .unwrap()
                .dirty
        );
    }
}
