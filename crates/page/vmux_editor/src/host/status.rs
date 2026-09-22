use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::*;

use crate::host::dir::parent_listing;
use crate::host::note::{NoteRevealLine, NoteSent};
use crate::host::plugin::{
    EditState, EditorFileLoadedSet, EditorKeymap, FileBuffer, FileDir, FileView,
};
use crate::host::viewport::{EditorCursor, EditorWindow, FileViewport};

pub(crate) struct EditorStatusPlugin;

impl Plugin for EditorStatusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SharedFileViewMode>()
            .add_message::<vmux_setting::SettingsWriteRequest>()
            .add_plugins(UiEventPlugin::<(FileViewModeSet, FileKeymapSet)>::default())
            .add_systems(
                Update,
                (send_initial_meta, send_initial_text_meta, send_initial_dir)
                    .after(EditorFileLoadedSet),
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

#[derive(Component)]
pub(crate) struct FileInitialMetaSent;

#[derive(Component)]
pub(crate) struct FileThemeSent;

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SharedFileViewMode(pub(crate) FileViewMode);

impl Default for SharedFileViewMode {
    fn default() -> Self {
        Self(FileViewMode::Note)
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
            commands.trigger(BinHostEmitEvent::from_event(
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
            &mut EditState,
            &EditorKeymap,
            &FileViewport,
        ),
        ReadyUnsentMeta,
    >,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, file, mut edit, keymap, viewport) in &mut files {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let shape = crate::shape::BufferShape::detect(&edit.core.buffer.rope);
        commands.trigger(BinHostEmitEvent::from_event(
            entity,
            &FileMetaEvent {
                path: file.display_path(),
                abs_path: file.path.to_string_lossy().into_owned(),
                language: edit.core.buffer.language.clone(),
                total_lines: edit.core.buffer.len_lines() as u32,
                indent: shape.indent,
                line_ending: shape.line_ending,
                encoding: edit.core.buffer.encoding,
            },
        ));
        if viewport.rows > 0 {
            EditorWindow::emit(entity, &mut edit, viewport, &browsers, &mut commands);
        }
        EditorCursor::emit(
            entity,
            &mut edit,
            keymap.0.as_ref(),
            viewport,
            &browsers,
            &mut commands,
        );
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
        commands.trigger(BinHostEmitEvent::from_event(
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
    mode: Res<SharedFileViewMode>,
    pending: Query<Entity, ReadyUnsentViewMode>,
    sent: Query<Entity, ReadySentViewMode>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let event = FileViewModeEvent { mode: mode.0 };
    for entity in &pending {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_event(entity, &event));
        commands.entity(entity).insert(FileViewModeSent);
    }
    if mode.is_changed() {
        for entity in &sent {
            if !browsers.can_emit_to(&entity) {
                continue;
            }
            commands.trigger(BinHostEmitEvent::from_event(entity, &event));
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
        keymap: EditorKeymap::configured_kind(&settings),
    };
    for entity in &pending {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_event(entity, &event));
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
            commands.trigger(BinHostEmitEvent::from_event(entity, &event));
        }
    }
}

fn apply_file_view_mode_requests(
    mut requests: MessageReader<FileViewModeRequest>,
    mut mode: ResMut<SharedFileViewMode>,
) {
    if let Some(request) = requests.read().last() {
        mode.0 = request.0;
    }
}

fn send_initial_dir(
    dirs: Query<(Entity, &FileView, &FileDir), ReadyUnsentMeta>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, file, dir) in &dirs {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let (parent_path, parent_entries) = parent_listing(&file.path);
        commands.trigger(BinHostEmitEvent::from_event(
            entity,
            &FileDirEvent {
                path: file.display_path(),
                abs_path: file.path.to_string_lossy().into_owned(),
                entries: dir.entries.clone(),
                parent_path,
                parent_entries,
            },
        ));
        commands.entity(entity).insert(FileInitialMetaSent);
    }
}

fn on_file_view_mode_set(
    trigger: On<BinReceive<FileViewModeSet>>,
    files: Query<(&FileView, Option<&EditState>)>,
    mut mode: ResMut<SharedFileViewMode>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((file, edit)) = files.get(entity) else {
        return;
    };
    mode.0 = trigger.event().payload.mode;
    if mode.0 != FileViewMode::Note || !crate::markdown::is_markdown_path(&file.path) {
        return;
    }
    let reveal_line = edit.map(EditState::cursor_line);
    let mut entity_commands = commands.entity(entity);
    entity_commands.remove::<NoteSent>();
    if let Some(line) = reveal_line {
        entity_commands.insert(NoteRevealLine(line));
    }
}

fn on_file_keymap_set(
    trigger: On<BinReceive<FileKeymapSet>>,
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
            .init_resource::<SharedFileViewMode>()
            .add_observer(on_file_view_mode_set);
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

        app.world_mut().trigger(BinReceive {
            webview: first,
            payload: FileViewModeSet {
                mode: FileViewMode::Diff,
            },
        });

        assert_eq!(
            app.world().resource::<SharedFileViewMode>().0,
            FileViewMode::Diff
        );
        assert!(app.world().get::<FileView>(second).is_some());
    }

    #[test]
    fn switching_to_note_reveals_the_current_cursor_line() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SharedFileViewMode>()
            .add_observer(on_file_view_mode_set);
        app.world_mut().resource_mut::<SharedFileViewMode>().0 = FileViewMode::Editor;

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
                EditState::new(
                    core,
                    HighlightCache::new(&path),
                    crate::host::fold::FoldState::default(),
                ),
            ))
            .id();

        app.world_mut().trigger(BinReceive {
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
            .init_resource::<SharedFileViewMode>()
            .add_message::<FileViewModeRequest>()
            .add_systems(Update, apply_file_view_mode_requests);

        app.world_mut()
            .resource_mut::<Messages<FileViewModeRequest>>()
            .write(FileViewModeRequest(FileViewMode::Diff));
        app.update();

        assert_eq!(
            app.world().resource::<SharedFileViewMode>().0,
            FileViewMode::Diff
        );
    }

    #[test]
    fn non_editor_cannot_change_file_view_mode() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SharedFileViewMode>()
            .add_observer(on_file_view_mode_set);
        let other = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(BinReceive {
            webview: other,
            payload: FileViewModeSet {
                mode: FileViewMode::Diff,
            },
        });

        assert_eq!(
            app.world().resource::<SharedFileViewMode>().0,
            FileViewMode::Note
        );
    }

    #[test]
    fn file_view_mode_defaults_to_note() {
        assert_eq!(SharedFileViewMode::default().0, FileViewMode::Note);
    }
}
