use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

use bevy::prelude::*;

use super::buffer::TextBuffer;
use super::command::{EditMode, Selection};
use super::core::{CaretMemory, Group};
use super::register::Registers;
use super::search::Search;
use super::undo::History;

#[derive(Bundle)]
pub struct Document {
    pub buffer: TextBuffer,
    pub carets: Carets,
    pub mode: Mode,
    pub viewport: Viewport,
    pub revision: Revision,
    pub history: History,
    pub transaction: Transaction,
    pub search: SearchState,
    pub navigation: Navigation,
    pub registers: Registers,
    pub input: crate::keymap::vim::VimKeymap,
}

impl Document {
    pub fn new(path: PathBuf, language: String, text: &str, mode: EditMode) -> Self {
        let buffer = TextBuffer::from_text(path, language, text);
        let fold_view = crate::fold::FoldState::default().view(buffer.len_lines() as u32);
        let tree = History::new(buffer.rope.clone(), vec![Selection::caret(0)], 0);
        Self {
            buffer,
            carets: Carets::default(),
            mode: Mode { value: mode },
            viewport: Viewport {
                rows: 0,
                top_row: 0,
                fold_view,
            },
            revision: Revision {
                identity: uuid::Uuid::new_v4(),
                content: uuid::Uuid::new_v4(),
                current: 0,
                next: 1,
                saved: Some(0),
                dirty: false,
            },
            history: tree,
            transaction: Transaction {
                last_group: None,
                in_sweep: false,
                sweep_snapshotted: false,
            },
            search: SearchState::default(),
            navigation: Navigation::default(),
            registers: Registers::default(),
            input: crate::keymap::vim::VimKeymap::default(),
        }
    }
}

#[derive(Component)]
pub struct Carets {
    pub selections: Vec<Selection>,
    pub(crate) active: usize,
    pub(super) memory: Vec<CaretMemory>,
    pub(super) block_insert: Option<(Vec<usize>, usize)>,
}

impl Default for Carets {
    fn default() -> Self {
        Self {
            selections: vec![Selection::caret(0)],
            active: 0,
            memory: vec![CaretMemory::default()],
            block_insert: None,
        }
    }
}

impl Carets {
    pub fn place(&mut self, at: usize) {
        self.selections = vec![Selection::caret(at)];
        self.active = 0;
        self.memory = vec![CaretMemory::default()];
    }
}

#[derive(Component)]
pub struct Mode {
    pub value: EditMode,
}

#[derive(Component)]
pub struct Viewport {
    pub rows: u16,
    pub top_row: u32,
    pub fold_view: crate::fold::FoldView,
}

impl Viewport {
    pub(crate) fn sync(&mut self, viewport: &crate::host::viewport::FileViewport) {
        self.rows = viewport.rows;
        self.top_row = viewport.top_row;
    }
}

#[derive(Component)]
pub struct Revision {
    pub identity: uuid::Uuid,
    pub content: uuid::Uuid,
    pub current: u64,
    next: u64,
    pub saved: Option<u64>,
    pub dirty: bool,
}

impl Revision {
    pub(super) fn change(&mut self) {
        self.content = uuid::Uuid::new_v4();
        self.current = self.next;
        self.next = self.next.wrapping_add(1).max(1);
        self.dirty = self.saved != Some(self.current);
    }
}

#[derive(Component)]
pub struct Transaction {
    pub(super) last_group: Option<Group>,
    pub(crate) in_sweep: bool,
    pub(crate) sweep_snapshotted: bool,
}

#[derive(Component, Default)]
pub struct SearchState {
    pub query: Option<Search>,
    pub highlight: bool,
    pub(super) cache: Option<(u64, String, Vec<Range<usize>>)>,
}

#[derive(Component, Default)]
pub struct Navigation {
    pub(super) marks: HashMap<char, usize>,
    pub(super) jumps: Vec<usize>,
    pub(super) jump_index: usize,
    pub(super) changes: Vec<usize>,
    pub(super) change_index: usize,
}
