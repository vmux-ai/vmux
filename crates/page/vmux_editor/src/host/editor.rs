use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vmux_core::PageMetadata;

use crate::host::edit::EditCore;
use crate::host::edit::highlight_cache::HighlightCache;
use crate::host::file_lifecycle::{FileBuffer, FileDir, FileLoadTask};
use crate::host::keymap::EditorKeymap;
use crate::host::language::LspEditDirty;
use crate::host::note::NoteSent;
use crate::host::status::FileInitialMetaSent;
use crate::host::viewport::FileViewport;
use crate::media::FileMedia;
use crate::wrap::WrapView;

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
            .map(|name| name.to_string_lossy().to_string())
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
pub struct Editor {
    pub core: EditCore,
    pub hl: HighlightCache,
    pub folds: crate::fold::FoldState,
    indent_width: u16,
    parsed_note: Option<crate::markdown::ParsedNote>,
    wrap_generation: u64,
    wrap_cache: Option<CachedWrapView>,
}

impl Editor {
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

    pub(super) fn is_note(&self) -> bool {
        self.parsed_note.is_some()
    }

    pub(super) fn note_blocks(&self) -> Option<&[vmux_core::event::NoteBlock]> {
        self.parsed_note.as_ref().map(|note| note.blocks.as_slice())
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

    pub(super) fn refresh_parsed_note(&mut self) {
        self.parsed_note = crate::markdown::is_markdown_path(&self.core.buffer.path)
            .then(|| crate::markdown::parse_note_document(&self.core.buffer.text()));
    }
}

#[derive(Component, Default)]
pub(super) struct ParkedEdits {
    pub(super) by_path: HashMap<PathBuf, ParkedEdit>,
    recent: Vec<PathBuf>,
}

pub(super) struct ParkedEdit {
    pub(super) edit: Editor,
    pub(super) diff: vmux_git::GitDiffSource,
    pub(super) modified: Option<std::time::SystemTime>,
}

impl ParkedEdits {
    pub(super) const CAPACITY: usize = 8;

    fn park(entity: &mut EntityWorldMut, path: PathBuf) {
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
            modified: Self::modified_at(&path),
        };
        let mut edits = entity.take::<ParkedEdits>().unwrap_or_default();
        edits.insert(path, parked);
        entity.insert(edits);
    }

    pub(super) fn insert(&mut self, path: PathBuf, edit: ParkedEdit) {
        self.recent.retain(|recent| recent != &path);
        self.recent.push(path.clone());
        self.by_path.insert(path, edit);
        while self.recent.len() > Self::CAPACITY {
            let evicted = self.recent.remove(0);
            self.by_path.remove(&evicted);
        }
    }

    pub(super) fn resume(&mut self, path: &Path) -> Option<ParkedEdit> {
        let parked = self.by_path.remove(path)?;
        self.recent.retain(|recent| recent != path);
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::{BinIpcEventRawBuffer, BinReceive, Browsers};
    use vmux_core::PageMetadata;
    use vmux_core::event::{FileEncoding, FileEncodingAction, FileEncodingSet, FileOpenEvent};

    use crate::host::edit::{EditCommand, EditMode};
    use crate::host::editing::{ClipboardHandle, EditExecutionPlugin};
    use crate::host::explorer::ExplorerTrees;
    use crate::host::file_lifecycle::{
        EditorFileLifecyclePlugin, FileBuffer, FileLoadTask, LoadFailure,
    };
    use crate::host::navigation::EditorNavigationPlugin;

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
                .get::<Editor>(self.entity)
                .expect("a loaded buffer")
                .core
                .buffer
                .encoding
        }

        fn failure(&self) -> Option<LoadFailure> {
            let buffer = self.app.world().get::<FileBuffer>(self.entity)?;
            let (reason, _) = LoadFailure::parse(&buffer.language)?;
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
                .get_mut::<Editor>(self.entity)
                .expect("a loaded buffer");
            edit.core.apply(EditCommand::InsertText(text.to_string()));
        }

        fn text(&self) -> String {
            self.app
                .world()
                .get::<Editor>(self.entity)
                .unwrap()
                .core
                .buffer
                .text()
        }

        fn undo(&mut self) {
            self.app
                .world_mut()
                .get_mut::<Editor>(self.entity)
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
        let mut session = Session::open("main.txt");
        session.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        session.settle();

        assert_eq!(session.text(), "日本語のテキスト\n", "decoded on load");
        assert_eq!(session.encoding(), FileEncoding::ShiftJis);

        session.type_into_buffer("EDIT");
        session.encoding_action(FileEncoding::ShiftJis, FileEncodingAction::Save);

        let mut expected = b"EDIT".to_vec();
        expected.extend_from_slice(&SHIFT_JIS_SAMPLE);
        assert_eq!(
            session.bytes("main.txt"),
            expected,
            "the file is still shift_jis, not transcoded to utf-8"
        );
    }

    #[test]
    fn saving_a_character_the_encoding_cannot_hold_leaves_the_file_untouched() {
        let mut session = Session::open("main.txt");
        session.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        session.settle();

        session.type_into_buffer("€");
        session.encoding_action(FileEncoding::ShiftJis, FileEncodingAction::Save);

        assert_eq!(
            session.bytes("main.txt"),
            SHIFT_JIS_SAMPLE,
            "a lossy save is refused rather than written with substitutions"
        );
    }

    #[test]
    fn reopening_with_an_encoding_redecodes_the_same_bytes() {
        let mut session = Session::open("main.txt");
        session.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        session.settle();
        assert_eq!(session.encoding(), FileEncoding::ShiftJis);

        session.encoding_action(FileEncoding::EucJp, FileEncodingAction::Reopen);

        assert_eq!(session.encoding(), FileEncoding::EucJp);
        assert_ne!(
            session.text(),
            "日本語のテキスト\n",
            "the override is honoured over what detection chose"
        );
    }

    #[test]
    fn a_file_that_would_not_decode_can_be_reopened_from_the_failure_itself() {
        let mut session = Session::open("main.log");
        session.write_bytes("main.log", b"caf\xe9\x00\x00 log\x00");
        session.settle();

        assert_eq!(session.failure(), Some(LoadFailure::Undecodable));
        assert!(
            session.app.world().get::<Editor>(session.entity).is_none(),
            "no buffer is loaded, so the footer chooser has nothing to hang off"
        );

        session.encoding_action(FileEncoding::Iso8859_1, FileEncodingAction::Reopen);

        assert_eq!(
            session.failure(),
            None,
            "the failure is cleared, not repeated"
        );
        assert_eq!(session.encoding(), FileEncoding::Iso8859_1);
        assert_eq!(session.text(), "café\u{0}\u{0} log\u{0}");
    }

    #[test]
    fn a_failure_no_encoding_can_rescue_is_not_offered_one() {
        let mut session = Session::open("gone.log");
        session.settle();

        assert_eq!(session.failure(), Some(LoadFailure::Fatal));
    }

    #[test]
    fn an_encoding_chosen_for_one_file_does_not_follow_the_pane_to_the_next() {
        let mut session = Session::open("main.txt");
        session.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        session.write("plain.txt", "ascii\n");
        session.settle();

        session.encoding_action(FileEncoding::Utf16Le, FileEncodingAction::Reopen);
        assert_eq!(session.encoding(), FileEncoding::Utf16Le);

        session.navigate_to("plain.txt");

        assert_eq!(session.encoding(), FileEncoding::Utf8);
        assert_eq!(session.text(), "ascii\n");
    }

    #[test]
    fn returning_to_a_file_keeps_its_undo_history() {
        let mut session = Session::open("main.rs");
        session.write("main.rs", "one\n");
        session.write("lib.rs", "two\n");
        session.settle();
        assert_eq!(session.text(), "one\n");

        session.type_into_buffer("EDIT");
        assert_eq!(session.text(), "EDITone\n");

        session.navigate_to("lib.rs");
        assert_eq!(session.text(), "two\n");

        session.navigate_to("main.rs");
        assert_eq!(
            session.text(),
            "EDITone\n",
            "unsaved edit survives the round trip"
        );
        session.undo();
        assert_eq!(
            session.text(),
            "one\n",
            "and so does the undo tree behind it"
        );
    }

    #[test]
    fn a_file_changed_while_parked_is_reloaded() {
        let mut session = Session::open("main.rs");
        session.write("main.rs", "before\n");
        session.write("lib.rs", "other\n");
        session.settle();
        assert_eq!(session.text(), "before\n");

        session.navigate_to("lib.rs");
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.write("main.rs", "changed on disk\n");
        session.navigate_to("main.rs");

        assert_eq!(session.text(), "changed on disk\n");
    }

    #[test]
    fn unsaved_edits_survive_a_file_changing_while_parked() {
        let mut session = Session::open("main.rs");
        session.write("main.rs", "before\n");
        session.write("lib.rs", "other\n");
        session.settle();

        session.type_into_buffer("MINE");
        session.navigate_to("lib.rs");
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.write("main.rs", "theirs\n");
        session.navigate_to("main.rs");

        assert_eq!(session.text(), "MINEbefore\n");
    }

    #[test]
    fn only_the_most_recent_files_are_held() {
        let mut edits = ParkedEdits::default();
        for index in 0..ParkedEdits::CAPACITY + 3 {
            let path = PathBuf::from(format!("/tmp/{index}.rs"));
            let core = EditCore::new(path.clone(), "Rust".into(), "x\n", EditMode::Normal);
            edits.insert(
                path,
                ParkedEdit {
                    edit: Editor::new(
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
