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
        }],
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&patch).expect("ser");
    let decoded = rkyv::from_bytes::<FileViewportPatch, rkyv::rancor::Error>(&bytes).expect("de");
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
    let s = FileScrollEvent { top_row: 42 };
    let b = rkyv::to_bytes::<rkyv::rancor::Error>(&s).unwrap();
    assert_eq!(
        rkyv::from_bytes::<FileScrollEvent, rkyv::rancor::Error>(&b)
            .unwrap()
            .top_row,
        42
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
    let back = rkyv::from_bytes::<FileDiagnosticsEvent, rkyv::rancor::Error>(&bytes).expect("de");
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
    };
    let b = rkyv::to_bytes::<rkyv::rancor::Error>(&ev).unwrap();
    let d = rkyv::from_bytes::<FileLspStatusEvent, rkyv::rancor::Error>(&b).unwrap();
    assert_eq!(d.path, "/x.rs");
    assert_eq!(d.server, "rust-analyzer");
    assert_eq!(d.package.as_deref(), Some("rust-analyzer"));
    assert_eq!(d.state, LspServerState::Ready);
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
