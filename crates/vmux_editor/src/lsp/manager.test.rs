use super::*;
use vmux_core::event::StyledSpan;

fn fline(no: u32, text: &str) -> FileLine {
    FileLine {
        line_no: no,
        fold: vmux_core::event::FoldGutter::None,
        spans: vec![StyledSpan {
            text: text.into(),
            fg: [0, 0, 0],
            bold: false,
            italic: false,
        }],
    }
}

fn diag(l0: u32, c0: u32, l1: u32, c1: u32, sev: i32, msg: &str) -> lsp_types::Diagnostic {
    let severity = match sev {
        1 => lsp_types::DiagnosticSeverity::ERROR,
        2 => lsp_types::DiagnosticSeverity::WARNING,
        3 => lsp_types::DiagnosticSeverity::INFORMATION,
        _ => lsp_types::DiagnosticSeverity::HINT,
    };
    lsp_types::Diagnostic {
        range: lsp_types::Range {
            start: lsp_types::Position {
                line: l0,
                character: c0,
            },
            end: lsp_types::Position {
                line: l1,
                character: c1,
            },
        },
        severity: Some(severity),
        message: msg.into(),
        source: Some("rustc".into()),
        ..Default::default()
    }
}

#[test]
fn ascii_columns_pass_through() {
    let lines = vec![fline(0, "let x = 1;")];
    let out = to_file_diagnostics(&lines, &[diag(0, 4, 0, 5, 1, "unused")]);
    assert_eq!(out[0].start_col, 4);
    assert_eq!(out[0].end_col, 5);
    assert_eq!(out[0].severity, DiagSeverity::Error);
}

#[test]
fn parses_folding_ranges() {
    let v = serde_json::json!([
        { "startLine": 0, "endLine": 3 },
        { "startLine": 1, "endLine": 1 },
    ]);
    let regs = parse_folding_ranges(&v);
    assert_eq!(regs, vec![crate::fold::FoldRegion { start: 0, end: 3 }]);
}

#[test]
fn utf16_emoji_maps_to_char_index() {
    let lines = vec![fline(0, "😀ab")];
    assert_eq!(utf16_to_char_col("😀ab", 2), 1);
    assert_eq!(utf16_to_char_col("😀ab", 3), 2);
    let out = to_file_diagnostics(&lines, &[diag(0, 2, 0, 3, 2, "warn")]);
    assert_eq!(out[0].start_col, 1);
    assert_eq!(out[0].end_col, 2);
    assert_eq!(out[0].severity, DiagSeverity::Warning);
}

#[test]
fn out_of_range_columns_clamp() {
    let lines = vec![fline(0, "ab")];
    let out = to_file_diagnostics(&lines, &[diag(0, 99, 0, 99, 1, "x")]);
    assert_eq!(out[0].start_col, 2);
    assert_eq!(out[0].end_col, 2);
}

#[test]
fn multiline_range_underlines_first_line_to_eol() {
    let lines = vec![fline(0, "abcdef"), fline(1, "ghi")];
    let out = to_file_diagnostics(&lines, &[diag(0, 2, 1, 1, 1, "multi")]);
    assert_eq!(out[0].line, 0);
    assert_eq!(out[0].start_col, 2);
    assert_eq!(out[0].end_col, 6);
}

#[test]
fn drain_empties_outbox() {
    use crate::lsp::LspOutbox;
    use std::path::PathBuf;

    let mut app = App::new();
    let outbox = LspOutbox::default();
    app.add_plugins(MinimalPlugins)
        .insert_resource(outbox.clone());
    outbox
        .0
        .lock()
        .unwrap()
        .push((PathBuf::from("/x.rs"), vec![]));
    app.add_systems(Update, |ob: Res<LspOutbox>| {
        ob.0.lock().unwrap().drain(..).for_each(drop);
    });
    app.update();
    assert!(outbox.0.lock().unwrap().is_empty());
}

#[test]
fn char_utf16_roundtrip_surrogate_pair() {
    let text = "a😀b";
    assert_eq!(char_to_utf16_col(text, 0), 0);
    assert_eq!(char_to_utf16_col(text, 1), 1);
    assert_eq!(char_to_utf16_col(text, 2), 3);
    assert_eq!(char_to_utf16_col(text, 3), 4);
    assert_eq!(utf16_to_char_col(text, 3), 2);
}

#[test]
fn diagnostics_map_through_editstate() {
    use crate::edit::highlight_cache::HighlightCache;
    use crate::edit::{EditCore, EditMode};
    use crate::lsp::LspOutbox;
    use crate::plugin::{EditState, FileView};
    use std::path::PathBuf;

    let path = PathBuf::from("/tmp/vmux_lsp_editstate.rs");
    let mut app = App::new();
    let outbox = LspOutbox::default();
    app.add_plugins(MinimalPlugins)
        .init_resource::<DiagState>()
        .insert_resource(outbox.clone())
        .add_systems(Update, drain_lsp_diagnostics);

    let core = EditCore::new(
        path.clone(),
        "Rust".into(),
        "fn a() {}\nlet x = 1;\n",
        EditMode::Insert,
    );
    let hl = HighlightCache::new(&path);
    app.world_mut().spawn((
        FileView { path: path.clone() },
        EditState::new(core, hl, crate::fold::FoldState::default()),
    ));

    let diag = lsp_types::Diagnostic {
        range: lsp_types::Range {
            start: lsp_types::Position {
                line: 1,
                character: 4,
            },
            end: lsp_types::Position {
                line: 1,
                character: 5,
            },
        },
        message: "boom".into(),
        ..Default::default()
    };
    outbox.0.lock().unwrap().push((path.clone(), vec![diag]));
    app.update();

    let state = app.world().resource::<DiagState>();
    let mapped = state
        .lsp
        .get(&canon(&path))
        .expect("diagnostics mapped for EditState entity");
    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped[0].line, 1);
    assert_eq!(mapped[0].start_col, 4);
}
