use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vmux_core::PageMetadata;

use crate::host::edit::EditCore;
use crate::host::edit::highlight_cache::HighlightCache;
use crate::host::viewport::FileViewport;
use crate::wrap::WrapView;

#[derive(Component, Clone, Debug)]
#[require(vmux_core::host::FileUiStateUpdates, FileDocumentRevision)]
pub struct FileView {
    pub path: PathBuf,
}

#[derive(EntityEvent)]
pub(crate) struct FileNavigateRequest {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) path: PathBuf,
    pub(crate) top_line: u32,
    pub(crate) page_url: Option<String>,
}

impl FileNavigateRequest {
    pub(crate) fn new(entity: Entity, path: PathBuf, top_line: u32) -> Self {
        Self {
            entity,
            path,
            top_line,
            page_url: None,
        }
    }

    pub(crate) fn with_page_url(mut self, page_url: String) -> Self {
        self.page_url = Some(page_url);
        self
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileDocumentRevision(u64);

impl Default for FileDocumentRevision {
    fn default() -> Self {
        Self(1)
    }
}

impl FileDocumentRevision {
    fn advance(&mut self) {
        self.0 = self.0.wrapping_add(1).max(1);
    }

    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

impl FileView {
    pub(super) fn in_stack(
        stack: Entity,
        children_q: &Query<&Children>,
        views: &Query<(&FileView, &mut PageMetadata)>,
    ) -> Option<Entity> {
        let Ok(children) = children_q.get(stack) else {
            return None;
        };
        children.iter().find(|&child| views.contains(child))
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

    pub(crate) fn document_kind(&self) -> vmux_core::event::FileDocumentKind {
        match crate::markdown::is_markdown_path(&self.path) {
            true => vmux_core::event::FileDocumentKind::Markdown,
            false => vmux_core::event::FileDocumentKind::Text,
        }
    }

    pub(crate) fn replace_path(
        &mut self,
        path: PathBuf,
        revision: &mut FileDocumentRevision,
    ) -> PathBuf {
        if self.path != path {
            revision.advance();
        }
        std::mem::replace(&mut self.path, path)
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

    pub(crate) fn modified_at(path: &Path) -> Option<std::time::SystemTime> {
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
    use bevy_cef::prelude::{BinIpcEventRawBuffer, Browsers, UiInput};
    use vmux_core::PageMetadata;
    use vmux_core::event::{FileEncoding, FileEncodingOperation, FileEncodingSet, FileOpenEvent};

    use crate::host::edit::{EditCommand, EditMode};
    use crate::host::editing::{ClipboardHandle, EditExecutionPlugin};
    use crate::host::explorer::ExplorerTrees;
    use crate::host::file_lifecycle::{FileBuffer, FileLifecyclePlugin, FileLoadTask, LoadFailure};
    use crate::host::navigation::NavigationPlugin;

    #[test]
    fn document_revision_changes_only_when_the_path_changes() {
        let mut view = FileView {
            path: PathBuf::from("/w/src/main.rs"),
        };
        let mut revision = FileDocumentRevision::default();

        view.replace_path(PathBuf::from("/w/src/main.rs"), &mut revision);
        assert_eq!(revision.get(), 1);

        view.replace_path(PathBuf::from("/w/src/lib.rs"), &mut revision);
        assert_eq!(revision.get(), 2);
    }

    #[test]
    fn document_kind_is_owned_by_the_file_view() {
        let markdown = FileView {
            path: PathBuf::from("/w/notes/readme.MDX"),
        };
        let source = FileView {
            path: PathBuf::from("/w/src/main.rs"),
        };

        assert_eq!(
            markdown.document_kind(),
            vmux_core::event::FileDocumentKind::Markdown
        );
        assert_eq!(
            source.document_kind(),
            vmux_core::event::FileDocumentKind::Text
        );
    }

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
                .add_plugins(NavigationPlugin)
                .init_resource::<ExplorerTrees>()
                .add_plugins(FileLifecyclePlugin)
                .add_plugins((EditExecutionPlugin, crate::encoding::EncodingPlugin))
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

        fn apply_encoding(&mut self, encoding: FileEncoding, operation: FileEncodingOperation) {
            self.app.world_mut().trigger(UiInput {
                webview: self.entity,
                payload: FileEncodingSet {
                    encoding,
                    operation,
                },
            });
            self.settle();
        }

        fn navigate_to(&mut self, name: &str) {
            let path = self.dir.path().join(name).to_string_lossy().into_owned();
            self.app.world_mut().trigger(UiInput {
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
        session.apply_encoding(FileEncoding::ShiftJis, FileEncodingOperation::Save);

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
        session.apply_encoding(FileEncoding::ShiftJis, FileEncodingOperation::Save);

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

        session.apply_encoding(FileEncoding::EucJp, FileEncodingOperation::Reopen);

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

        session.apply_encoding(FileEncoding::Iso8859_1, FileEncodingOperation::Reopen);

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

        session.apply_encoding(FileEncoding::Utf16Le, FileEncodingOperation::Reopen);
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
