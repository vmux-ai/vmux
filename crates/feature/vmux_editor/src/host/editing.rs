use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_api::command_bar::ExRequest;
use vmux_api::input::KeyStroke;
use vmux_command::{BindCommands, CommandInvocation, CommandRegistry};
use vmux_command::{CommandBarDismiss, CommandBarOpenRequest, WriteCommandBarRequests};
use vmux_command::{KeyCombo, KeyContext, Keymap};
use vmux_ecs::event::*;

#[cfg(test)]
use crate::edit::EditCore;
use crate::edit::{EditCommand, Motion, Selection};
use crate::host::editor::Editor;
use crate::host::explorer::{OpenEditorsDirty, OutlineDirty};
use crate::host::file_lifecycle::SelfWrites;
use crate::host::fold::FoldState;
use crate::host::keymap::{EditorKeymap, KeymapConfig};
use crate::host::language::{
    EditorCompletionRequest, EditorDefinitionRequest, EditorHoverRequest, EditorReferencesRequest,
    EditorRenameRequest, LspEditDirty, WikiCompletionRequest,
};
use crate::host::markdown::ParsedNote;
use crate::host::note::NoteSent;
use crate::host::status::SharedFileViewMode;
use crate::host::viewport::{CursorRenderRequest, FileViewport, FoldsDirty, ViewportRenderRequest};
use crate::keymap::{KeyInput, Mods};
use vmux_ecs::scroll::ScrollWindow;

pub(super) struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EditExecutionPlugin)
            .add_message::<CommandBarOpenRequest>()
            .add_plugins(UiEventPlugin::<(
                FileOpenEvent,
                FileTextInput,
                FilePointerEvent,
            )>::default())
            .add_plugins(UiEventPlugin::<(
                KnowledgeLinkOpen,
                FilePropertyEdit,
                FileFindRequest,
                ExRequest,
            )>::default())
            .add_message::<ExLineSubmitted>()
            .add_message::<OpenExRequest>()
            .add_systems(Startup, bind_ex_command.in_set(BindCommands))
            .add_systems(Update, open_ex.in_set(WriteCommandBarRequests))
            .add_observer(file_key)
            .add_observer(file_text_input)
            .add_observer(file_pointer)
            .add_systems(Update, (reapply_keymap_on_change, run_submitted_ex_lines))
            .add_observer(open_file_find)
            .add_observer(close_file_find)
            .add_observer(file_find_request)
            .add_observer(file_property_edit)
            .add_observer(submit_ex);
    }
}

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
struct ExLineSubmitted {
    stack: Option<Entity>,
    line: String,
}

fn bind_ex_command(registry: CommandRegistry, mut commands: Commands) {
    registry.message::<OpenExRequest>(&mut commands);
}

fn open_ex(
    mut requests: MessageReader<OpenExRequest>,
    mut open: MessageWriter<CommandBarOpenRequest>,
) {
    if requests.read().next().is_some() {
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

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileFindState {
    open: bool,
    forward: bool,
    revision: u64,
}

impl Default for FileFindState {
    fn default() -> Self {
        Self {
            open: false,
            forward: true,
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
    mut states: Query<&mut FileFindState>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(mut state) = states.get_mut(entity) else {
        return;
    };
    state.open = true;
    state.forward = trigger.event().forward;
    state.revision = state.revision.wrapping_add(1).max(1);
    commands.trigger(vmux_ecs::host::FileUiStateWrite::from_event(
        entity,
        &FileFindEvent {
            open: state.open,
            forward: state.forward,
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
    state.revision = state.revision.wrapping_add(1).max(1);
    commands.trigger(vmux_ecs::host::FileUiStateWrite::from_event(
        entity,
        &FileFindEvent {
            open: state.open,
            forward: state.forward,
            revision: state.revision,
        },
    ));
}

pub(super) struct EditExecutionPlugin;

impl Plugin for EditExecutionPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(ClipboardHandle(arboard::Clipboard::new().ok()))
            .add_observer(apply_edit_request);
    }
}

pub(super) struct ClipboardHandle(pub(super) Option<arboard::Clipboard>);

#[derive(Event)]
pub(super) struct EditRequest {
    entity: Entity,
    commands: Vec<EditCommand>,
}

impl EditRequest {
    pub(super) fn new(entity: Entity, commands: Vec<EditCommand>) -> Self {
        Self { entity, commands }
    }

    fn accelerated_navigation(mut self, repeat: bool) -> Self {
        if !repeat {
            return self;
        }
        let mut commands = Vec::with_capacity(self.commands.len() * 2);
        for command in self.commands {
            if let EditCommand::ScrollViewport(lines) = command {
                commands.push(EditCommand::ScrollViewport(lines.saturating_mul(2)));
                continue;
            }
            let accelerate = matches!(
                &command,
                EditCommand::Move(
                    Motion::Left
                        | Motion::Right
                        | Motion::LeftBounded
                        | Motion::RightBounded
                        | Motion::Up
                        | Motion::Down,
                ) | EditCommand::Select(
                    Motion::Left
                        | Motion::Right
                        | Motion::LeftBounded
                        | Motion::RightBounded
                        | Motion::Up
                        | Motion::Down,
                )
            );
            if accelerate {
                commands.push(command.clone());
            }
            commands.push(command);
        }
        self.commands = commands;
        self
    }

    fn remapped_for_note(mut self, blocks: &[NoteBlock], start_line: u32) -> Self {
        let mut line = start_line;
        let mut commands = Vec::new();
        for command in self.commands {
            let (direction, select) = match &command {
                EditCommand::Move(Motion::Down) => (1, false),
                EditCommand::Move(Motion::Up) => (-1, false),
                EditCommand::Select(Motion::Down) => (1, true),
                EditCommand::Select(Motion::Up) => (-1, true),
                _ => {
                    commands.push(command);
                    continue;
                }
            };
            let Some(target) = ParsedNote::vertical_target(blocks, line, direction) else {
                line = if direction > 0 {
                    line.saturating_add(1)
                } else {
                    line.saturating_sub(1)
                };
                commands.push(command);
                continue;
            };
            if target == line {
                continue;
            }
            let steps = target.abs_diff(line) as usize;
            line = target;
            let motion = if direction > 0 {
                Motion::Down
            } else {
                Motion::Up
            };
            let command = if select {
                EditCommand::Select(motion)
            } else {
                EditCommand::Move(motion)
            };
            commands.extend(std::iter::repeat_n(command, steps));
        }
        self.commands = commands;
        self
    }

    fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

fn reapply_keymap_on_change(
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut last: Local<Option<KeymapConfig>>,
    mut q: Query<(
        Entity,
        &mut Editor,
        &mut EditorKeymap,
        Option<&FileViewport>,
    )>,
    mut commands: Commands,
) {
    let next = KeymapConfig::resolve(settings.as_deref());
    if last.as_ref() == Some(&next) {
        return;
    }
    let first = last.is_none();
    let kind_changed = last
        .as_ref()
        .is_none_or(|previous| previous.kind() != next.kind());
    *last = Some(next);
    if first {
        return;
    }
    let Some(config) = last.as_ref() else {
        return;
    };
    for (entity, mut edit, mut keymap, viewport) in &mut q {
        *keymap = config.keymap();
        if kind_changed {
            edit.core.mode = config.initial_mode();
        }
        if viewport.is_some() {
            commands.trigger(CursorRenderRequest::new(entity));
        }
    }
}

fn apply_edit_request(
    trigger: On<EditRequest>,
    mut views: Query<(&mut Editor, &mut FileViewport, &mut vmux_git::GitDiffSource)>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let request = trigger.event();
    let entity = request.entity;
    let cmds = request.commands.clone();
    let Ok((mut edit, mut vp, mut diff_source)) = views.get_mut(entity) else {
        return;
    };
    let top_before = vp.top_row;
    let mut text_changed = false;
    let mut cursor_stale = false;
    let mut dirty_changed = false;
    let mut fold_changed = false;
    for cmd in cmds {
        if let EditCommand::ScrollViewport(lines) = &cmd {
            let visible = vp.visible_rows(&mut edit);
            let target = (vp.top_row as i64 + *lines as i64).clamp(0, u32::MAX as i64) as u32;
            let target = ScrollWindow::new(visible, target, vp.rows).top();
            if let Some(scroll) = vp.set_top(target)
                && browsers.can_emit_to(&entity)
            {
                commands.trigger(vmux_ecs::host::FileUiStateWrite::from_event(
                    entity, &scroll,
                ));
            }
            edit.core.top_row = vp.top_row;
            if vp.follow_scrolled_cursor(&mut edit) {
                cursor_stale = true;
            }
            continue;
        }
        if let EditCommand::ScrollCursorTo(placement) = &cmd {
            let row = edit
                .folds
                .view(edit.core.buffer.len_lines() as u32)
                .buffer_to_row(edit.core.cursor_pos().line);
            let rows = vp.rows.max(1) as u32;
            let target = match placement {
                crate::edit::command::ScrollPlacement::Top => row,
                crate::edit::command::ScrollPlacement::Center => row.saturating_sub(rows / 2),
                crate::edit::command::ScrollPlacement::Bottom => row.saturating_sub(rows - 1),
            };
            if let Some(scroll) = vp.set_top(target)
                && browsers.can_emit_to(&entity)
            {
                commands.trigger(vmux_ecs::host::FileUiStateWrite::from_event(
                    entity, &scroll,
                ));
            }
            edit.core.top_row = vp.top_row;
            continue;
        }
        if matches!(
            cmd,
            EditCommand::FoldToggle
                | EditCommand::FoldOpen
                | EditCommand::FoldClose
                | EditCommand::FoldToggleRecursive
                | EditCommand::FoldAll
                | EditCommand::UnfoldAll
        ) {
            let line = edit.core.cursor_pos().line;
            match cmd {
                EditCommand::FoldToggle => edit.folds.toggle(line),
                EditCommand::FoldOpen => edit.folds.open(line),
                EditCommand::FoldClose => edit.folds.close(line),
                EditCommand::FoldToggleRecursive => edit.folds.toggle_recursive(line),
                EditCommand::FoldAll => edit.folds.fold_all(),
                EditCommand::UnfoldAll => edit.folds.unfold_all(),
                _ => {}
            }
            edit.sync_fold_view();
            if let Some(header) = edit.folds.hiding_header(line) {
                let at = edit.core.buffer.line_to_char(header as usize);
                edit.core.set_caret(at);
            }
            fold_changed = true;
            continue;
        }
        match &cmd {
            EditCommand::Hover => {
                commands.trigger(EditorHoverRequest::from(entity));
                continue;
            }
            EditCommand::GotoDefinition => {
                commands.trigger(EditorDefinitionRequest::from(entity));
                continue;
            }
            EditCommand::FindReferences => {
                commands.trigger(EditorReferencesRequest::from(entity));
                continue;
            }
            EditCommand::BeginRename => {
                commands.trigger(EditorRenameRequest::from(entity));
                continue;
            }
            EditCommand::ClearSearchHighlight => {
                commands.trigger(FileFindCloseRequest::from(entity));
                cursor_stale = true;
            }
            EditCommand::OpenFind { forward } => {
                commands.trigger(FileFindOpenRequest::new(entity, *forward));
                continue;
            }
            EditCommand::OpenCommandLine => {
                commands.write_message(CommandInvocation::new(entity, "browser_open_ex_bar"));
                continue;
            }
            EditCommand::TriggerCompletion => {
                commands.trigger(EditorCompletionRequest::from(entity));
                continue;
            }
            EditCommand::ScrollViewport(_) => unreachable!(),
            _ => {}
        }
        if matches!(cmd, EditCommand::Save) {
            let path = edit.core.buffer.path.clone();
            let body = edit.core.buffer.text();
            let encoding = edit.core.buffer.encoding;
            let bytes = match (crate::encoding::Reencode { encoding }).applied(&body) {
                Ok(bytes) => bytes,
                Err(unmappable) => {
                    tracing::warn!(path = %path.display(), "editor save refused: {unmappable}");
                    if browsers.can_emit_to(&entity) {
                        commands.trigger(vmux_ecs::host::FileUiStateWrite::from_event(
                            entity,
                            &FileErrorEvent {
                                message: format!("save failed: {unmappable}"),
                                undecodable: false,
                            },
                        ));
                    }
                    continue;
                }
            };
            match vmux_path::AtomicFile::write(&path, &bytes) {
                Ok(()) => {
                    self_writes.0.insert(
                        vmux_path::PathIdentity::resolve(&path).into_path_buf(),
                        std::time::Instant::now(),
                    );
                    let was_dirty = edit.core.dirty;
                    edit.core.mark_saved();
                    if was_dirty {
                        dirty_changed = true;
                    }
                    commands
                        .entity(entity)
                        .insert(LspEditDirty)
                        .remove::<crate::lsp::manager::LintRan>();
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), "editor save failed: {e}");
                    if browsers.can_emit_to(&entity) {
                        commands.trigger(vmux_ecs::host::FileUiStateWrite::from_event(
                            entity,
                            &FileErrorEvent {
                                message: format!("save failed: {e}"),
                                undecodable: false,
                            },
                        ));
                    }
                }
            }
            continue;
        }
        if matches!(cmd, EditCommand::Paste) {
            let Some(cb) = clipboard.0.as_mut() else {
                continue;
            };
            let Ok(s) = cb.get_text() else {
                continue;
            };
            if edit.core.paste(&s) {
                text_changed = true;
                let (l, _) = edit.core.buffer.char_to_coords(edit.core.primary().head);
                edit.hl.invalidate_from(l.saturating_sub(1));
            }
            cursor_stale = true;
            dirty_changed = true;
            continue;
        }
        if matches!(cmd, EditCommand::Put { .. })
            && let Some(cb) = clipboard.0.as_mut()
            && let Ok(s) = cb.get_text()
            && s != edit.core.registers.clipboard_shadow
        {
            edit.core.registers.clipboard_shadow = s.clone();
            edit.core
                .registers
                .set_unnamed(crate::edit::RegisterValue::charwise(s));
        }
        let out = edit.core.apply(cmd);
        if out.text_changed {
            text_changed = true;
            let (l, _) = edit.core.buffer.char_to_coords(edit.core.primary().head);
            edit.hl.invalidate_from(l.saturating_sub(1));
        }
        cursor_stale |= out.sel_changed || out.mode_changed;
        dirty_changed |= out.dirty_changed;
        if let Some(value) = out.yank
            && let Some(cb) = clipboard.0.as_mut()
        {
            edit.core.registers.clipboard_shadow = value.text.clone();
            let _ = cb.set_text(value.text);
        }
    }
    if text_changed {
        let regions = FoldState::indent_regions(&edit.core.buffer.rope);
        edit.folds.set_regions(regions);
        edit.sync_fold_view();
    }
    {
        let total = edit.core.buffer.len_lines() as u32;
        let caret_line = edit.core.cursor_pos().line;
        if edit.folds.view(total).is_hidden(caret_line) {
            edit.folds.reveal(caret_line);
            edit.sync_fold_view();
            fold_changed = true;
        }
    }
    if let Some(top) = vp.autoscroll(&mut edit) {
        if let Some(scroll) = vp.set_top(top)
            && browsers.can_emit_to(&entity)
        {
            commands.trigger(vmux_ecs::host::FileUiStateWrite::from_event(
                entity, &scroll,
            ));
        }
        edit.core.top_row = vp.top_row;
    }
    let vpc = *vp;
    if text_changed || fold_changed || vpc.left_render_band(top_before) {
        commands.trigger(ViewportRenderRequest::new(entity));
    }
    if text_changed || cursor_stale || fold_changed {
        commands.trigger(CursorRenderRequest::new(entity));
    }
    if fold_changed {
        commands.entity(entity).insert(FoldsDirty);
    }
    if dirty_changed {
        commands.entity(entity).insert(OpenEditorsDirty);
    }
    if text_changed || dirty_changed {
        diff_source.content = edit.core.buffer.text();
        diff_source.dirty = edit.core.dirty;
        commands.trigger(vmux_ecs::host::FileUiStateWrite::from_event(
            entity,
            &FileDirtyEvent {
                dirty: edit.core.dirty,
            },
        ));
    }
    if text_changed {
        edit.refresh_parsed_note();
        let markdown = edit.is_note();
        let mut entity_commands = commands.entity(entity);
        entity_commands
            .insert(LspEditDirty)
            .remove::<crate::lsp::manager::LintRan>();
        if markdown {
            entity_commands.remove::<NoteSent>().insert(OutlineDirty);
        }
    }
}

fn file_key(
    trigger: On<UiInput<KeyStroke>>,
    mut q: Query<(&Editor, &mut EditorKeymap)>,
    app_keymaps: Query<&Keymap>,
    app_contexts: Query<&KeyContext>,
    view_mode: Single<&SharedFileViewMode>,
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
    let Ok((edit, mut keymap)) = q.get_mut(entity) else {
        return;
    };
    let input = KeyInput {
        key: evt.key.clone(),
        mods: Mods {
            ctrl: evt.mods.ctrl,
            alt: evt.mods.alt,
            shift: evt.mods.shift,
            meta: evt.mods.super_key,
        },
        repeat: evt.repeat,
    };
    let mut request =
        EditRequest::new(entity, keymap.0.handle(&input)).accelerated_navigation(evt.repeat);
    if view_mode.0 == FileViewMode::Note
        && let Some(blocks) = edit.note_blocks()
    {
        let line = edit.core.cursor_pos().line;
        request = request.remapped_for_note(blocks, line);
    }
    if request.is_empty() {
        return;
    }
    commands.trigger(request);
}

fn file_text_input(
    trigger: On<UiInput<FileTextInput>>,
    mut q: Query<(&Editor, &mut EditorKeymap)>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let text = trigger.event().payload.text.clone();
    if text.is_empty() {
        return;
    }
    let Ok((_, mut keymap)) = q.get_mut(entity) else {
        return;
    };
    if !keymap.0.mode().accepts_text() {
        return;
    }
    keymap.0.record_text(&text);
    let command = if keymap.0.mode() == vmux_api::editor::EditMode::Replace {
        EditCommand::OvertypeText(text)
    } else {
        EditCommand::InsertText(text)
    };
    commands.trigger(EditRequest::new(entity, vec![command]));
    commands.trigger(WikiCompletionRequest::from(entity));
}

fn file_property_edit(
    trigger: On<UiInput<FilePropertyEdit>>,
    q: Query<&Editor>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(edit) = q.get(entity) else {
        return;
    };
    if !ParsedNote::supports(&edit.core.buffer.path) {
        return;
    }
    let text = edit.core.buffer.text();
    let updated =
        match vmux_knowledge::Frontmatter::from(text.as_str()).apply(&trigger.event().payload) {
            Ok(updated) => updated,
            Err(message) => {
                commands.trigger(vmux_ecs::host::FileUiStateWrite::from_event(
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
    commands.trigger(EditRequest::new(
        entity,
        vec![EditCommand::ReplaceText(updated)],
    ));
}

fn run_submitted_ex_lines(
    mut submitted: MessageReader<ExLineSubmitted>,
    children: Query<&Children>,
    q: Query<(), (With<Editor>, With<EditorKeymap>)>,
    mut commands: Commands,
) {
    for message in submitted.read() {
        let cmds = crate::edit::ex::ExLine::edits(&message.line);
        if cmds.is_empty() {
            continue;
        }
        let Some(stack) = message.stack else {
            continue;
        };
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(entity) = kids.iter().find(|child| q.contains(*child)) else {
            continue;
        };
        commands.trigger(EditRequest::new(entity, cmds));
    }
}

fn file_find_request(
    trigger: On<UiInput<FileFindRequest>>,
    mut q: Query<&mut Editor>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload.clone();
    let Ok(mut edit) = q.get_mut(entity) else {
        return;
    };
    if request.done || request.query.is_empty() {
        edit.core.apply(EditCommand::ClearSearchHighlight);
        if request.done {
            commands.trigger(FileFindCloseRequest::from(entity));
        }
    } else if request.step {
        edit.core.apply(EditCommand::Move(Motion::SearchNext {
            reverse: request.reverse,
        }));
    } else {
        let pattern = match request.regex {
            true => crate::edit::search::Search::translate(&request.query),
            false => regex::escape(&request.query),
        };
        edit.core.apply(EditCommand::SetSearch {
            pattern,
            forward: request.forward,
        });
    }
    commands.trigger(CursorRenderRequest::new(entity));
}

fn file_pointer(
    trigger: On<UiInput<FilePointerEvent>>,
    mut q: Query<(&mut Editor, &mut EditorKeymap)>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let p = trigger.event().payload;
    let Ok((mut edit, mut keymap)) = q.get_mut(entity) else {
        return;
    };
    let col = edit.core.char_at_cell(p.line as usize, p.col);
    let at = edit.core.buffer.coords_to_char(p.line as usize, col);
    if p.add {
        edit.core.toggle_caret(at);
    } else if p.extend {
        let anchor = edit.core.primary().anchor;
        edit.core.selections = vec![Selection { anchor, head: at }];
    } else {
        edit.core.collapse_carets();
        edit.core.set_caret(at);
    }
    if let Some(command) = keymap.0.pointer_selection_mode(p.extend) {
        edit.core.apply(command);
    }
    commands.trigger(CursorRenderRequest::new(entity));
}

#[cfg(test)]
mod edit_flow_tests {
    use super::*;
    use crate::keymap::{KeyInput, KeymapKindExt, Mods};

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
            *app.world().get::<FileFindState>(first).unwrap(),
            FileFindState {
                open: true,
                forward: false,
                revision: 1,
            }
        );
        assert_eq!(
            *app.world().get::<FileFindState>(second).unwrap(),
            FileFindState::default()
        );

        app.world_mut().trigger(FileFindCloseRequest::from(first));

        assert_eq!(
            *app.world().get::<FileFindState>(first).unwrap(),
            FileFindState {
                open: false,
                forward: false,
                revision: 2,
            }
        );
    }

    #[test]
    fn vim_dd_deletes_line_via_keymap_and_core() {
        let mut km = vmux_api::editor::KeymapKind::Vim.make(&[], " ");
        let mut core = EditCore::new(
            std::path::PathBuf::from("a.txt"),
            "Plain Text".into(),
            "one\ntwo\nthree\n",
            crate::edit::EditMode::Normal,
        );
        for key in ["d", "d"] {
            for cmd in km.handle(&KeyInput {
                key: key.into(),
                mods: Mods::default(),
                repeat: false,
            }) {
                core.apply(cmd);
            }
        }
        assert_eq!(core.buffer.text(), "two\nthree\n");
    }

    #[test]
    fn vscode_typing_inserts_and_marks_dirty() {
        let mut core = EditCore::new(
            std::path::PathBuf::from("a.txt"),
            "Plain Text".into(),
            "",
            crate::edit::EditMode::Insert,
        );
        core.apply(EditCommand::InsertText("hello".into()));
        assert_eq!(core.buffer.text(), "hello");
        assert!(core.dirty);
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
