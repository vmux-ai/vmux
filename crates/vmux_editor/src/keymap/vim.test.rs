use super::*;
use crate::keymap::Mods;

fn k(key: &str) -> KeyInput {
    KeyInput {
        key: key.into(),
        mods: Mods::default(),
        repeat: false,
    }
}
fn chord(key: &str, mods: Mods) -> KeyInput {
    KeyInput {
        key: key.into(),
        mods,
        repeat: false,
    }
}
fn ctrl() -> Mods {
    Mods {
        ctrl: true,
        ..Default::default()
    }
}
fn run(km: &mut VimKeymap, seq: &[&str]) -> Vec<EditCommand> {
    let mut out = Vec::new();
    for s in seq {
        out.extend(km.handle(&k(s)));
    }
    out
}
fn op(operator: Operator, target: Target) -> EditCommand {
    EditCommand::Op {
        operator,
        target,
        register: None,
    }
}

#[test]
fn operators_pair_with_motions_and_doubled_keys() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["d", "w"]),
        vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 1))]
    );
    assert_eq!(
        run(&mut km, &["d", "d"]),
        vec![op(Operator::Delete, Target::Line(1))]
    );
    assert_eq!(
        run(&mut km, &["y", "y"]),
        vec![op(Operator::Yank, Target::Line(1))]
    );
    assert_eq!(
        run(&mut km, &["c", "c"]),
        vec![op(Operator::Change, Target::Line(1))]
    );
    assert_eq!(km.mode(), EditMode::Insert);
}

#[test]
fn counts_multiply_across_operator_and_motion() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["d", "3", "w"]),
        vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 3))]
    );
    assert_eq!(
        run(&mut km, &["2", "d", "3", "w"]),
        vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 6))]
    );
    assert_eq!(
        run(&mut km, &["2", "d", "d"]),
        vec![op(Operator::Delete, Target::Line(2))]
    );
}

fn object(kind: TextObjectKind, around: bool, count: usize) -> Target {
    Target::TextObject(TextObject {
        kind,
        around,
        count,
    })
}

#[test]
fn operators_take_text_objects() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["d", "i", "w"]),
        vec![op(Operator::Delete, object(TextObjectKind::Word, false, 1))]
    );
    assert_eq!(
        run(&mut km, &["c", "a", "\""]),
        vec![op(
            Operator::Change,
            object(TextObjectKind::DoubleQuote, true, 1)
        )]
    );
    assert_eq!(km.mode(), EditMode::Insert);
}

#[test]
fn text_object_counts_come_from_the_operator() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["2", "d", "i", "("]),
        vec![op(
            Operator::Delete,
            object(TextObjectKind::Paren, false, 2)
        )]
    );
}

#[test]
fn visual_mode_selects_text_objects() {
    let mut km = VimKeymap::default();
    run(&mut km, &["v"]);
    assert_eq!(
        run(&mut km, &["i", "p"]),
        vec![EditCommand::SelectTextObject(TextObject {
            kind: TextObjectKind::Paragraph,
            around: false,
            count: 1,
        })]
    );
}

#[test]
fn an_unknown_object_key_abandons_the_operator() {
    let mut km = VimKeymap::default();
    assert_eq!(run(&mut km, &["d", "i", "z"]), vec![]);
    assert_eq!(
        run(&mut km, &["d", "w"]),
        vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 1))]
    );
}

fn find(ch: char, forward: bool, till: bool) -> Motion {
    Motion::FindChar { ch, forward, till }
}

#[test]
fn find_char_prefixes_consume_the_next_key() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["f", ","]),
        vec![EditCommand::Move(find(',', true, false))]
    );
    assert_eq!(
        run(&mut km, &["T", "x"]),
        vec![EditCommand::Move(find('x', false, true))]
    );
}

#[test]
fn semicolon_repeats_and_comma_reverses_the_last_find() {
    let mut km = VimKeymap::default();
    run(&mut km, &["f", "x"]);
    assert_eq!(
        run(&mut km, &[";"]),
        vec![EditCommand::Move(find('x', true, false))]
    );
    assert_eq!(
        run(&mut km, &[","]),
        vec![EditCommand::Move(find('x', false, false))]
    );
}

#[test]
fn repeat_find_without_a_previous_find_does_nothing() {
    let mut km = VimKeymap::default();
    assert_eq!(run(&mut km, &[";"]), vec![]);
}

#[test]
fn operators_compose_with_find_char() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["d", "t", ","]),
        vec![op(
            Operator::Delete,
            Target::Motion(find(',', true, true), 1)
        )]
    );
    assert_eq!(
        run(&mut km, &["2", "d", "f", "x"]),
        vec![op(
            Operator::Delete,
            Target::Motion(find('x', true, false), 2)
        )]
    );
}

#[test]
fn the_tag_object_is_not_shadowed_by_the_till_prefix() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["d", "i", "t"]),
        vec![op(Operator::Delete, object(TextObjectKind::Tag, false, 1))]
    );
}

#[test]
fn word_and_screen_motions_are_bound() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["W"]),
        vec![EditCommand::Move(Motion::BigWordNext)]
    );
    assert_eq!(
        run(&mut km, &["g", "e"]),
        vec![EditCommand::Move(Motion::WordEndPrev)]
    );
    assert_eq!(
        run(&mut km, &["%"]),
        vec![EditCommand::Move(Motion::MatchPair)]
    );
    assert_eq!(
        run(&mut km, &["H"]),
        vec![EditCommand::Move(Motion::ScreenTop)]
    );
    assert_eq!(
        run(&mut km, &["8", "|"]),
        vec![EditCommand::Move(Motion::Column(8))]
    );
}

/// `motion_for` maps bare `e`, `E` and `_`, so a visual-mode `g` prefix has to be consumed
/// before the motion lookup or it resolves the wrong motion and leaves `g_pending` set.
#[test]
fn visual_g_prefixed_motions_are_not_shadowed() {
    let mut km = VimKeymap::default();
    run(&mut km, &["v"]);
    assert_eq!(km.mode(), EditMode::Visual);

    assert_eq!(
        run(&mut km, &["g", "e"]),
        vec![EditCommand::Select(Motion::WordEndPrev)]
    );
    assert_eq!(
        run(&mut km, &["g", "E"]),
        vec![EditCommand::Select(Motion::BigWordEndPrev)]
    );
    assert_eq!(
        run(&mut km, &["g", "_"]),
        vec![EditCommand::Select(Motion::LastNonBlank)]
    );
    // The prefix must not leak into the next key.
    assert_eq!(
        run(&mut km, &["e"]),
        vec![EditCommand::Select(Motion::WordEnd)]
    );
}

#[test]
fn half_and_full_page_scroll_chords() {
    let mut km = VimKeymap::default();
    assert_eq!(
        km.handle(&chord("d", ctrl())),
        vec![EditCommand::Move(Motion::HalfPageDown)]
    );
    assert_eq!(
        km.handle(&chord("b", ctrl())),
        vec![EditCommand::Move(Motion::PageUp)]
    );
}

#[test]
fn z_prefix_serves_both_folds_and_scroll_placement() {
    let mut km = VimKeymap::default();
    assert_eq!(run(&mut km, &["z", "a"]), vec![EditCommand::FoldToggle]);
    assert_eq!(
        run(&mut km, &["z", "z"]),
        vec![EditCommand::ScrollCursorTo(ScrollPlacement::Center)]
    );
    assert_eq!(
        run(&mut km, &["z", "b"]),
        vec![EditCommand::ScrollCursorTo(ScrollPlacement::Bottom)]
    );
}

#[test]
fn dot_repeats_the_last_change_not_the_last_motion() {
    let mut km = VimKeymap::default();
    run(&mut km, &["d", "w"]);
    run(&mut km, &["j", "l"]);
    assert_eq!(
        run(&mut km, &["."]),
        vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 1))]
    );
}

#[test]
fn dot_takes_a_count_and_survives_reuse() {
    let mut km = VimKeymap::default();
    run(&mut km, &["x"]);
    let expected = op(Operator::Delete, Target::Motion(Motion::Right, 1));
    assert_eq!(
        run(&mut km, &["2", "."]),
        vec![expected.clone(), expected.clone()]
    );
    assert_eq!(run(&mut km, &["."]), vec![expected]);
}

#[test]
fn dot_repeats_an_insert_session_including_typed_text() {
    let mut km = VimKeymap::default();
    run(&mut km, &["i"]);
    km.record_text("hi");
    run(&mut km, &["Escape"]);
    assert_eq!(
        run(&mut km, &["."]),
        vec![
            EditCommand::SetMode(EditMode::Insert),
            EditCommand::InsertText("hi".into()),
            EditCommand::Move(Motion::LeftBounded),
            EditCommand::SetMode(EditMode::Normal),
        ]
    );
}

#[test]
fn a_yank_is_not_a_change() {
    let mut km = VimKeymap::default();
    run(&mut km, &["x"]);
    run(&mut km, &["y", "y"]);
    assert_eq!(
        run(&mut km, &["."]),
        vec![op(Operator::Delete, Target::Motion(Motion::Right, 1))]
    );
}

#[test]
fn macros_record_and_replay_through_a_register() {
    let mut km = VimKeymap::default();
    run(&mut km, &["q", "a", "x", "j", "q"]);
    assert_eq!(
        run(&mut km, &["@", "a"]),
        vec![
            op(Operator::Delete, Target::Motion(Motion::Right, 1)),
            EditCommand::Move(Motion::Down),
        ]
    );
    assert_eq!(
        run(&mut km, &["@", "@"]),
        vec![
            op(Operator::Delete, Target::Motion(Motion::Right, 1)),
            EditCommand::Move(Motion::Down),
        ]
    );
}

#[test]
fn a_counted_macro_replays_repeatedly() {
    let mut km = VimKeymap::default();
    run(&mut km, &["q", "b", "x", "q"]);
    let once = op(Operator::Delete, Target::Motion(Motion::Right, 1));
    assert_eq!(
        run(&mut km, &["3", "@", "b"]),
        vec![once.clone(), once.clone(), once]
    );
}

#[test]
fn replaying_an_empty_register_is_inert() {
    let mut km = VimKeymap::default();
    assert_eq!(run(&mut km, &["@", "z"]), vec![]);
}

#[test]
fn insert_mode_editing_chords() {
    let mut km = VimKeymap::default();
    run(&mut km, &["i"]);
    assert_eq!(
        km.handle(&chord("w", ctrl())),
        vec![EditCommand::DeleteWordBack]
    );
    assert_eq!(
        km.handle(&chord("u", ctrl())),
        vec![op(Operator::Delete, Target::Motion(Motion::LineStart, 1))]
    );
    assert_eq!(
        km.handle(&chord("t", ctrl())),
        vec![op(Operator::Indent, Target::Line(1))]
    );
    assert_eq!(km.mode(), EditMode::Insert);
}

#[test]
fn ctrl_r_pastes_a_register_while_inserting() {
    let mut km = VimKeymap::default();
    run(&mut km, &["i"]);
    assert_eq!(km.handle(&chord("r", ctrl())), vec![]);
    assert_eq!(
        run(&mut km, &["a"]),
        vec![EditCommand::Put {
            before: true,
            count: 1,
            register: Some('a'),
        }]
    );
}

fn mapped(specs: &[(&str, &str, &str)]) -> VimKeymap {
    let specs: Vec<_> = specs
        .iter()
        .map(|(mode, lhs, rhs)| vmux_core::editor::KeyMapping {
            mode: (*mode).into(),
            lhs: (*lhs).into(),
            rhs: (*rhs).into(),
        })
        .collect();
    VimKeymap::with_mappings(&specs, " ")
}

#[test]
fn jk_maps_to_escape_in_insert_mode() {
    let mut km = mapped(&[("i", "jk", "<Esc>")]);
    run(&mut km, &["i"]);
    assert_eq!(run(&mut km, &["j"]), vec![]);
    assert_eq!(
        run(&mut km, &["k"]),
        vec![
            EditCommand::Move(Motion::LeftBounded),
            EditCommand::SetMode(EditMode::Normal),
        ]
    );
    assert_eq!(km.mode(), EditMode::Normal);
}

#[test]
fn an_abandoned_mapping_prefix_replays_as_typed() {
    let mut km = mapped(&[("n", "gs", "0")]);
    assert_eq!(run(&mut km, &["g"]), vec![]);
    assert_eq!(
        run(&mut km, &["d"]),
        vec![EditCommand::GotoDefinition],
        "the buffered g and the new d still form gd"
    );
}

#[test]
fn leader_mappings_expand_through_the_configured_leader() {
    let mut km = mapped(&[("n", "<leader>s", ":w<CR>")]);
    assert_eq!(run(&mut km, &[" "]), vec![]);
    assert_eq!(run(&mut km, &["s"]), vec![EditCommand::Save]);
}

#[test]
fn an_unmapped_key_is_untouched_by_the_mapping_layer() {
    let mut km = mapped(&[("i", "jk", "<Esc>")]);
    assert_eq!(
        run(&mut km, &["x"]),
        vec![op(Operator::Delete, Target::Motion(Motion::Right, 1))]
    );
}

#[test]
fn ctrl_v_toggles_visual_block() {
    let mut km = VimKeymap::default();
    assert_eq!(
        km.handle(&chord("v", ctrl())),
        vec![EditCommand::SetMode(EditMode::VisualBlock)]
    );
    assert_eq!(km.mode(), EditMode::VisualBlock);
    assert_eq!(
        km.handle(&chord("v", ctrl())),
        vec![EditCommand::SetMode(EditMode::Normal)]
    );
}

#[test]
fn block_insert_starts_then_replicates_on_escape() {
    let mut km = VimKeymap::default();
    km.handle(&chord("v", ctrl()));
    assert_eq!(
        run(&mut km, &["I"]),
        vec![EditCommand::BeginBlockInsert { after: false }]
    );
    assert_eq!(km.mode(), EditMode::Insert);
    km.record_text("xy");
    assert_eq!(
        run(&mut km, &["Escape"]),
        vec![
            EditCommand::FinishBlockInsert { text: "xy".into() },
            EditCommand::Move(Motion::LeftBounded),
            EditCommand::SetMode(EditMode::Normal),
        ]
    );
}

#[test]
fn block_operators_route_through_the_selection() {
    let mut km = VimKeymap::default();
    km.handle(&chord("v", ctrl()));
    assert_eq!(
        run(&mut km, &["d"]),
        vec![
            op(Operator::Delete, Target::Selection),
            EditCommand::SetMode(EditMode::Normal)
        ]
    );
}

#[test]
fn ctrl_o_runs_one_normal_command_then_returns_to_insert() {
    let mut km = VimKeymap::default();
    run(&mut km, &["i"]);
    assert_eq!(
        km.handle(&chord("o", ctrl())),
        vec![EditCommand::SetMode(EditMode::Normal)]
    );
    assert_eq!(km.mode(), EditMode::Normal);
    assert_eq!(
        run(&mut km, &["$"]),
        vec![
            EditCommand::Move(Motion::LineEnd),
            EditCommand::SetMode(EditMode::Insert),
        ]
    );
    assert_eq!(km.mode(), EditMode::Insert);
}

#[test]
fn scroll_chords_do_not_fire_while_inserting() {
    let mut km = VimKeymap::default();
    run(&mut km, &["i"]);
    assert_eq!(km.handle(&chord("e", ctrl())), vec![]);
}

#[test]
fn search_repeat_and_word_search_bindings() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["n"]),
        vec![EditCommand::Move(Motion::SearchNext { reverse: false })]
    );
    assert_eq!(
        run(&mut km, &["N"]),
        vec![EditCommand::Move(Motion::SearchNext { reverse: true })]
    );
    assert_eq!(
        run(&mut km, &["*"]),
        vec![EditCommand::SearchWord { forward: true }]
    );
    assert_eq!(
        run(&mut km, &["#"]),
        vec![EditCommand::SearchWord { forward: false }]
    );
}

#[test]
fn an_operator_can_target_the_next_match() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["d", "n"]),
        vec![op(
            Operator::Delete,
            Target::Motion(Motion::SearchNext { reverse: false }, 1)
        )]
    );
}

#[test]
fn mark_bindings_set_and_recall() {
    let mut km = VimKeymap::default();
    assert_eq!(run(&mut km, &["m", "a"]), vec![EditCommand::SetMark('a')]);
    assert_eq!(
        run(&mut km, &["`", "a"]),
        vec![EditCommand::GotoMark {
            name: 'a',
            linewise: false
        }]
    );
    assert_eq!(
        run(&mut km, &["'", "a"]),
        vec![EditCommand::GotoMark {
            name: 'a',
            linewise: true
        }]
    );
}

#[test]
fn jump_and_change_list_bindings() {
    let mut km = VimKeymap::default();
    assert_eq!(
        km.handle(&chord("o", ctrl())),
        vec![EditCommand::JumpList {
            back: true,
            count: 1
        }]
    );
    assert_eq!(
        km.handle(&chord("i", ctrl())),
        vec![EditCommand::JumpList {
            back: false,
            count: 1
        }]
    );
    assert_eq!(
        run(&mut km, &["g", ";"]),
        vec![EditCommand::ChangeList {
            back: true,
            count: 1
        }]
    );
}

#[test]
fn a_mark_key_is_not_read_as_a_quote_object() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["d", "i", "'"]),
        vec![op(
            Operator::Delete,
            object(TextObjectKind::SingleQuote, false, 1)
        )]
    );
}

#[test]
fn count_repeats_motion() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["3", "j"]),
        vec![
            EditCommand::Move(Motion::Down),
            EditCommand::Move(Motion::Down),
            EditCommand::Move(Motion::Down)
        ]
    );
}

#[test]
fn register_prefix_routes_yank_and_put() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["\"", "a", "y", "y"]),
        vec![EditCommand::Op {
            operator: Operator::Yank,
            target: Target::Line(1),
            register: Some('a'),
        }]
    );
    assert_eq!(
        run(&mut km, &["\"", "a", "p"]),
        vec![EditCommand::Put {
            before: false,
            count: 1,
            register: Some('a'),
        }]
    );
}

#[test]
fn indent_operators_use_doubled_keys() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &[">", ">"]),
        vec![op(Operator::Indent, Target::Line(1))]
    );
    assert_eq!(
        run(&mut km, &["3", "<", "<"]),
        vec![op(Operator::Outdent, Target::Line(3))]
    );
}

#[test]
fn g_prefixed_case_operators() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["g", "u", "w"]),
        vec![op(Operator::Lower, Target::Motion(Motion::WordNext, 1))]
    );
    assert_eq!(
        run(&mut km, &["g", "U", "U"]),
        vec![op(Operator::Upper, Target::Line(1))]
    );
}

#[test]
fn join_replace_and_toggle_case() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["3", "J"]),
        vec![EditCommand::JoinLines {
            count: 3,
            spaces: true
        }]
    );
    assert_eq!(
        run(&mut km, &["g", "J"]),
        vec![EditCommand::JoinLines {
            count: 1,
            spaces: false
        }]
    );
    assert_eq!(
        run(&mut km, &["2", "r", "x"]),
        vec![EditCommand::ReplaceChar { ch: 'x', count: 2 }]
    );
    assert_eq!(
        run(&mut km, &["~"]),
        vec![
            op(Operator::ToggleCase, Target::Motion(Motion::Right, 1)),
            EditCommand::Move(Motion::RightBounded)
        ]
    );
}

#[test]
fn shift_d_c_s_and_y_bindings() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["D"]),
        vec![op(Operator::Delete, Target::Motion(Motion::LineEnd, 1))]
    );
    assert_eq!(
        run(&mut km, &["C"]),
        vec![op(Operator::Change, Target::Motion(Motion::LineEnd, 1))]
    );
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["S"]),
        vec![op(Operator::Change, Target::Line(1))]
    );
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["2", "Y"]),
        vec![op(Operator::Yank, Target::Line(2))]
    );
    assert_eq!(
        run(&mut km, &["3", "x"]),
        vec![op(Operator::Delete, Target::Motion(Motion::Right, 3))]
    );
}

#[test]
fn put_carries_a_count_and_direction() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["2", "p"]),
        vec![EditCommand::Put {
            before: false,
            count: 2,
            register: None
        }]
    );
    assert_eq!(
        run(&mut km, &["P"]),
        vec![EditCommand::Put {
            before: true,
            count: 1,
            register: None
        }]
    );
}

#[test]
fn open_line_replaces_the_newline_dance() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["o"]),
        vec![EditCommand::OpenLine { above: false }]
    );
    assert_eq!(km.mode(), EditMode::Insert);
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["O"]),
        vec![EditCommand::OpenLine { above: true }]
    );
}

#[test]
fn visual_operators_act_on_the_selection() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["v"]),
        vec![EditCommand::SetMode(EditMode::Visual)]
    );
    assert_eq!(
        run(&mut km, &["l"]),
        vec![EditCommand::Select(Motion::RightBounded)]
    );
    assert_eq!(
        run(&mut km, &["y"]),
        vec![
            op(Operator::Yank, Target::Selection),
            EditCommand::SetMode(EditMode::Normal)
        ]
    );
    assert_eq!(km.mode(), EditMode::Normal);
}

#[test]
fn visual_motions_take_counts() {
    let mut km = VimKeymap::default();
    run(&mut km, &["v"]);
    assert_eq!(
        run(&mut km, &["3", "j"]),
        vec![
            EditCommand::Select(Motion::Down),
            EditCommand::Select(Motion::Down),
            EditCommand::Select(Motion::Down)
        ]
    );
}

#[test]
fn visual_indent_and_swap_ends() {
    let mut km = VimKeymap::default();
    run(&mut km, &["v"]);
    assert_eq!(run(&mut km, &["o"]), vec![EditCommand::SwapSelectionEnds]);
    assert_eq!(
        run(&mut km, &[">"]),
        vec![
            op(Operator::Indent, Target::Selection),
            EditCommand::SetMode(EditMode::Normal)
        ]
    );
}

#[test]
fn document_jump_bindings() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["g", "g"]),
        vec![EditCommand::Move(Motion::DocStart)]
    );
    assert_eq!(
        run(&mut km, &["G"]),
        vec![EditCommand::Move(Motion::DocEnd)]
    );
    assert_eq!(
        run(&mut km, &["5", "G"]),
        vec![EditCommand::Move(Motion::GotoLine(4))]
    );
    assert_eq!(
        run(&mut km, &["5", "g", "g"]),
        vec![EditCommand::Move(Motion::GotoLine(4))]
    );
}

#[test]
fn braces_move_by_paragraph() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["{"]),
        vec![EditCommand::Move(Motion::ParagraphPrev)]
    );
    assert_eq!(
        run(&mut km, &["}"]),
        vec![EditCommand::Move(Motion::ParagraphNext)]
    );
}

#[test]
fn fold_and_lsp_bindings() {
    let mut km = VimKeymap::default();
    assert_eq!(run(&mut km, &["z", "a"]), vec![EditCommand::FoldToggle]);
    assert_eq!(run(&mut km, &["z", "R"]), vec![EditCommand::UnfoldAll]);
    assert_eq!(run(&mut km, &["g", "d"]), vec![EditCommand::GotoDefinition]);
    assert_eq!(run(&mut km, &["g", "r"]), vec![EditCommand::FindReferences]);
    assert_eq!(run(&mut km, &["K"]), vec![EditCommand::Hover]);
}

#[test]
fn insert_mode_entry_and_escape() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["i"]),
        vec![EditCommand::SetMode(EditMode::Insert)]
    );
    assert_eq!(km.mode(), EditMode::Insert);
    assert_eq!(
        run(&mut km, &["Escape"]),
        vec![
            EditCommand::Move(Motion::LeftBounded),
            EditCommand::SetMode(EditMode::Normal)
        ]
    );
    assert_eq!(km.mode(), EditMode::Normal);
}

#[test]
fn arrow_keys_navigate_in_normal_mode() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["ArrowDown"]),
        vec![EditCommand::Move(Motion::Down)]
    );
}

#[test]
fn ctrl_navigation_never_falls_through_to_vim_commands() {
    let mut km = VimKeymap::default();
    assert_eq!(
        km.handle(&chord("n", ctrl())),
        vec![EditCommand::Move(Motion::Down)]
    );
    assert_eq!(
        km.handle(&chord("p", ctrl())),
        vec![EditCommand::Move(Motion::Up)]
    );
    assert_eq!(km.mode(), EditMode::Normal);
}

#[test]
fn ctrl_e_y_scroll_the_viewport_with_counts() {
    let mut km = VimKeymap::default();
    run(&mut km, &["3"]);
    assert_eq!(
        km.handle(&chord("e", ctrl())),
        vec![EditCommand::ScrollViewport(3)]
    );
    assert_eq!(
        km.handle(&chord("y", ctrl())),
        vec![EditCommand::ScrollViewport(-1)]
    );
}

#[test]
fn ctrl_r_redoes() {
    let mut km = VimKeymap::default();
    assert_eq!(km.handle(&chord("r", ctrl())), vec![EditCommand::Redo]);
}

#[test]
fn ctrl_c_escapes_insert_and_visual() {
    let mut km = VimKeymap::default();
    run(&mut km, &["i"]);
    assert_eq!(
        km.handle(&chord("c", ctrl())),
        vec![
            EditCommand::Move(Motion::LeftBounded),
            EditCommand::SetMode(EditMode::Normal)
        ]
    );
    run(&mut km, &["v"]);
    assert_eq!(
        km.handle(&chord("c", ctrl())),
        vec![EditCommand::SetMode(EditMode::Normal)]
    );
    assert_eq!(km.mode(), EditMode::Normal);
}

#[test]
fn ex_write_saves() {
    let mut km = VimKeymap::default();
    assert_eq!(run(&mut km, &[":", "w", "Enter"]), vec![EditCommand::Save]);
    assert_eq!(run(&mut km, &[":", "q", "Enter"]), vec![]);
}

#[test]
fn a_prompt_reports_command_line_mode_and_its_text() {
    let mut km = VimKeymap::default();
    run(&mut km, &[":", "w", "q"]);
    assert_eq!(km.mode(), EditMode::CommandLine);
    assert_eq!(km.command_line().as_deref(), Some(":wq"));
    run(&mut km, &["Escape"]);
    assert_eq!(km.mode(), EditMode::Normal);
    assert_eq!(km.command_line(), None);
}

#[test]
fn slash_starts_a_forward_search_and_question_a_backward_one() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &["/", "f", "o", "o", "Enter"]),
        vec![EditCommand::SetSearch {
            pattern: "foo".into(),
            forward: true
        }]
    );
    assert_eq!(
        run(&mut km, &["?", "b", "a", "r", "Enter"]),
        vec![EditCommand::SetSearch {
            pattern: "bar".into(),
            forward: false
        }]
    );
}

#[test]
fn backspacing_an_empty_prompt_leaves_command_line_mode() {
    let mut km = VimKeymap::default();
    run(&mut km, &["/", "a", "Backspace"]);
    assert_eq!(km.mode(), EditMode::Normal);
}

#[test]
fn ex_substitute_and_nohl_reach_the_core() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(
            &mut km,
            &[":", "%", "s", "/", "a", "/", "b", "/", "g", "Enter"]
        ),
        vec![EditCommand::Substitute {
            range: crate::edit::ex::ExRange::WholeFile,
            pattern: "a".into(),
            replacement: "b".into(),
            all: true,
        }]
    );
    assert_eq!(
        run(&mut km, &[":", "n", "o", "h", "Enter"]),
        vec![EditCommand::ClearSearchHighlight]
    );
}

#[test]
fn a_bare_line_number_jumps() {
    let mut km = VimKeymap::default();
    assert_eq!(
        run(&mut km, &[":", "1", "2", "Enter"]),
        vec![EditCommand::Move(Motion::GotoLine(11))]
    );
}

#[test]
fn pointer_drag_enters_visual_and_click_returns_normal() {
    let mut km = VimKeymap::default();
    assert_eq!(
        km.pointer_selection_mode(true),
        Some(EditCommand::SetMode(EditMode::Visual))
    );
    assert_eq!(km.mode(), EditMode::Visual);
    assert_eq!(
        km.pointer_selection_mode(false),
        Some(EditCommand::SetMode(EditMode::Normal))
    );
}
