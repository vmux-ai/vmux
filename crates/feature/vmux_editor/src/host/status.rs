use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::*;

use crate::host::editor::{Editor, FileDocumentRevision, FileView};
use crate::host::file_lifecycle::{EditorFileLoadedSet, FileBuffer};
use crate::host::keymap::KeymapConfig;
use crate::host::note::{NoteRevealLine, NoteSent};
use crate::host::viewport::{CursorRenderRequest, FileViewport, ViewportRenderRequest};

pub(crate) struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_shared_file_view_mode)
            .add_message::<vmux_setting::SettingsWriteRequest>()
            .add_plugins(UiEventPlugin::<(FileViewModeSet, FileKeymapSet)>::default())
            .add_systems(
                Update,
                (send_initial_meta, send_initial_text_meta).after(EditorFileLoadedSet),
            )
            .add_systems(
                Update,
                (
                    (resend_file_theme_on_change, send_file_theme).chain(),
                    apply_file_view_mode_requests.before(send_file_view_mode),
                    send_file_view_mode,
                    send_file_keymap,
                ),
            )
            .add_observer(on_file_view_mode_set)
            .add_observer(on_file_keymap_set);
    }
}

fn spawn_shared_file_view_mode(mut commands: Commands) {
    commands.spawn((
        Name::new("Shared file view mode"),
        SharedFileViewMode::default(),
        FileViewModeRevision::default(),
    ));
}

#[derive(Component)]
pub(crate) struct FileInitialMetaSent;

#[derive(Component)]
pub(crate) struct FileThemeSent;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SharedFileViewMode(pub(crate) FileViewMode);

impl Default for SharedFileViewMode {
    fn default() -> Self {
        Self(FileViewMode::Note)
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct FileViewModeRevision(pub(crate) u64);

impl Default for FileViewModeRevision {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileViewModeRequest(pub FileViewMode);

#[derive(Component)]
pub(crate) struct FileViewModeSent;

#[derive(Component)]
pub(crate) struct FileKeymapSent;

type ReadyUnsentMeta = (
    Without<FileInitialMetaSent>,
    With<vmux_core::page::PageReady>,
);
type ReadyUnsentTheme = (
    With<FileView>,
    Without<FileThemeSent>,
    With<vmux_core::page::PageReady>,
);
type ReadyUnsentViewMode = (
    With<FileView>,
    Without<FileViewModeSent>,
    With<vmux_core::page::PageReady>,
);
type ReadySentViewMode = (
    With<FileView>,
    With<FileViewModeSent>,
    With<vmux_core::page::PageReady>,
);
type ReadyUnsentKeymap = (
    With<FileView>,
    Without<FileKeymapSent>,
    With<vmux_core::page::PageReady>,
);
type ReadySentKeymap = (
    With<FileView>,
    With<FileKeymapSent>,
    With<vmux_core::page::PageReady>,
);

fn send_initial_meta(
    buffers: Query<(Entity, &FileBuffer), ReadyUnsentMeta>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, buffer) in &buffers {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        if let Some((undecodable, message)) = buffer.load_error() {
            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                entity,
                &FileErrorEvent {
                    message: message.to_string(),
                    undecodable,
                },
            ));
        }
        commands.entity(entity).insert(FileInitialMetaSent);
    }
}

fn send_initial_text_meta(
    mut files: Query<
        (
            Entity,
            &FileView,
            &FileDocumentRevision,
            &Editor,
            &FileViewport,
        ),
        ReadyUnsentMeta,
    >,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, file, revision, edit, viewport) in &mut files {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let shape = crate::shape::BufferShape::detect(&edit.core.buffer.rope);
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity,
            &FileMetaEvent {
                revision: revision.get(),
                path: file.display_path(),
                abs_path: file.path.to_string_lossy().into_owned(),
                kind: file.document_kind(),
                language: edit.core.buffer.language.clone(),
                total_lines: edit.core.buffer.len_lines() as u32,
                indent: shape.indent,
                line_ending: shape.line_ending,
                encoding: edit.core.buffer.encoding,
            },
        ));
        if viewport.rows > 0 {
            commands.trigger(ViewportRenderRequest::new(entity));
        }
        commands.trigger(CursorRenderRequest::new(entity));
        commands.entity(entity).insert(FileInitialMetaSent);
    }
}

fn resend_file_theme_on_change(
    sent: Query<Entity, With<FileThemeSent>>,
    settings: Res<vmux_setting::AppSettings>,
    mut commands: Commands,
) {
    if !settings.is_changed() || settings.is_added() {
        return;
    }
    for entity in &sent {
        commands.entity(entity).remove::<FileThemeSent>();
    }
}

fn send_file_theme(
    pending: Query<Entity, ReadyUnsentTheme>,
    settings: Res<vmux_setting::AppSettings>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for entity in &pending {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let (font_family, font_size, line_height) = settings
            .terminal
            .as_ref()
            .map(|terminal| {
                let theme = terminal.resolve_theme(&terminal.default_theme);
                (
                    theme.font_family.clone(),
                    theme.font_size,
                    theme.line_height,
                )
            })
            .unwrap_or_else(|| (String::new(), 0.0, 0.0));
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity,
            &FileThemeEvent {
                font_family,
                font_size,
                line_height,
            },
        ));
        commands.entity(entity).insert(FileThemeSent);
    }
}

fn send_file_view_mode(
    state: Single<(Ref<SharedFileViewMode>, Ref<FileViewModeRevision>)>,
    pending: Query<Entity, ReadyUnsentViewMode>,
    sent: Query<Entity, ReadySentViewMode>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let (mode, revision) = state.into_inner();
    let event = FileViewModeEvent {
        mode: mode.0,
        revision: revision.0,
    };
    for entity in &pending {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity, &event,
        ));
        commands.entity(entity).insert(FileViewModeSent);
    }
    if mode.is_changed() || revision.is_changed() {
        for entity in &sent {
            if !browsers.can_emit_to(&entity) {
                continue;
            }
            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                entity, &event,
            ));
        }
    }
}

fn send_file_keymap(
    settings: Option<Res<vmux_setting::AppSettings>>,
    pending: Query<Entity, ReadyUnsentKeymap>,
    sent: Query<Entity, ReadySentKeymap>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let event = FileKeymapEvent {
        keymap: KeymapConfig::resolve(settings.as_deref()).kind(),
    };
    for entity in &pending {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity, &event,
        ));
        commands.entity(entity).insert(FileKeymapSent);
    }
    if settings
        .as_ref()
        .is_some_and(|settings| settings.is_changed())
    {
        for entity in &sent {
            if !browsers.can_emit_to(&entity) {
                continue;
            }
            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                entity, &event,
            ));
        }
    }
}

fn apply_file_view_mode_requests(
    mut requests: MessageReader<FileViewModeRequest>,
    state: Single<(&mut SharedFileViewMode, &mut FileViewModeRevision)>,
) {
    if let Some(request) = requests.read().last() {
        let (mut mode, mut revision) = state.into_inner();
        mode.0 = request.0;
        revision.0 = revision.0.wrapping_add(1).max(1);
    }
}

fn on_file_view_mode_set(
    trigger: On<UiInput<FileViewModeSet>>,
    files: Query<(&FileView, Option<&Editor>)>,
    state: Single<(&mut SharedFileViewMode, &mut FileViewModeRevision)>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((file, edit)) = files.get(entity) else {
        return;
    };
    let (mut mode, mut revision) = state.into_inner();
    mode.0 = trigger.event().payload.mode;
    revision.0 = revision.0.wrapping_add(1).max(1);
    if mode.0 != FileViewMode::Note || !crate::markdown::is_markdown_path(&file.path) {
        return;
    }
    let reveal_line = edit.map(Editor::cursor_line);
    let mut entity_commands = commands.entity(entity);
    entity_commands.remove::<NoteSent>();
    if let Some(line) = reveal_line {
        entity_commands.insert(NoteRevealLine(line));
    }
}

fn on_file_keymap_set(
    trigger: On<UiInput<FileKeymapSet>>,
    views: Query<(), With<FileView>>,
    mut settings: ResMut<vmux_setting::AppSettings>,
    mut writes: MessageWriter<vmux_setting::SettingsWriteRequest>,
) {
    if !views.contains(trigger.event().webview) {
        return;
    }
    let keymap = trigger.event().payload.keymap;
    if settings.editor.keymap == keymap {
        return;
    }
    match settings.apply_update(
        "editor.keymap",
        serde_json::to_value(keymap).unwrap_or_default(),
    ) {
        Ok(ron_bytes) => {
            writes.write(vmux_setting::SettingsWriteRequest { ron_bytes });
        }
        Err(error) => bevy::log::warn!("editor: keymap update rejected: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::host::edit::highlight_cache::HighlightCache;
    use crate::host::edit::{EditCommand, EditCore, Motion};

    #[test]
    fn file_view_mode_is_shared_across_editors() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_observer(on_file_view_mode_set);
        app.world_mut().spawn((
            SharedFileViewMode::default(),
            FileViewModeRevision::default(),
        ));
        let first = app
            .world_mut()
            .spawn(FileView {
                path: PathBuf::from("/a.rs"),
            })
            .id();
        let second = app
            .world_mut()
            .spawn(FileView {
                path: PathBuf::from("/b.rs"),
            })
            .id();

        app.world_mut().trigger(UiInput {
            webview: first,
            payload: FileViewModeSet {
                mode: FileViewMode::Diff,
            },
        });

        let mut modes = app.world_mut().query::<&SharedFileViewMode>();
        assert_eq!(modes.single(app.world()).unwrap().0, FileViewMode::Diff);
        assert!(app.world().get::<FileView>(second).is_some());
        let mut revisions = app.world_mut().query::<&FileViewModeRevision>();
        assert_eq!(revisions.single(app.world()).unwrap().0, 2);

        app.world_mut().trigger(UiInput {
            webview: first,
            payload: FileViewModeSet {
                mode: FileViewMode::Diff,
            },
        });

        let mut revisions = app.world_mut().query::<&FileViewModeRevision>();
        assert_eq!(revisions.single(app.world()).unwrap().0, 3);
    }

    #[test]
    fn switching_to_note_reveals_the_current_cursor_line() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_observer(on_file_view_mode_set);
        app.world_mut().spawn((
            SharedFileViewMode(FileViewMode::Editor),
            FileViewModeRevision::default(),
        ));

        let path = PathBuf::from("/note.md");
        let mut core = EditCore::new(
            path.clone(),
            "Markdown".into(),
            "one\ntwo\nthree\n",
            crate::host::edit::EditMode::Normal,
        );
        core.apply(EditCommand::Move(Motion::GotoLine(2)));
        let entity = app
            .world_mut()
            .spawn((
                FileView { path: path.clone() },
                Editor::new(
                    core,
                    HighlightCache::new(&path),
                    crate::host::fold::FoldState::default(),
                ),
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview: entity,
            payload: FileViewModeSet {
                mode: FileViewMode::Note,
            },
        });
        app.update();

        assert_eq!(
            app.world().get::<NoteRevealLine>(entity).map(|line| line.0),
            Some(2)
        );
    }

    #[test]
    fn file_view_mode_request_updates_shared_mode() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<FileViewModeRequest>()
            .add_systems(Update, apply_file_view_mode_requests);
        app.world_mut().spawn((
            SharedFileViewMode::default(),
            FileViewModeRevision::default(),
        ));

        app.world_mut()
            .resource_mut::<Messages<FileViewModeRequest>>()
            .write(FileViewModeRequest(FileViewMode::Diff));
        app.update();

        let mut modes = app.world_mut().query::<&SharedFileViewMode>();
        assert_eq!(modes.single(app.world()).unwrap().0, FileViewMode::Diff);
        let mut revisions = app.world_mut().query::<&FileViewModeRevision>();
        assert_eq!(revisions.single(app.world()).unwrap().0, 2);

        app.world_mut()
            .resource_mut::<Messages<FileViewModeRequest>>()
            .write(FileViewModeRequest(FileViewMode::Diff));
        app.update();

        let mut revisions = app.world_mut().query::<&FileViewModeRevision>();
        assert_eq!(revisions.single(app.world()).unwrap().0, 3);
    }

    #[test]
    fn non_editor_cannot_change_file_view_mode() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_observer(on_file_view_mode_set);
        app.world_mut().spawn((
            SharedFileViewMode::default(),
            FileViewModeRevision::default(),
        ));
        let other = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(UiInput {
            webview: other,
            payload: FileViewModeSet {
                mode: FileViewMode::Diff,
            },
        });

        let mut modes = app.world_mut().query::<&SharedFileViewMode>();
        assert_eq!(modes.single(app.world()).unwrap().0, FileViewMode::Note);
    }

    #[test]
    fn file_view_mode_defaults_to_note() {
        assert_eq!(SharedFileViewMode::default().0, FileViewMode::Note);
    }
}
