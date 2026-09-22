use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_command::ScopedKeys;
use vmux_core::PageMetadata;
use vmux_core::event::*;
use vmux_core::input::KeyStroke;
#[cfg(test)]
use vmux_core::page_open::PageOpenTask;

use crate::edit::highlight_cache::HighlightCache;
use crate::edit::{EditCommand, EditCore, Motion, Selection};
#[cfg(test)]
use crate::host::explorer::{
    ExplorerPanelDefaults, ExplorerTabsPlugin, StackExplorerRevision, StackExplorerVisibility,
};
#[cfg(test)]
use crate::host::explorer::{ExplorerPanelPlugin, ExplorerPanelSent, ExplorerState};
#[cfg(test)]
use crate::host::explorer::{
    ExplorerTree, ExplorerTreeDirty, ExplorerTreePlugin, ExplorerTrees, IDLE_TREE_CAPACITY,
};
use crate::host::explorer::{OpenEditorsDirty, OutlineDirty};
#[cfg(test)]
use crate::host::file_lifecycle::EditorFileLifecyclePlugin;
#[cfg(test)]
use crate::host::file_lifecycle::LoadFailure;
use crate::host::file_lifecycle::{FileBuffer, FileDir, FileLoadTask, SelfWrites, canon};
#[cfg(test)]
use crate::host::history::EditorHistoryPlugin;
use crate::host::language::{EditorLanguageRequest, LspEditDirty, WikiCompletionRequest};
#[cfg(test)]
use crate::host::navigation::EditorNavigationPlugin;
use crate::host::note::NoteSent;
#[cfg(test)]
use crate::host::page_open::EditorPageOpenPlugin;
use crate::host::status::{FileInitialMetaSent, SharedFileViewMode};
use crate::host::viewport::{EditorCursor, EditorWindow, FileViewport, FoldsDirty};
use crate::keymap::{KeyInput, Keymap, KeymapKindExt, Mods};
use crate::media::FileMedia;
use crate::wrap::WrapView;
use vmux_core::scroll::clamp_top_line;

pub(super) struct EditorEditingPlugin;

impl Plugin for EditorEditingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EditExecutionPlugin)
            .add_plugins(UiEventPlugin::<(
                FileOpenEvent,
                FileTextInput,
                FilePointerEvent,
            )>::default())
            .add_plugins(UiEventPlugin::<(
                KnowledgeLinkOpen,
                FilePropertyEdit,
                FileFindRequest,
            )>::default())
            .add_observer(on_file_key)
            .add_observer(on_file_text_input)
            .add_observer(on_file_pointer)
            .add_systems(Update, (reapply_keymap_on_change, run_submitted_ex_lines))
            .add_observer(on_file_find_request)
            .add_observer(on_file_property_edit);
    }
}

pub(super) struct EditExecutionPlugin;

impl Plugin for EditExecutionPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(ClipboardHandle(arboard::Clipboard::new().ok()))
            .add_observer(apply_edit_request);
    }
}

#[derive(Component, Clone, Debug)]
pub struct FileView {
    pub path: PathBuf,
}

impl FileView {
    pub(super) fn in_stack(
        stack: Entity,
        children_q: &Query<&Children>,
        views: &Query<(&mut FileView, &mut FileViewport, &mut PageMetadata)>,
    ) -> Option<Entity> {
        let Ok(children) = children_q.get(stack) else {
            return None;
        };
        children.iter().find(|&child| views.contains(child))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn navigate(
        &mut self,
        entity: Entity,
        path: PathBuf,
        top_line: u32,
        viewport: &mut FileViewport,
        metadata: &mut PageMetadata,
        manager: &mut crate::lsp::manager::LspManager,
        commands: &mut Commands,
    ) {
        let previous = std::mem::replace(&mut self.path, path);
        manager.close(&previous);
        metadata.title = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.to_string_lossy().to_string());
        metadata.url = self.url();
        viewport.top_row = top_line;
        commands.queue(move |world: &mut World| {
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return;
            };
            ParkedEdits::park(&mut entity, previous);
        });
        commands
            .entity(entity)
            .remove::<FileDir>()
            .remove::<FileBuffer>()
            .remove::<FileMedia>()
            .remove::<FileLoadTask>()
            .remove::<EditorKeymap>()
            .remove::<NoteSent>()
            .remove::<LspEditDirty>()
            .remove::<FileInitialMetaSent>()
            .remove::<crate::lsp::manager::LspOpened>()
            .remove::<crate::lsp::manager::LintRan>();
    }

    pub(super) fn url(&self) -> String {
        url::Url::from_file_path(&self.path)
            .map(|url| url.to_string())
            .unwrap_or_else(|_| format!("file://{}", self.path.to_string_lossy()))
    }

    pub(crate) fn display_path(&self) -> String {
        if let Ok(cwd) = std::env::current_dir()
            && let Ok(relative) = self.path.strip_prefix(&cwd)
        {
            return relative.to_string_lossy().to_string();
        }
        if let Some(home) = std::env::home_dir()
            && let Ok(relative) = self.path.strip_prefix(&home)
        {
            return format!("~/{}", relative.to_string_lossy());
        }
        self.path.to_string_lossy().to_string()
    }

    pub(crate) fn raw_media_url(&self) -> String {
        let mut url = self.url();
        url.push_str("?vmux-raw=1");
        url
    }
}

#[derive(Component)]
pub struct EditState {
    pub core: EditCore,
    pub hl: HighlightCache,
    pub folds: crate::fold::FoldState,
    indent_width: u16,
    parsed_note: Option<crate::markdown::ParsedNote>,
    wrap_generation: u64,
    wrap_cache: Option<CachedWrapView>,
}

impl EditState {
    pub(crate) fn new(core: EditCore, hl: HighlightCache, folds: crate::fold::FoldState) -> Self {
        let parsed_note = crate::markdown::is_markdown_path(&core.buffer.path)
            .then(|| crate::markdown::parse_note_document(&core.buffer.text()));
        let indent_width = crate::shape::BufferShape::detect(&core.buffer.rope)
            .indent
            .width;
        Self {
            core,
            hl,
            folds,
            indent_width,
            parsed_note,
            wrap_generation: 0,
            wrap_cache: None,
        }
    }

    pub(crate) fn parsed_note(&self) -> Option<crate::markdown::ParsedNote> {
        self.parsed_note.clone()
    }

    pub(crate) fn cursor_line(&self) -> u32 {
        self.core.cursor_pos().line
    }

    pub(crate) fn indent_width(&self) -> u16 {
        self.indent_width
    }

    pub(crate) fn wrapped_view<'a>(&'a mut self, viewport: &FileViewport) -> &'a WrapView {
        let stale = self.wrap_cache.as_ref().is_none_or(|cache| {
            cache.generation != self.wrap_generation
                || cache.mode != viewport.word_wrap
                || cache.viewport_columns != viewport.wrap_columns
                || cache.word_wrap_column != viewport.word_wrap_column
        });
        if stale {
            let total = self.core.buffer.len_lines() as u32;
            let folds = self.folds.view(total);
            self.wrap_cache = Some(CachedWrapView {
                generation: self.wrap_generation,
                mode: viewport.word_wrap,
                viewport_columns: viewport.wrap_columns,
                word_wrap_column: viewport.word_wrap_column,
                view: WrapView::new(
                    &self.core.buffer.rope,
                    &folds,
                    viewport.word_wrap,
                    viewport.wrap_columns,
                    viewport.word_wrap_column,
                ),
            });
        }
        &self.wrap_cache.as_ref().expect("wrap cache").view
    }

    pub(crate) fn sync_fold_view(&mut self) {
        let total = self.core.buffer.len_lines() as u32;
        self.core.fold_view = self.folds.view(total);
        self.wrap_generation = self.wrap_generation.wrapping_add(1);
    }

    pub(super) fn set_shape(&mut self, shape: crate::shape::BufferShape) {
        self.indent_width = shape.indent.width;
    }

    fn refresh_parsed_note(&mut self) {
        self.parsed_note = crate::markdown::is_markdown_path(&self.core.buffer.path)
            .then(|| crate::markdown::parse_note_document(&self.core.buffer.text()));
    }
}

#[derive(Component, Default)]
pub(super) struct ParkedEdits {
    by_path: HashMap<PathBuf, ParkedEdit>,
    recent: Vec<PathBuf>,
}

pub(super) struct ParkedEdit {
    pub(super) edit: EditState,
    pub(super) diff: vmux_git::GitDiffSource,
    modified: Option<std::time::SystemTime>,
}

impl ParkedEdits {
    const CAPACITY: usize = 8;

    fn park(entity: &mut EntityWorldMut, path: PathBuf) {
        if !entity.contains::<EditState>() || !entity.contains::<vmux_git::GitDiffSource>() {
            return;
        }
        let Some(edit) = entity.take::<EditState>() else {
            return;
        };
        let Some(diff) = entity.take::<vmux_git::GitDiffSource>() else {
            return;
        };
        let parked = ParkedEdit {
            edit,
            diff,
            modified: Self::modified_at(&path),
        };
        let mut edits = entity.take::<ParkedEdits>().unwrap_or_default();
        edits.insert(path, parked);
        entity.insert(edits);
    }

    fn insert(&mut self, path: PathBuf, edit: ParkedEdit) {
        self.recent.retain(|p| p != &path);
        self.recent.push(path.clone());
        self.by_path.insert(path, edit);
        while self.recent.len() > Self::CAPACITY {
            let evicted = self.recent.remove(0);
            self.by_path.remove(&evicted);
        }
    }

    pub(super) fn resume(&mut self, path: &Path) -> Option<ParkedEdit> {
        let parked = self.by_path.remove(path)?;
        self.recent.retain(|p| p != path);
        if parked.edit.core.dirty || parked.modified == Self::modified_at(path) {
            return Some(parked);
        }
        None
    }

    pub(super) fn is_dirty(&self, path: &Path) -> bool {
        self.by_path
            .get(path)
            .is_some_and(|parked| parked.edit.core.dirty)
    }

    fn modified_at(path: &Path) -> Option<std::time::SystemTime> {
        std::fs::metadata(path).ok()?.modified().ok()
    }
}

struct CachedWrapView {
    generation: u64,
    mode: vmux_core::editor::WordWrap,
    viewport_columns: u16,
    word_wrap_column: u16,
    view: WrapView,
}

#[derive(Component)]
pub struct EditorKeymap(pub Box<dyn Keymap>);

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
            let Some(target) = crate::markdown::note_vertical_target(blocks, line, direction)
            else {
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

#[derive(PartialEq, Eq)]
pub(super) struct KeymapConfig {
    kind: vmux_core::KeymapKind,
    maps: Vec<vmux_core::editor::KeyMapping>,
    leader: String,
}

impl KeymapConfig {
    pub(super) fn resolve(settings: Option<&vmux_setting::AppSettings>) -> Self {
        let Some(settings) = settings else {
            return Self {
                kind: vmux_core::KeymapKind::default(),
                maps: Vec::new(),
                leader: " ".to_string(),
            };
        };
        Self {
            kind: settings.editor.keymap,
            maps: settings.editor.mappings.clone(),
            leader: settings.editor.leader.clone(),
        }
    }

    pub(super) fn keymap(&self) -> EditorKeymap {
        EditorKeymap(self.kind.make(&self.maps, &self.leader))
    }

    pub(super) fn initial_mode(&self) -> vmux_core::EditMode {
        self.kind.initial_mode()
    }

    pub(super) fn kind(&self) -> vmux_core::KeymapKind {
        self.kind
    }
}

fn reapply_keymap_on_change(
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut last: Local<Option<KeymapConfig>>,
    mut q: Query<(
        Entity,
        &mut EditState,
        &mut EditorKeymap,
        Option<&FileViewport>,
    )>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let next = KeymapConfig::resolve(settings.as_deref());
    if last.as_ref() == Some(&next) {
        return;
    }
    let first = last.is_none();
    let kind_changed = last.as_ref().is_none_or(|prev| prev.kind != next.kind);
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
        if let (Some(viewport), Some(browsers)) = (viewport, browsers.as_deref()) {
            EditorCursor::emit(
                entity,
                &mut edit,
                keymap.0.as_ref(),
                viewport,
                browsers,
                &mut commands,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_edit_request(
    trigger: On<EditRequest>,
    mut views: Query<(
        &mut EditState,
        &EditorKeymap,
        &mut FileViewport,
        &mut vmux_git::GitDiffSource,
    )>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let request = trigger.event();
    let entity = request.entity;
    let cmds = request.commands.clone();
    let Ok((mut edit, keymap, mut vp, mut diff_source)) = views.get_mut(entity) else {
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
            let target = clamp_top_line(target, visible, vp.rows);
            vp.scroll_to(target, entity, &browsers, &mut commands);
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
            vp.scroll_to(target, entity, &browsers, &mut commands);
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
                commands.trigger(EditorLanguageRequest::Hover(entity));
                continue;
            }
            EditCommand::GotoDefinition => {
                commands.trigger(EditorLanguageRequest::Definition(entity));
                continue;
            }
            EditCommand::FindReferences => {
                commands.trigger(EditorLanguageRequest::References(entity));
                continue;
            }
            EditCommand::BeginRename => {
                commands.trigger(EditorLanguageRequest::BeginRename(entity));
                continue;
            }
            EditCommand::ClearSearchHighlight => {
                commands.trigger(BinHostEmitEvent::from_event(
                    entity,
                    &vmux_core::event::FileKey::FindClose,
                ));
                cursor_stale = true;
            }
            EditCommand::OpenFind { forward } => {
                commands.trigger(BinHostEmitEvent::from_event(
                    entity,
                    &vmux_core::event::FileKey::Find { forward: *forward },
                ));
                continue;
            }
            EditCommand::OpenCommandLine => {
                commands.write_message(vmux_command::host::command::AppCommand::Browser(
                    vmux_command::host::command::BrowserCommand::Bar(
                        vmux_command::host::command::BrowserBarCommand::OpenExBar,
                    ),
                ));
                continue;
            }
            EditCommand::TriggerCompletion => {
                commands.trigger(EditorLanguageRequest::Completion(entity));
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
                        commands.trigger(BinHostEmitEvent::from_event(
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
                    self_writes
                        .0
                        .insert(canon(&path), std::time::Instant::now());
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
                        commands.trigger(BinHostEmitEvent::from_event(
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
        let regions = crate::fold::indent_regions(&edit.core.buffer.rope);
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
        vp.scroll_to(top, entity, &browsers, &mut commands);
        edit.core.top_row = vp.top_row;
    }
    let vpc = *vp;
    if text_changed || fold_changed || vpc.left_render_band(top_before) {
        EditorWindow::emit(entity, &mut edit, &vpc, &browsers, &mut commands);
    }
    if text_changed || cursor_stale || fold_changed {
        EditorCursor::emit(
            entity,
            &mut edit,
            keymap.0.as_ref(),
            &vpc,
            &browsers,
            &mut commands,
        );
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
        commands.trigger(BinHostEmitEvent::from_event(
            entity,
            &FileDirtyEvent {
                dirty: edit.core.dirty,
            },
        ));
    }
    if text_changed {
        edit.refresh_parsed_note();
        let markdown = edit.parsed_note.is_some();
        let mut entity_commands = commands.entity(entity);
        entity_commands
            .insert(LspEditDirty)
            .remove::<crate::lsp::manager::LintRan>();
        if markdown {
            entity_commands.remove::<NoteSent>().insert(OutlineDirty);
        }
    }
}

fn on_file_key(
    trigger: On<BinReceive<KeyStroke>>,
    mut q: Query<(&EditState, &mut EditorKeymap)>,
    app_keys: ScopedKeys,
    view_mode: Res<SharedFileViewMode>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    if app_keys.answered(entity, evt) {
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
        && let Some(note) = edit.parsed_note.as_ref()
    {
        let line = edit.core.cursor_pos().line;
        request = request.remapped_for_note(&note.blocks, line);
    }
    if request.is_empty() {
        return;
    }
    commands.trigger(request);
}

fn on_file_text_input(
    trigger: On<BinReceive<FileTextInput>>,
    mut q: Query<(&EditState, &mut EditorKeymap)>,
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
    let command = if keymap.0.mode() == vmux_core::EditMode::Replace {
        EditCommand::OvertypeText(text)
    } else {
        EditCommand::InsertText(text)
    };
    commands.trigger(EditRequest::new(entity, vec![command]));
    commands.trigger(WikiCompletionRequest::new(entity));
}

fn on_file_property_edit(
    trigger: On<BinReceive<FilePropertyEdit>>,
    q: Query<&EditState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(edit) = q.get(entity) else {
        return;
    };
    if !crate::markdown::is_markdown_path(&edit.core.buffer.path) {
        return;
    }
    let text = edit.core.buffer.text();
    let updated = match vmux_core::knowledge::Frontmatter::from(text.as_str())
        .apply(&trigger.event().payload)
    {
        Ok(updated) => updated,
        Err(message) => {
            commands.trigger(BinHostEmitEvent::from_event(
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
    mut submitted: MessageReader<vmux_command::host::ExLineSubmitted>,
    children: Query<&Children>,
    q: Query<(), (With<EditState>, With<EditorKeymap>)>,
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

fn on_file_find_request(
    trigger: On<BinReceive<FileFindRequest>>,
    mut q: Query<(&mut EditState, &EditorKeymap, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload.clone();
    let Ok((mut edit, keymap, vp)) = q.get_mut(entity) else {
        return;
    };
    if request.done || request.query.is_empty() {
        edit.core.apply(EditCommand::ClearSearchHighlight);
    } else if request.step {
        edit.core.apply(EditCommand::Move(Motion::SearchNext {
            reverse: request.reverse,
        }));
    } else {
        let pattern = match request.regex {
            true => crate::edit::search::translate(&request.query),
            false => regex::escape(&request.query),
        };
        edit.core.apply(EditCommand::SetSearch {
            pattern,
            forward: request.forward,
        });
    }
    EditorCursor::emit(
        entity,
        &mut edit,
        keymap.0.as_ref(),
        vp,
        &browsers,
        &mut commands,
    );
}

fn on_file_pointer(
    trigger: On<BinReceive<FilePointerEvent>>,
    mut q: Query<(&mut EditState, &mut EditorKeymap, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let p = trigger.event().payload;
    let Ok((mut edit, mut keymap, vp)) = q.get_mut(entity) else {
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
    EditorCursor::emit(
        entity,
        &mut edit,
        keymap.0.as_ref(),
        vp,
        &browsers,
        &mut commands,
    );
}

#[cfg(test)]
mod edit_flow_tests {
    use super::*;
    use crate::keymap::{KeyInput, KeymapKindExt, Mods};

    #[test]
    fn vim_dd_deletes_line_via_keymap_and_core() {
        let mut km = vmux_core::KeymapKind::Vim.make(&[], " ");
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
        let blocks = crate::markdown::parse_note("- one\n- two\n\nnext\n");
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

#[cfg(test)]
mod explorer_tests {
    use super::*;
    use std::fs;
    use vmux_api::BinEvent;

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
            app.add_plugins((MinimalPlugins, ExplorerTreePlugin))
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
            let decoded =
                rkyv::from_bytes::<ExplorerFocusEvent, rkyv::rancor::Error>(emit.payload());
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
        app.add_plugins((MinimalPlugins, ExplorerPanelPlugin))
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
        app.add_plugins((MinimalPlugins, ExplorerPanelPlugin))
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
        app.add_plugins(ExplorerPanelPlugin);
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
        app.add_plugins((MinimalPlugins, ExplorerPanelPlugin))
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
        app.add_plugins((MinimalPlugins, ExplorerTabsPlugin))
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
}

#[cfg(test)]
mod fold_window_tests {
    use crate::fold::{FoldState, indent_regions};
    use ropey::Rope;

    #[test]
    fn collapsed_region_hidden_from_window() {
        let r = Rope::from_str("fn a() {\n    x;\n    y;\n}\nz;\n");
        let mut folds = FoldState::default();
        folds.set_regions(indent_regions(&r));
        folds.close(0);
        let view = folds.view(r.len_lines() as u32);
        let visible = view.lines_for_window(0, view.visible_count());
        assert!(visible.contains(&0));
        assert!(!visible.contains(&1) && !visible.contains(&2));
        assert!(visible.contains(&3));
    }
}

#[cfg(test)]
mod parked_edit_tests {
    use super::*;

    struct Session {
        app: App,
        entity: Entity,
        dir: tempfile::TempDir,
    }

    impl Session {
        fn open(first: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(EditorNavigationPlugin)
                .init_resource::<ExplorerTrees>()
                .add_plugins(EditorFileLifecyclePlugin)
                .add_plugins((EditExecutionPlugin, crate::encoding::EditorEncodingPlugin))
                .init_resource::<BinIpcEventRawBuffer>();
            app.world_mut().insert_non_send(Browsers::default());
            app.world_mut().insert_non_send(ClipboardHandle(None));
            app.world_mut()
                .insert_resource(crate::lsp::manager::LspManager::new(
                    crate::lsp::LspOutbox::default(),
                    crate::lsp::server_request::ServerEvents::default().sender(),
                ));
            let entity = app
                .world_mut()
                .spawn((
                    FileView {
                        path: dir.path().join(first),
                    },
                    FileViewport {
                        top_row: 0,
                        rows: 0,
                        wrap_columns: 0,
                        word_wrap: vmux_core::editor::WordWrap::default(),
                        word_wrap_column: 80,
                    },
                    PageMetadata::default(),
                ))
                .id();
            Self { app, entity, dir }
        }

        fn write(&self, name: &str, text: &str) {
            std::fs::write(self.dir.path().join(name), text).unwrap();
        }

        fn write_bytes(&self, name: &str, bytes: &[u8]) {
            std::fs::write(self.dir.path().join(name), bytes).unwrap();
        }

        fn bytes(&self, name: &str) -> Vec<u8> {
            std::fs::read(self.dir.path().join(name)).unwrap()
        }

        fn encoding(&self) -> FileEncoding {
            self.app
                .world()
                .get::<EditState>(self.entity)
                .expect("a loaded buffer")
                .core
                .buffer
                .encoding
        }

        fn failure(&self) -> Option<LoadFailure> {
            let buf = self.app.world().get::<FileBuffer>(self.entity)?;
            let (reason, _) = LoadFailure::parse(&buf.language)?;
            Some(reason)
        }

        fn encoding_action(&mut self, encoding: FileEncoding, action: FileEncodingAction) {
            self.app.world_mut().trigger(BinReceive {
                webview: self.entity,
                payload: FileEncodingSet { encoding, action },
            });
            self.settle();
        }

        fn navigate_to(&mut self, name: &str) {
            let path = self.dir.path().join(name).to_string_lossy().into_owned();
            self.app.world_mut().trigger(BinReceive {
                webview: self.entity,
                payload: FileOpenEvent { path },
            });
            self.settle();
        }

        fn settle(&mut self) {
            FileLoadTask::settle(&mut self.app, self.entity);
        }

        fn type_into_buffer(&mut self, text: &str) {
            let mut edit = self
                .app
                .world_mut()
                .get_mut::<EditState>(self.entity)
                .expect("a loaded buffer");
            edit.core.apply(EditCommand::InsertText(text.to_string()));
        }

        fn text(&self) -> String {
            self.app
                .world()
                .get::<EditState>(self.entity)
                .unwrap()
                .core
                .buffer
                .text()
        }

        fn undo(&mut self) {
            self.app
                .world_mut()
                .get_mut::<EditState>(self.entity)
                .unwrap()
                .core
                .apply(EditCommand::Undo);
        }
    }

    const SHIFT_JIS_SAMPLE: [u8; 17] = [
        0x93, 0xFA, 0x96, 0x7B, 0x8C, 0xEA, 0x82, 0xCC, 0x83, 0x65, 0x83, 0x4C, 0x83, 0x58, 0x83,
        0x67, 0x0A,
    ];

    #[test]
    fn a_shift_jis_file_opens_and_saves_back_as_shift_jis() {
        let mut s = Session::open("main.txt");
        s.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        s.settle();

        assert_eq!(s.text(), "日本語のテキスト\n", "decoded on load");
        assert_eq!(s.encoding(), FileEncoding::ShiftJis);

        s.type_into_buffer("EDIT");
        s.encoding_action(FileEncoding::ShiftJis, FileEncodingAction::Save);

        let mut expected = b"EDIT".to_vec();
        expected.extend_from_slice(&SHIFT_JIS_SAMPLE);
        assert_eq!(
            s.bytes("main.txt"),
            expected,
            "the file is still shift_jis, not transcoded to utf-8"
        );
    }

    #[test]
    fn saving_a_character_the_encoding_cannot_hold_leaves_the_file_untouched() {
        let mut s = Session::open("main.txt");
        s.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        s.settle();

        s.type_into_buffer("€");
        s.encoding_action(FileEncoding::ShiftJis, FileEncodingAction::Save);

        assert_eq!(
            s.bytes("main.txt"),
            SHIFT_JIS_SAMPLE,
            "a lossy save is refused rather than written with substitutions"
        );
    }

    #[test]
    fn reopening_with_an_encoding_redecodes_the_same_bytes() {
        let mut s = Session::open("main.txt");
        s.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        s.settle();
        assert_eq!(s.encoding(), FileEncoding::ShiftJis);

        s.encoding_action(FileEncoding::EucJp, FileEncodingAction::Reopen);

        assert_eq!(s.encoding(), FileEncoding::EucJp);
        assert_ne!(
            s.text(),
            "日本語のテキスト\n",
            "the override is honoured over what detection chose"
        );
    }

    #[test]
    fn a_file_that_would_not_decode_can_be_reopened_from_the_failure_itself() {
        let mut s = Session::open("main.log");
        s.write_bytes("main.log", b"caf\xe9\x00\x00 log\x00");
        s.settle();

        assert_eq!(s.failure(), Some(LoadFailure::Undecodable));
        assert!(
            s.app.world().get::<EditState>(s.entity).is_none(),
            "no buffer is loaded, so the footer chooser has nothing to hang off"
        );

        s.encoding_action(FileEncoding::Iso8859_1, FileEncodingAction::Reopen);

        assert_eq!(s.failure(), None, "the failure is cleared, not repeated");
        assert_eq!(s.encoding(), FileEncoding::Iso8859_1);
        assert_eq!(s.text(), "café\u{0}\u{0} log\u{0}");
    }

    #[test]
    fn a_failure_no_encoding_can_rescue_is_not_offered_one() {
        let mut s = Session::open("gone.log");
        s.settle();

        assert_eq!(s.failure(), Some(LoadFailure::Fatal));
    }

    #[test]
    fn an_encoding_chosen_for_one_file_does_not_follow_the_pane_to_the_next() {
        let mut s = Session::open("main.txt");
        s.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        s.write("plain.txt", "ascii\n");
        s.settle();

        s.encoding_action(FileEncoding::Utf16Le, FileEncodingAction::Reopen);
        assert_eq!(s.encoding(), FileEncoding::Utf16Le);

        s.navigate_to("plain.txt");

        assert_eq!(s.encoding(), FileEncoding::Utf8);
        assert_eq!(s.text(), "ascii\n");
    }

    #[test]
    fn returning_to_a_file_keeps_its_undo_history() {
        let mut s = Session::open("main.rs");
        s.write("main.rs", "one\n");
        s.write("lib.rs", "two\n");
        s.settle();
        assert_eq!(s.text(), "one\n");

        s.type_into_buffer("EDIT");
        assert_eq!(s.text(), "EDITone\n");

        s.navigate_to("lib.rs");
        assert_eq!(s.text(), "two\n");

        s.navigate_to("main.rs");
        assert_eq!(
            s.text(),
            "EDITone\n",
            "unsaved edit survives the round trip"
        );
        s.undo();
        assert_eq!(s.text(), "one\n", "and so does the undo tree behind it");
    }

    #[test]
    fn a_file_changed_while_parked_is_reloaded() {
        let mut s = Session::open("main.rs");
        s.write("main.rs", "before\n");
        s.write("lib.rs", "other\n");
        s.settle();
        assert_eq!(s.text(), "before\n");

        s.navigate_to("lib.rs");
        std::thread::sleep(std::time::Duration::from_millis(10));
        s.write("main.rs", "changed on disk\n");
        s.navigate_to("main.rs");

        assert_eq!(s.text(), "changed on disk\n");
    }

    #[test]
    fn unsaved_edits_survive_a_file_changing_while_parked() {
        let mut s = Session::open("main.rs");
        s.write("main.rs", "before\n");
        s.write("lib.rs", "other\n");
        s.settle();

        s.type_into_buffer("MINE");
        s.navigate_to("lib.rs");
        std::thread::sleep(std::time::Duration::from_millis(10));
        s.write("main.rs", "theirs\n");
        s.navigate_to("main.rs");

        assert_eq!(s.text(), "MINEbefore\n");
    }

    #[test]
    fn only_the_most_recent_files_are_held() {
        let mut edits = ParkedEdits::default();
        for i in 0..ParkedEdits::CAPACITY + 3 {
            let path = PathBuf::from(format!("/tmp/{i}.rs"));
            let core = EditCore::new(
                path.clone(),
                "Rust".into(),
                "x\n",
                crate::edit::EditMode::Normal,
            );
            edits.insert(
                path,
                ParkedEdit {
                    edit: EditState::new(
                        core,
                        HighlightCache::new(Path::new("/tmp/a.rs")),
                        crate::fold::FoldState::default(),
                    ),
                    diff: vmux_git::GitDiffSource {
                        content: String::new(),
                        dirty: false,
                    },
                    modified: None,
                },
            );
        }
        assert_eq!(edits.by_path.len(), ParkedEdits::CAPACITY);
        assert!(edits.by_path.contains_key(Path::new("/tmp/10.rs")));
        assert!(!edits.by_path.contains_key(Path::new("/tmp/0.rs")));
    }
}

#[cfg(test)]
mod host_history_tests {
    use super::*;
    use vmux_core::PageOpenId;
    use vmux_core::host::page::{HostHistory, HostHistoryDelta, HostHistoryStep};

    struct Editor {
        app: App,
        view: Entity,
        dir: tempfile::TempDir,
    }

    impl Editor {
        fn opened(name: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(vmux_core::CorePlugin)
                .add_plugins(EditorNavigationPlugin)
                .add_plugins(EditorHistoryPlugin)
                .add_plugins(EditorPageOpenPlugin);
            app.world_mut()
                .insert_resource(crate::lsp::manager::LspManager::new(
                    crate::lsp::LspOutbox::default(),
                    crate::lsp::server_request::ServerEvents::default().sender(),
                ));
            let stack = app.world_mut().spawn_empty().id();
            app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: format!("file://{}", dir.path().join(name).display()),
                request_id: None,
            });
            app.update();
            app.update();
            let view = app
                .world_mut()
                .query_filtered::<Entity, With<FileView>>()
                .single(app.world())
                .expect("opening a file:// page spawns one file view");
            Self { app, view, dir }
        }

        fn open(&mut self, name: &str) {
            let path = self.dir.path().join(name).to_string_lossy().into_owned();
            self.app.world_mut().trigger(BinReceive {
                webview: self.view,
                payload: FileOpenEvent { path },
            });
            self.app.update();
        }

        fn scroll_to(&mut self, top_row: u32) {
            self.app
                .world_mut()
                .get_mut::<FileViewport>(self.view)
                .expect("a file view has a viewport")
                .top_row = top_row;
            self.app.update();
        }

        fn step(&mut self, delta: HostHistoryDelta) {
            let webview = self.view;
            self.app
                .world_mut()
                .write_message(HostHistoryStep { webview, delta });
            self.app.update();
        }

        fn showing(&self) -> String {
            self.app
                .world()
                .get::<FileView>(self.view)
                .expect("a file view")
                .path
                .file_name()
                .expect("a named file")
                .to_string_lossy()
                .into_owned()
        }

        fn top_row(&self) -> u32 {
            self.app
                .world()
                .get::<FileViewport>(self.view)
                .expect("a file view has a viewport")
                .top_row
        }

        fn history(&self) -> &HostHistory {
            self.app
                .world()
                .get::<HostHistory>(self.view)
                .expect("a file view owns its history")
        }
    }

    #[test]
    fn back_and_forward_walk_the_files_the_editor_opened() {
        let mut editor = Editor::opened("a.rs");
        editor.open("b.rs");
        editor.open("c.rs");
        assert!(editor.history().can_go_back());
        assert!(!editor.history().can_go_forward());

        editor.step(HostHistoryDelta::Back);
        assert_eq!(editor.showing(), "b.rs");
        assert!(editor.history().can_go_forward());

        editor.step(HostHistoryDelta::Back);
        assert_eq!(editor.showing(), "a.rs");
        assert!(!editor.history().can_go_back());

        editor.step(HostHistoryDelta::Forward);
        assert_eq!(editor.showing(), "b.rs");
    }

    #[test]
    fn opening_a_file_after_going_back_drops_the_forward_trail() {
        let mut editor = Editor::opened("a.rs");
        editor.open("b.rs");
        editor.open("c.rs");
        editor.step(HostHistoryDelta::Back);
        editor.step(HostHistoryDelta::Back);

        editor.open("d.rs");

        assert!(!editor.history().can_go_forward());
        editor.step(HostHistoryDelta::Back);
        assert_eq!(editor.showing(), "a.rs");
        editor.step(HostHistoryDelta::Forward);
        assert_eq!(editor.showing(), "d.rs");
    }

    #[test]
    fn going_back_lands_on_the_line_the_file_was_left_at() {
        let mut editor = Editor::opened("a.rs");
        editor.scroll_to(120);
        editor.open("b.rs");
        assert_eq!(editor.top_row(), 0);

        editor.step(HostHistoryDelta::Back);

        assert_eq!(editor.showing(), "a.rs");
        assert_eq!(editor.top_row(), 120);
    }
}

#[cfg(test)]
mod open_editor_tests {
    use super::*;

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
        let mut st = ExplorerState::holding(&["/a", "/b", "/c"]);
        assert_eq!(
            st.close_editor(Path::new("/b")),
            Some(PathBuf::from("/c")),
            "closing a middle tab moves right, as the tab strip reads"
        );
        assert_eq!(
            st.close_editor(Path::new("/c")),
            Some(PathBuf::from("/a")),
            "closing the last tab falls back to the one on its left"
        );
        assert_eq!(st.close_editor(Path::new("/a")), None);
        assert!(st.open_editors.is_empty());
    }

    #[test]
    fn closing_an_editor_that_was_never_open_changes_nothing() {
        let mut st = ExplorerState::holding(&["/a"]);
        assert_eq!(st.close_editor(Path::new("/zzz")), None);
        assert_eq!(st.open_editors, vec![PathBuf::from("/a")]);
    }
}
