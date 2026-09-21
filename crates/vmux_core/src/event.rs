pub use vmux_wire::space;
pub use vmux_wire::team;
pub use vmux_wire::{
    CursorShape, FLAG_BOLD, FLAG_DIM, FLAG_INVERSE, FLAG_ITALIC, FLAG_STRIKETHROUGH,
    FLAG_UNDERLINE, LinkRange, TermColor, TermCursor, TermLine, TermSelectionRange, TermSpan,
    command_bar::CommandBarPicker,
};

mod editor;
mod explorer;
mod extension;
mod lsp;
mod page;
mod terminal;

pub use editor::*;
pub use explorer::*;
pub use extension::*;
pub use lsp::*;
pub use page::*;
pub use terminal::*;

#[cfg(test)]
mod file_event_tests {
    use super::*;

    #[test]
    fn explorer_tree_event_rkyv_roundtrip() {
        let e = ExplorerTreeEvent {
            root_name: "VMUX".into(),
            root_path: "/r".into(),
            current_path: "/r/src/lib.rs".into(),
            focus_path: "/r/src/lib.rs".into(),
            loading: false,
            rows: vec![TreeRow {
                name: "src".into(),
                path: "/r/src".into(),
                depth: 0,
                is_dir: true,
                expanded: true,
                loading: false,
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&e).expect("ser");
        let back = rkyv::from_bytes::<ExplorerTreeEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(e, back);
    }

    #[test]
    fn explorer_outline_and_open_editors_roundtrip() {
        let o = OutlineEvent {
            items: vec![OutlineRow {
                name: "## Install".into(),
                kind: 15,
                line: 12,
                end_line: 40,
                depth: 0,
            }],
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&o).unwrap();
        assert_eq!(
            rkyv::from_bytes::<OutlineEvent, rkyv::rancor::Error>(&b).unwrap(),
            o
        );
        let oe = OpenEditorsEvent {
            items: vec![OpenEditorItem {
                name: "lib.rs".into(),
                path: "/r/src/lib.rs".into(),
                active: true,
                dirty: false,
                is_dir: false,
            }],
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&oe).unwrap();
        assert_eq!(
            rkyv::from_bytes::<OpenEditorsEvent, rkyv::rancor::Error>(&b).unwrap(),
            oe
        );
    }

    #[test]
    fn file_viewport_patch_rkyv_roundtrip() {
        let patch = FileViewportPatch {
            sticky: Vec::new(),
            first_row: 100,
            total_rows: 4000,
            total_lines: 5000,
            wrap_columns: 80,
            layouts: vec![FileLineLayout {
                line_no: 100,
                row: 100,
                rows: 1,
            }],
            lines: vec![FileLine {
                line_no: 100,
                fold: FoldGutter::None,
                spans: vec![StyledSpan {
                    text: "fn main() {".into(),
                    fg: [220, 220, 170],
                    bold: false,
                    italic: false,
                }],
                indent_levels: 0,
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&patch).expect("ser");
        let decoded =
            rkyv::from_bytes::<FileViewportPatch, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded.first_row, 100);
        assert_eq!(decoded.total_rows, 4000);
        assert_eq!(decoded.total_lines, 5000);
        assert_eq!(decoded.wrap_columns, 80);
        assert_eq!(decoded.layouts, patch.layouts);
        assert_eq!(decoded.lines[0].line_no, 100);
        assert_eq!(decoded.lines[0].spans[0].text, "fn main() {");
        assert_eq!(decoded.lines[0].spans[0].fg, [220, 220, 170]);
    }

    #[test]
    fn file_scroll_and_resize_roundtrip() {
        let s = FileScrollEvent {
            top_row: 42,
            needs_rows: true,
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&s).unwrap();
        assert_eq!(
            rkyv::from_bytes::<FileScrollEvent, rkyv::rancor::Error>(&b).unwrap(),
            s
        );
        let r = FileResizeEvent {
            char_height: 16.0,
            viewport_height: 480.0,
            wrap_columns: 120,
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let d = rkyv::from_bytes::<FileResizeEvent, rkyv::rancor::Error>(&b).unwrap();
        assert_eq!(d.char_height, 16.0);
        assert_eq!(d.viewport_height, 480.0);
        assert_eq!(d.wrap_columns, 120);
    }

    #[test]
    fn preview_kind_rkyv_roundtrip() {
        let k = PreviewKind::Image {
            mime: "image/png".into(),
            bytes: vec![1, 2, 3],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&k).unwrap();
        let back = rkyv::from_bytes::<PreviewKind, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(k, back);
    }

    #[test]
    fn file_dir_event_has_parent_fields() {
        let e = FileDirEvent {
            path: "/a/b".into(),
            abs_path: "/a/b".into(),
            entries: vec![],
            parent_path: "/a".into(),
            parent_entries: vec![],
        };
        assert_eq!(e.parent_path, "/a");
    }

    #[test]
    fn file_diagnostics_event_rkyv_roundtrip() {
        let ev = FileDiagnosticsEvent {
            path: "/src/main.rs".into(),
            diagnostics: vec![FileDiagnostic {
                line: 3,
                start_col: 4,
                end_col: 9,
                severity: DiagSeverity::Error,
                message: "cannot find value `x`".into(),
                source: Some("rustc".into()),
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&ev).expect("ser");
        let back =
            rkyv::from_bytes::<FileDiagnosticsEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(back.path, "/src/main.rs");
        assert_eq!(back.diagnostics.len(), 1);
        assert_eq!(back.diagnostics[0].line, 3);
        assert_eq!(back.diagnostics[0].end_col, 9);
        assert_eq!(back.diagnostics[0].severity, DiagSeverity::Error);
        assert_eq!(back.diagnostics[0].source.as_deref(), Some("rustc"));
    }

    #[test]
    fn lsp_catalog_event_rkyv_roundtrip() {
        let ev = LspCatalogEvent {
            packages: vec![LspPackage {
                name: "rust-analyzer".into(),
                description: "Rust LSP".into(),
                languages: vec!["rust".into()],
                categories: vec!["LSP".into()],
                status: LspPkgStatus::Available,
                version: None,
                installable: true,
                requires: None,
            }],
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&ev).unwrap();
        let d = rkyv::from_bytes::<LspCatalogEvent, rkyv::rancor::Error>(&b).unwrap();
        assert_eq!(d.packages[0].name, "rust-analyzer");
        assert_eq!(d.packages[0].status, LspPkgStatus::Available);
        assert!(d.packages[0].installable);
    }

    #[test]
    fn lsp_status_event_rkyv_roundtrip() {
        let ev = FileLspStatusEvent {
            path: "/x.rs".into(),
            server: "rust-analyzer".into(),
            package: Some("rust-analyzer".into()),
            state: LspServerState::Ready,
            actions: vec![EditorAction::Rename, EditorAction::FormatDocument],
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&ev).unwrap();
        let d = rkyv::from_bytes::<FileLspStatusEvent, rkyv::rancor::Error>(&b).unwrap();
        assert_eq!(d.path, "/x.rs");
        assert_eq!(d.server, "rust-analyzer");
        assert_eq!(d.package.as_deref(), Some("rust-analyzer"));
        assert_eq!(d.state, LspServerState::Ready);
        assert_eq!(
            d.actions,
            vec![EditorAction::Rename, EditorAction::FormatDocument]
        );
    }

    #[test]
    fn lsp_install_progress_rkyv_roundtrip() {
        let p = LspInstallProgress {
            name: "gopls".into(),
            phase: InstallPhase::Downloading,
            pct: Some(42),
            message: "downloading".into(),
        };
        let b = rkyv::to_bytes::<rkyv::rancor::Error>(&p).unwrap();
        let d = rkyv::from_bytes::<LspInstallProgress, rkyv::rancor::Error>(&b).unwrap();
        assert_eq!(d.name, "gopls");
        assert_eq!(d.phase, InstallPhase::Downloading);
        assert_eq!(d.pct, Some(42));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_cursor_event_roundtrips() {
        use crate::editor::{CursorPos, EditMode, SelSpan};
        let e = FileCursorEvent {
            search_total: 4,
            search_index: 2,
            mode: EditMode::Insert,
            mode_label: "INSERT".into(),
            primary: CursorPos {
                line: 3,
                row: 3,
                col: 5,
                char_col: 4,
            },
            carets: vec![
                CursorPos {
                    line: 3,
                    row: 3,
                    col: 5,
                    char_col: 4,
                },
                CursorPos {
                    line: 4,
                    row: 4,
                    col: 5,
                    char_col: 4,
                },
            ],
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
                char_col: 20,
            },
            source_selections: vec![SelSpan {
                line: 3,
                row: 3,
                start: 20,
                end: 25,
            }],
            search: vec![SelSpan {
                line: 1,
                row: 1,
                start: 2,
                end: 6,
            }],
            word_highlights: vec![SelSpan {
                line: 3,
                row: 3,
                start: 0,
                end: 5,
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
}
