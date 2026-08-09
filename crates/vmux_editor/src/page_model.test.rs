use super::*;

fn append_live_text(source: &[char], nodes: &[NoteInlineNode], caret: u32, output: &mut String) {
    for node in nodes {
        match node {
            NoteInlineNode::Text { start, end } => {
                output.extend(
                    source[*start as usize..*end as usize]
                        .iter()
                        .map(
                            |character| {
                                if *character == '\n' { ' ' } else { *character }
                            },
                        ),
                );
            }
            NoteInlineNode::Syntax {
                start,
                prefix_end,
                suffix_start,
                end,
                children,
                ..
            } => {
                let reveal = *start <= caret && caret <= *end;
                if reveal {
                    output.extend(source[*start as usize..*prefix_end as usize].iter());
                }
                append_live_text(source, children, caret, output);
                if reveal {
                    output.extend(source[*suffix_start as usize..*end as usize].iter());
                }
            }
        }
    }
}

fn live_text(source: &str, caret: u32) -> String {
    let chars = source.chars().collect::<Vec<_>>();
    let nodes = note_inline_nodes(source, None);
    let mut output = String::new();
    append_live_text(&chars, &nodes, caret, &mut output);
    output
}

#[test]
fn gutter_width_min_three() {
    assert_eq!(gutter_width(0), 3);
    assert_eq!(gutter_width(9), 3);
    assert_eq!(gutter_width(1000), 4);
    assert_eq!(gutter_width(99999), 5);
}

#[test]
fn span_style_emits_color_and_styles() {
    let s = span_style(&StyledSpan {
        text: "x".into(),
        fg: [10, 20, 30],
        bold: true,
        italic: true,
    });
    assert!(s.contains("color:rgb(10,20,30)"));
    assert!(s.contains("font-weight:700"));
    assert!(s.contains("font-style:italic"));
}

#[test]
fn line_severity_takes_most_severe() {
    let mk = |line, sev| FileDiagnostic {
        line,
        start_col: 0,
        end_col: 1,
        severity: sev,
        message: String::new(),
        source: None,
    };
    let v = vec![mk(3, DiagSeverity::Warning), mk(3, DiagSeverity::Error)];
    assert_eq!(line_severity(&v, 3), Some(DiagSeverity::Error));
    assert_eq!(line_severity(&v, 4), None);
}

#[test]
fn squiggle_style_positions_by_columns() {
    let s = squiggle_style(2, 6, "rgb(255,0,0)");
    assert!(s.contains("left:calc(var(--cw,1ch) * 2)"));
    assert!(s.contains("width:calc(var(--cw,1ch) * 4)"));
}

#[test]
fn pkg_action_by_status() {
    assert_eq!(
        pkg_action(LspPkgStatus::Available, true),
        PkgAction::Install
    );
    assert_eq!(pkg_action(LspPkgStatus::Available, false), PkgAction::None);
    assert_eq!(
        pkg_action(LspPkgStatus::Installed, true),
        PkgAction::Uninstall
    );
    assert_eq!(pkg_action(LspPkgStatus::Outdated, true), PkgAction::Update);
    assert_eq!(pkg_action(LspPkgStatus::Installing, true), PkgAction::None);
    assert_eq!(pkg_action(LspPkgStatus::OnPath, true), PkgAction::None);
}

#[test]
fn pkg_status_label_covers_states() {
    assert_eq!(pkg_status_label(LspPkgStatus::OnPath), "On PATH");
    assert_eq!(pkg_status_label(LspPkgStatus::Installed), "Installed");
    assert_eq!(pkg_status_label(LspPkgStatus::Available), "Available");
}

#[test]
fn rapid_explorer_toggle_ignores_stale_echoes() {
    assert!(!should_apply_explorer_chrome(7, 3, 7, 1));
    assert!(!should_apply_explorer_chrome(7, 3, 7, 2));
    assert!(should_apply_explorer_chrome(7, 3, 7, 3));
    assert!(should_apply_explorer_chrome(7, 3, 9, 1));
}

#[test]
fn cursor_centering_places_target_at_viewport_midpoint() {
    assert_eq!(centered_scroll_top(500.0, 400.0), 300.0);
    assert_eq!(centered_scroll_top(100.0, 400.0), 0.0);
}

#[test]
fn cursor_reveal_waits_until_the_caret_leaves_the_viewport() {
    assert_eq!(viewport_reveal_delta(120.0, 148.0, 100.0, 500.0), 0.0);
    assert_eq!(viewport_reveal_delta(80.0, 108.0, 100.0, 500.0), -20.0);
    assert_eq!(viewport_reveal_delta(480.0, 520.0, 100.0, 500.0), 20.0);
}

#[test]
fn note_caret_visibility_coalesces_to_latest_cursor_per_frame() {
    let mut queue = NoteCaretVisibilityQueue::default();
    let first = NoteCaretVisibilityRequest {
        block_index: 2,
        line: 8,
        retry: true,
    };
    let latest = NoteCaretVisibilityRequest {
        block_index: 4,
        line: 15,
        retry: true,
    };

    assert!(queue.enqueue(first));
    assert!(!queue.enqueue(latest));
    assert_eq!(queue.take(), Some(latest));
    assert!(queue.enqueue(first));
}

#[test]
fn editor_drag_requires_deliberate_pointer_movement() {
    assert!(!editor_drag_started((100, 100), (103, 102)));
    assert!(editor_drag_started((100, 100), (104, 100)));
    assert!(editor_drag_started((100, 100), (96, 96)));
}

#[test]
fn note_cursor_restore_preserves_viewport_until_explicit_reveal() {
    assert_eq!(
        note_cursor_activation(Some(12), true, 8),
        Some(NoteCursorActivation::Center(12))
    );
    assert_eq!(
        note_cursor_activation(None, true, 8),
        Some(NoteCursorActivation::PreserveViewport(8))
    );
    assert_eq!(note_cursor_activation(None, false, 8), None);
}

#[test]
fn note_list_prefix_excludes_marker_and_task_checkbox() {
    assert_eq!(note_list_marker_prefix_len("- item"), Some((0, 2)));
    assert_eq!(note_list_marker_prefix_len("  12. item"), Some((2, 6)));
    assert_eq!(note_list_marker_prefix_len("- [ ] task"), Some((0, 6)));
    assert_eq!(note_list_marker_prefix_len("  * [x] done"), Some((2, 8)));
    assert_eq!(note_list_marker_prefix_len("paragraph"), None);
}

#[test]
fn note_live_preview_preserves_paragraph_flow() {
    let source = "first line\nsecond line\nthird line";
    assert_eq!(live_text(source, 4), "first line second line third line");
    assert_eq!(note_source_offset(source, 5, 7, 3), 26);
    assert_eq!(note_source_position(source, 5, 26), (7, 3));
}

#[test]
fn note_live_preview_reveals_only_active_inline_syntax() {
    let source = "plain `code` and **bold** with [link](https://vmux.ai)";
    assert_eq!(live_text(source, 2), "plain code and bold with link");
    assert_eq!(live_text(source, 8), "plain `code` and bold with link");
    assert_eq!(live_text(source, 20), "plain code and **bold** with link");
    assert_eq!(
        live_text(source, 35),
        "plain code and bold with [link](https://vmux.ai)"
    );
}

#[test]
fn note_live_preview_uses_wiki_link_label() {
    let source = "See [[projects/vmux|vmux project]] now";
    assert_eq!(live_text(source, 1), "See vmux project now");
    assert_eq!(
        live_text(source, 10),
        "See [[projects/vmux|vmux project]] now"
    );
}

#[test]
fn tree_motion_merge_is_linear_ordered_and_marks_entries() {
    let row = |path: &str| TreeRow {
        name: path.to_string(),
        path: path.to_string(),
        depth: 0,
        is_dir: false,
        expanded: false,
        loading: false,
    };
    let current = vec![row("a"), row("b"), row("c"), row("d")];
    let next = vec![row("a"), row("x"), row("d")];
    let merged = merge_tree_motion_rows(&current, &next);
    assert_eq!(
        merged
            .iter()
            .map(|(row, visible)| (row.path.as_str(), *visible))
            .collect::<Vec<_>>(),
        vec![
            ("a", true),
            ("b", false),
            ("c", false),
            ("x", false),
            ("d", true)
        ]
    );
}
