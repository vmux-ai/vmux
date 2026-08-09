use super::*;

#[test]
fn file_cursor_event_roundtrips() {
    use crate::editor::{CursorPos, EditMode, SelSpan};
    let e = FileCursorEvent {
        mode: EditMode::Insert,
        mode_label: "INSERT".into(),
        primary: CursorPos {
            line: 3,
            row: 3,
            col: 5,
        },
        selections: vec![SelSpan {
            line: 3,
            row: 3,
            start: 0,
            end: 5,
        }],
        source_primary: CursorPos {
            line: 3,
            row: 3,
            col: 25,
        },
        source_selections: vec![SelSpan {
            line: 3,
            row: 3,
            start: 20,
            end: 25,
        }],
        command_line: ":wq".into(),
        search: vec![SelSpan {
            line: 1,
            row: 1,
            start: 2,
            end: 6,
        }],
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&e).unwrap();
    let back = rkyv::from_bytes::<FileCursorEvent, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(back, e);
}

#[test]
fn file_view_mode_event_roundtrips() {
    let event = FileViewModeEvent {
        mode: FileViewMode::Diff,
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
    let back = rkyv::from_bytes::<FileViewModeEvent, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(back, event);
}

#[test]
fn file_note_event_roundtrips() {
    let event = FileNoteEvent {
        title: "Title".into(),
        properties: vec![crate::knowledge::KnowledgeProperty {
            key: "tags".into(),
            kind: crate::knowledge::KnowledgePropertyKind::Tags,
            values: vec!["test".into()],
        }],
        blocks: vec![NoteBlock {
            start_line: 0,
            end_line: 1,
            source: "# Title".into(),
            block: MdBlock::Heading {
                level: 1,
                inlines: vec![MdInline::Text("Title".into())],
            },
        }],
        active: Some(0),
        references: Vec::new(),
        reveal_line: None,
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
    let back = rkyv::from_bytes::<FileNoteEvent, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(back, event);
}

fn patch(changed_rows: Vec<u32>, cols: u16, rows: u16, full: bool) -> TermViewportPatch {
    TermViewportPatch {
        changed_lines: changed_rows
            .into_iter()
            .map(|row| (row, TermLine::default()))
            .collect(),
        cursor: TermCursor::default(),
        cols,
        rows,
        selection: None,
        copy_mode: false,
        full,
        first_row: 0,
        total_rows: rows as u32,
        alt: false,
        mouse: false,
        evicted_total: 0,
    }
}

#[test]
fn viewport_patch_rebuilds_only_for_full_or_dimension_change() {
    assert!(!patch(vec![3], 80, 24, false).requires_row_rebuild(80, 24));
    assert!(patch(vec![3], 80, 24, true).requires_row_rebuild(80, 24));
    assert!(patch(vec![3], 100, 24, false).requires_row_rebuild(80, 24));
    assert!(patch(vec![3], 80, 30, false).requires_row_rebuild(80, 24));
}

#[test]
fn viewport_patch_changed_rows_come_only_from_changed_lines() {
    let rows = patch(vec![1, 9], 80, 24, false)
        .changed_row_indices()
        .collect::<Vec<_>>();
    assert_eq!(rows, vec![1, 9]);
}

#[test]
fn cursor_row_update_targets_only_old_and_new_visible_rows() {
    let old = TermCursor {
        row: 2,
        visible: true,
        ..TermCursor::default()
    };
    let new = TermCursor {
        row: 5,
        visible: true,
        ..TermCursor::default()
    };

    assert_eq!(
        cursor_row_update(Some(&old), &new),
        CursorRowUpdate {
            clear: Some(2),
            set: Some(5)
        }
    );
    assert_eq!(
        cursor_row_update(Some(&new), &new),
        CursorRowUpdate {
            clear: None,
            set: Some(5)
        }
    );
    assert_eq!(
        cursor_row_update(
            Some(&old),
            &TermCursor {
                visible: false,
                ..new
            }
        ),
        CursorRowUpdate {
            clear: Some(2),
            set: None
        }
    );
}

#[test]
fn term_title_event_rkyv_roundtrip() {
    let original = TermTitleEvent {
        title: "hello-osc".to_string(),
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
    let recovered =
        rkyv::from_bytes::<TermTitleEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
    assert_eq!(original.title, recovered.title);
}

#[test]
fn term_loading_event_rkyv_roundtrip() {
    let original = TermLoadingEvent {
        loading: true,
        label: "Vibe".to_string(),
        segment: "vibe".to_string(),
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
    let recovered =
        rkyv::from_bytes::<TermLoadingEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
    assert_eq!(original, recovered);
}

#[test]
fn agent_prompt_draft_event_rkyv_roundtrip() {
    let original = AgentPromptDraftEvent {
        draft: "find me a hotel".to_string(),
        skipped: false,
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
    let recovered = rkyv::from_bytes::<AgentPromptDraftEvent, rkyv::rancor::Error>(&bytes)
        .expect("deserialize");
    assert_eq!(original, recovered);
}
