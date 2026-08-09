use super::*;

fn core(text: &str) -> EditCore {
    EditCore::new(
        PathBuf::from("a.txt"),
        "Plain Text".into(),
        text,
        EditMode::Insert,
    )
}
fn text_of(c: &EditCore) -> String {
    c.buffer.text()
}
fn op(operator: Operator, target: Target) -> EditCommand {
    EditCommand::Op {
        operator,
        target,
        register: None,
    }
}
fn put(before: bool) -> EditCommand {
    EditCommand::Put {
        before,
        count: 1,
        register: None,
    }
}

#[test]
fn insert_text_moves_caret() {
    let mut c = core("");
    c.apply(EditCommand::InsertText("hi".into()));
    assert_eq!(text_of(&c), "hi");
    assert_eq!(c.primary().head, 2);
    assert!(c.dirty);
}

#[test]
fn backspace_deletes_prev_char() {
    let mut c = core("ab");
    c.set_caret(2);
    c.apply(EditCommand::DeleteBack);
    assert_eq!(text_of(&c), "a");
}

#[test]
fn word_next_motion() {
    let mut c = core("foo bar");
    c.set_caret(0);
    c.apply(EditCommand::Move(Motion::WordNext));
    assert_eq!(c.primary().head, 4);
}

#[test]
fn bounded_horizontal_motion_stays_on_the_current_line() {
    let mut c = core("ab\ncd");
    c.set_caret(c.buffer.coords_to_char(1, 0));
    c.apply(EditCommand::Move(Motion::LeftBounded));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 0));

    c.set_caret(c.buffer.coords_to_char(0, 1));
    c.apply(EditCommand::Move(Motion::RightBounded));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 1));
}

#[test]
fn normal_mode_clamps_every_cursor_target_to_a_line_cell() {
    let mut c = core("ab\ncd");
    c.mode = EditMode::Normal;

    c.set_caret(c.buffer.coords_to_char(0, 0));
    c.apply(EditCommand::Move(Motion::LineEnd));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 1));

    c.apply(EditCommand::Move(Motion::DocEnd));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 1));

    c.apply(EditCommand::Move(Motion::WordNext));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 1));

    c.mode = EditMode::Insert;
    c.set_caret(c.buffer.coords_to_char(0, 2));
    c.apply(EditCommand::SetMode(EditMode::Normal));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 1));
}

#[test]
fn paragraph_motion_moves_between_visible_paragraph_starts() {
    let mut c = core("one\ntwo\n\nthree\nfour\n\nfive\n");
    c.set_caret(c.buffer.coords_to_char(1, 2));
    c.apply(EditCommand::Move(Motion::ParagraphNext));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (3, 0));
    c.apply(EditCommand::Move(Motion::ParagraphPrev));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 0));

    c.apply(EditCommand::Move(Motion::ParagraphPrev));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 0));

    c.set_caret(c.buffer.coords_to_char(6, 0));
    c.apply(EditCommand::Move(Motion::ParagraphNext));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (6, 0));
}

#[test]
fn visual_delete_covers_the_character_under_the_cursor() {
    let mut c = core("abcdef");
    c.set_caret(1);
    c.mode = EditMode::Visual;
    c.apply(EditCommand::Select(Motion::Right));
    c.apply(EditCommand::Select(Motion::Right));
    c.apply(op(Operator::Delete, Target::Selection));
    assert_eq!(text_of(&c), "aef");
}

#[test]
fn delete_range_word() {
    let mut c = core("foo bar");
    c.set_caret(0);
    c.apply(op(Operator::Delete, Target::Motion(Motion::WordNext, 1)));
    assert_eq!(text_of(&c), "bar");
}

#[test]
fn find_char_lands_on_or_before_the_target() {
    let mut c = core("alpha, beta, gamma");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Move(Motion::FindChar {
        ch: ',',
        forward: true,
        till: false,
    }));
    assert_eq!(c.primary().head, 5);
    c.set_caret(0);
    c.apply(EditCommand::Move(Motion::FindChar {
        ch: ',',
        forward: true,
        till: true,
    }));
    assert_eq!(c.primary().head, 4);
}

#[test]
fn find_char_stays_on_the_cursor_line() {
    let mut c = core("abc\nx,y\n");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Move(Motion::FindChar {
        ch: ',',
        forward: true,
        till: false,
    }));
    assert_eq!(c.primary().head, 0);
}

#[test]
fn forward_find_is_inclusive_as_an_operator_target() {
    let mut c = core("alpha, beta");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(op(
        Operator::Delete,
        Target::Motion(
            Motion::FindChar {
                ch: ',',
                forward: true,
                till: true,
            },
            1,
        ),
    ));
    assert_eq!(text_of(&c), ", beta");
}

#[test]
fn counted_find_reaches_the_nth_match() {
    let mut c = core("a,b,c,d");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(op(
        Operator::Delete,
        Target::Motion(
            Motion::FindChar {
                ch: ',',
                forward: true,
                till: false,
            },
            2,
        ),
    ));
    assert_eq!(text_of(&c), "c,d");
}

#[test]
fn match_pair_jumps_both_directions() {
    let mut c = core("foo(bar[baz])");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Move(Motion::MatchPair));
    assert_eq!(c.primary().head, 12);
    c.apply(EditCommand::Move(Motion::MatchPair));
    assert_eq!(c.primary().head, 3);
}

#[test]
fn big_word_motions_span_punctuation() {
    let mut c = core("foo.bar baz");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Move(Motion::BigWordNext));
    assert_eq!(c.primary().head, 8);
    c.apply(EditCommand::Move(Motion::BigWordPrev));
    assert_eq!(c.primary().head, 0);
    c.apply(EditCommand::Move(Motion::BigWordEnd));
    assert_eq!(c.primary().head, 6);
}

#[test]
fn word_end_prev_walks_back_to_the_previous_word() {
    let mut c = core("one two three");
    c.mode = EditMode::Normal;
    c.set_caret(8);
    c.apply(EditCommand::Move(Motion::WordEndPrev));
    assert_eq!(c.primary().head, 6);
}

#[test]
fn screen_motions_use_the_viewport_top() {
    let mut c = core("a\nb\nc\nd\ne\nf\ng\n");
    c.mode = EditMode::Normal;
    c.rows = 4;
    c.top_row = 2;
    c.apply(EditCommand::Move(Motion::ScreenTop));
    assert_eq!(c.buffer.char_to_coords(c.primary().head).0, 2);
    c.apply(EditCommand::Move(Motion::ScreenBottom));
    assert_eq!(c.buffer.char_to_coords(c.primary().head).0, 5);
    c.apply(EditCommand::Move(Motion::ScreenMiddle));
    assert_eq!(c.buffer.char_to_coords(c.primary().head).0, 3);
}

#[test]
fn column_motion_is_one_based() {
    let mut c = core("abcdef");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Move(Motion::Column(4)));
    assert_eq!(c.primary().head, 3);
}

fn block(text: &str, from: (usize, usize), to: (usize, usize)) -> EditCore {
    let mut c = core(text);
    c.mode = EditMode::VisualBlock;
    c.selections = vec![Selection {
        anchor: c.buffer.coords_to_char(from.0, from.1),
        head: c.buffer.coords_to_char(to.0, to.1),
    }];
    c
}

#[test]
fn a_new_edit_after_undo_branches_and_time_travel_reaches_the_old_one() {
    let mut c = core("");
    c.apply(EditCommand::InsertText("first".into()));
    c.apply(EditCommand::Undo);
    assert_eq!(text_of(&c), "");
    c.apply(EditCommand::InsertText("second".into()));
    assert_eq!(text_of(&c), "second");

    c.apply(EditCommand::UndoTime {
        forward: false,
        count: 1,
    });
    assert_eq!(text_of(&c), "first", "g- reaches the abandoned branch");
    c.apply(EditCommand::UndoTime {
        forward: true,
        count: 1,
    });
    assert_eq!(text_of(&c), "second");
}

#[test]
fn bounded_horizontal_motion_stops_at_a_hard_wrap() {
    let mut c = core("alpha\nbeta\n");
    c.mode = EditMode::Normal;
    c.set_caret(c.buffer.coords_to_char(0, 4));
    c.apply(EditCommand::Move(Motion::RightBounded));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 4));
    c.set_caret(c.buffer.coords_to_char(1, 0));
    c.apply(EditCommand::Move(Motion::LeftBounded));
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 0));
}

#[test]
fn increment_finds_the_number_at_or_after_the_caret() {
    let mut c = core("item 41 done");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Increment(1));
    assert_eq!(text_of(&c), "item 42 done");
}

#[test]
fn increment_crosses_zero_on_a_negative_number() {
    let mut c = core("x -1 y");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Increment(2));
    assert_eq!(text_of(&c), "x 1 y");
}

#[test]
fn increment_preserves_zero_padding() {
    let mut c = core("v007");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Increment(1));
    assert_eq!(text_of(&c), "v008");
}

#[test]
fn increment_without_a_number_is_inert() {
    let mut c = core("no digits");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Increment(1));
    assert_eq!(text_of(&c), "no digits");
}

#[test]
fn block_delete_removes_a_rectangle() {
    let mut c = block("abcd\nefgh\nijkl\n", (0, 1), (2, 2));
    c.apply(op(Operator::Delete, Target::Selection));
    assert_eq!(text_of(&c), "ad\neh\nil\n");
    assert_eq!(c.mode, EditMode::Normal);
}

#[test]
fn block_yank_joins_rows_with_newlines() {
    let mut c = block("abcd\nefgh\n", (0, 1), (1, 2));
    let out = c.apply(op(Operator::Yank, Target::Selection));
    assert_eq!(
        out.yank,
        Some(RegisterValue {
            text: "bc\nfg".into(),
            kind: RegisterKind::Blockwise,
        })
    );
}

#[test]
fn block_rows_clamp_to_short_lines() {
    let c = block("abcd\nx\nijkl\n", (0, 1), (2, 2));
    let rows = c.block_rows();
    assert_eq!(rows.len(), 3);
    assert!(rows[1].is_empty(), "the short line contributes no columns");
}

#[test]
fn block_put_inserts_a_column_on_each_line() {
    let mut c = core("ab\ncd\n");
    c.mode = EditMode::Normal;
    c.registers.set_unnamed(RegisterValue {
        text: "X\nY".into(),
        kind: RegisterKind::Blockwise,
    });
    c.set_caret(0);
    c.apply(put(true));
    assert_eq!(text_of(&c), "Xab\nYcd\n");
}

#[test]
fn block_put_pads_short_lines_out_to_the_column() {
    let mut c = core("abcd\nx\n");
    c.mode = EditMode::Normal;
    c.registers.set_unnamed(RegisterValue {
        text: "1\n2".into(),
        kind: RegisterKind::Blockwise,
    });
    c.set_caret(c.buffer.coords_to_char(0, 3));
    c.apply(put(true));
    assert_eq!(text_of(&c), "abc1d\nx  2\n");
}

#[test]
fn block_case_operator_covers_every_row() {
    let mut c = block("abcd\nefgh\n", (0, 0), (1, 1));
    c.apply(op(Operator::Upper, Target::Selection));
    assert_eq!(text_of(&c), "ABcd\nEFgh\n");
}

#[test]
fn block_insert_replicates_typed_text_down_the_column() {
    let mut c = block("abc\ndef\nghi\n", (0, 1), (2, 1));
    c.apply(EditCommand::BeginBlockInsert { after: false });
    assert_eq!(c.mode, EditMode::Insert);
    c.apply(EditCommand::InsertText("-".into()));
    c.apply(EditCommand::FinishBlockInsert { text: "-".into() });
    assert_eq!(text_of(&c), "a-bc\nd-ef\ng-hi\n");
}

#[test]
fn block_append_inserts_after_the_right_edge() {
    let mut c = block("abc\ndef\n", (0, 0), (1, 0));
    c.apply(EditCommand::BeginBlockInsert { after: true });
    c.apply(EditCommand::InsertText("!".into()));
    c.apply(EditCommand::FinishBlockInsert { text: "!".into() });
    assert_eq!(text_of(&c), "a!bc\nd!ef\n");
}

#[test]
fn block_selection_renders_one_span_per_row() {
    let c = block("abcd\nefgh\n", (0, 1), (1, 2));
    let spans = c.sel_spans(0, 4);
    assert_eq!(spans.len(), 2);
    assert!(spans.iter().all(|s| s.start == 1 && s.end == 3));
}

#[test]
fn overtype_replaces_characters_and_backspace_restores_them() {
    let mut c = core("abcd");
    c.mode = EditMode::Replace;
    c.set_caret(0);
    c.apply(EditCommand::OvertypeText("XY".into()));
    assert_eq!(text_of(&c), "XYcd");
    c.apply(EditCommand::DeleteBack);
    assert_eq!(text_of(&c), "Xbcd");
    c.apply(EditCommand::DeleteBack);
    assert_eq!(text_of(&c), "abcd");
}

#[test]
fn overtype_past_the_line_end_appends() {
    let mut c = core("ab\ncd\n");
    c.mode = EditMode::Replace;
    c.set_caret(2);
    c.apply(EditCommand::OvertypeText("XY".into()));
    assert_eq!(text_of(&c), "abXY\ncd\n");
}

#[test]
fn substitute_replaces_the_first_match_per_line_without_g() {
    let mut c = core("aa bb aa\naa cc\n");
    c.mode = EditMode::Normal;
    c.apply(EditCommand::Substitute {
        range: crate::edit::ex::ExRange::WholeFile,
        pattern: "aa".into(),
        replacement: "X".into(),
        all: false,
    });
    assert_eq!(text_of(&c), "X bb aa\nX cc\n");
}

#[test]
fn substitute_with_g_replaces_every_match() {
    let mut c = core("aa bb aa\n");
    c.mode = EditMode::Normal;
    c.apply(EditCommand::Substitute {
        range: crate::edit::ex::ExRange::WholeFile,
        pattern: "aa".into(),
        replacement: "X".into(),
        all: true,
    });
    assert_eq!(text_of(&c), "X bb X\n");
}

#[test]
fn substitute_honours_a_line_range() {
    let mut c = core("a\na\na\n");
    c.mode = EditMode::Normal;
    c.apply(EditCommand::Substitute {
        range: crate::edit::ex::ExRange::Lines(1, 1),
        pattern: "a".into(),
        replacement: "b".into(),
        all: false,
    });
    assert_eq!(text_of(&c), "a\nb\na\n");
}

#[test]
fn substitute_expands_ampersand_to_the_match() {
    let mut c = core("cat\n");
    c.mode = EditMode::Normal;
    c.apply(EditCommand::Substitute {
        range: crate::edit::ex::ExRange::WholeFile,
        pattern: "cat".into(),
        replacement: "[&]".into(),
        all: false,
    });
    assert_eq!(text_of(&c), "[cat]\n");
}

#[test]
fn ex_delete_removes_lines_and_yanks_them_linewise() {
    let mut c = core("one\ntwo\nthree\n");
    c.mode = EditMode::Normal;
    c.apply(EditCommand::ExDelete(crate::edit::ex::ExRange::Lines(0, 1)));
    assert_eq!(text_of(&c), "three\n");
    assert_eq!(
        c.registers.read(None),
        Some(&RegisterValue::linewise("one\ntwo\n"))
    );
}

#[test]
fn search_jumps_to_the_next_match_and_wraps() {
    let mut c = core("alpha beta alpha gamma");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::SetSearch {
        pattern: "alpha".into(),
        forward: true,
    });
    assert_eq!(c.primary().head, 11);
    c.apply(EditCommand::Move(Motion::SearchNext { reverse: false }));
    assert_eq!(c.primary().head, 0);
    c.apply(EditCommand::Move(Motion::SearchNext { reverse: true }));
    assert_eq!(c.primary().head, 11);
}

#[test]
fn a_backward_search_reverses_what_n_means() {
    let mut c = core("x a x a x");
    c.mode = EditMode::Normal;
    c.set_caret(4);
    c.apply(EditCommand::SetSearch {
        pattern: "x".into(),
        forward: false,
    });
    assert_eq!(c.primary().head, 0);
    c.apply(EditCommand::Move(Motion::SearchNext { reverse: false }));
    assert_eq!(c.primary().head, 8);
}

#[test]
fn star_searches_the_whole_word_under_the_cursor() {
    let mut c = core("cat category cat");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::SearchWord { forward: true });
    assert_eq!(c.primary().head, 13);
}

#[test]
fn search_composes_with_an_operator() {
    let mut c = core("one two three");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::SetSearch {
        pattern: "three".into(),
        forward: true,
    });
    c.set_caret(0);
    c.apply(op(
        Operator::Delete,
        Target::Motion(Motion::SearchNext { reverse: false }, 1),
    ));
    assert_eq!(text_of(&c), "three");
}

#[test]
fn an_invalid_pattern_leaves_the_caret_alone() {
    let mut c = core("abc");
    c.mode = EditMode::Normal;
    c.set_caret(1);
    c.apply(EditCommand::SetSearch {
        pattern: "\\v(".into(),
        forward: true,
    });
    assert_eq!(c.primary().head, 1);
    assert!(c.search.is_none());
}

#[test]
fn highlight_spans_appear_only_while_highlighting_is_on() {
    let mut c = core("foo bar foo\n");
    c.mode = EditMode::Normal;
    c.apply(EditCommand::SetSearch {
        pattern: "foo".into(),
        forward: true,
    });
    assert_eq!(c.search_spans(0, 4).len(), 2);
    c.apply(EditCommand::ClearSearchHighlight);
    assert!(c.search_spans(0, 4).is_empty());
}

#[test]
fn marks_return_to_a_saved_position() {
    let mut c = core("one\ntwo\nthree\n");
    c.mode = EditMode::Normal;
    c.set_caret(c.buffer.coords_to_char(1, 1));
    c.apply(EditCommand::SetMark('a'));
    c.set_caret(0);
    c.apply(EditCommand::GotoMark {
        name: 'a',
        linewise: false,
    });
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 1));
}

#[test]
fn a_linewise_mark_lands_on_the_first_non_blank() {
    let mut c = core("one\n    two\n");
    c.mode = EditMode::Normal;
    c.set_caret(c.buffer.coords_to_char(1, 7));
    c.apply(EditCommand::SetMark('b'));
    c.set_caret(0);
    c.apply(EditCommand::GotoMark {
        name: 'b',
        linewise: true,
    });
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 4));
}

#[test]
fn marks_slide_when_text_is_inserted_before_them() {
    let mut c = core("one\ntwo\n");
    c.mode = EditMode::Normal;
    c.set_caret(c.buffer.coords_to_char(1, 0));
    c.apply(EditCommand::SetMark('a'));
    c.set_caret(0);
    c.apply(EditCommand::OpenLine { above: true });
    c.apply(EditCommand::InsertText("zero".into()));
    c.apply(EditCommand::SetMode(EditMode::Normal));
    c.apply(EditCommand::GotoMark {
        name: 'a',
        linewise: false,
    });
    assert_eq!(c.buffer.char_to_coords(c.primary().head), (2, 0));
}

#[test]
fn an_unset_mark_does_not_move_the_caret() {
    let mut c = core("one\ntwo\n");
    c.mode = EditMode::Normal;
    c.set_caret(1);
    c.apply(EditCommand::GotoMark {
        name: 'z',
        linewise: false,
    });
    assert_eq!(c.primary().head, 1);
}

#[test]
fn the_jump_list_walks_back_and_forward() {
    let mut c = core("a\nb\nc\nd\ne\n");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Move(Motion::DocEnd));
    let end = c.primary().head;
    c.apply(EditCommand::JumpList {
        back: true,
        count: 1,
    });
    assert_eq!(c.primary().head, 0);
    c.apply(EditCommand::JumpList {
        back: false,
        count: 1,
    });
    assert_eq!(c.primary().head, end);
}

#[test]
fn plain_motions_do_not_record_jumps() {
    let mut c = core("a\nb\nc\n");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Move(Motion::Down));
    c.apply(EditCommand::JumpList {
        back: true,
        count: 1,
    });
    assert_eq!(c.buffer.char_to_coords(c.primary().head).0, 1);
}

#[test]
fn the_change_list_revisits_edit_positions() {
    let mut c = core("one\ntwo\nthree\n");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::ReplaceChar { ch: 'X', count: 1 });
    c.set_caret(c.buffer.coords_to_char(2, 0));
    c.apply(EditCommand::ReplaceChar { ch: 'Y', count: 1 });
    c.set_caret(c.buffer.coords_to_char(1, 0));
    c.apply(EditCommand::ChangeList {
        back: true,
        count: 1,
    });
    assert_eq!(c.buffer.char_to_coords(c.primary().head).0, 0);
}

#[test]
fn operator_counts_multiply_the_motion() {
    let mut c = core("one two three four");
    c.set_caret(0);
    c.apply(op(Operator::Delete, Target::Motion(Motion::WordNext, 3)));
    assert_eq!(text_of(&c), "four");
}

#[test]
fn inclusive_motion_covers_its_last_character() {
    let mut c = core("foo bar");
    c.set_caret(0);
    c.apply(op(Operator::Delete, Target::Motion(Motion::WordEnd, 1)));
    assert_eq!(text_of(&c), " bar");
}

#[test]
fn linewise_motion_deletes_whole_lines() {
    let mut c = core("one\ntwo\nthree\n");
    c.set_caret(c.buffer.coords_to_char(0, 1));
    c.apply(op(Operator::Delete, Target::Motion(Motion::Down, 1)));
    assert_eq!(text_of(&c), "three\n");
}

#[test]
fn exclusive_motion_ending_in_column_one_stops_at_the_previous_line() {
    let mut c = core("foo\nbar\n");
    c.set_caret(1);
    c.apply(op(Operator::Delete, Target::Motion(Motion::WordNext, 1)));
    assert_eq!(text_of(&c), "f\nbar\n");
}

#[test]
fn delete_line_yanks_linewise_and_put_opens_a_new_line() {
    let mut c = core("one\ntwo\nthree\n");
    c.mode = EditMode::Normal;
    c.set_caret(c.buffer.coords_to_char(1, 0));
    c.apply(op(Operator::Delete, Target::Line(1)));
    assert_eq!(text_of(&c), "one\nthree\n");
    assert_eq!(
        c.registers.read(None),
        Some(&RegisterValue::linewise("two\n"))
    );
    c.apply(put(false));
    assert_eq!(text_of(&c), "one\nthree\ntwo\n");
}

#[test]
fn count_deletes_multiple_lines() {
    let mut c = core("a\nb\nc\nd\n");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(op(Operator::Delete, Target::Line(3)));
    assert_eq!(text_of(&c), "d\n");
}

#[test]
fn change_line_keeps_indentation() {
    let mut c = core("    foo\nbar\n");
    c.mode = EditMode::Normal;
    c.set_caret(c.buffer.coords_to_char(0, 4));
    c.apply(op(Operator::Change, Target::Line(1)));
    assert_eq!(text_of(&c), "    \nbar\n");
    assert_eq!(c.primary().head, 4);
    assert_eq!(c.mode, EditMode::Insert);
}

#[test]
fn indent_and_outdent_shift_whole_lines() {
    let mut c = core("a\nb\n");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(op(Operator::Indent, Target::Line(2)));
    assert_eq!(text_of(&c), "\ta\n\tb\n");
    c.apply(op(Operator::Outdent, Target::Line(2)));
    assert_eq!(text_of(&c), "a\nb\n");
}

#[test]
fn case_operators_transform_a_range() {
    let mut c = core("hello");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(op(Operator::Upper, Target::Motion(Motion::LineEnd, 1)));
    assert_eq!(text_of(&c), "HELLO");
    c.apply(op(Operator::ToggleCase, Target::Motion(Motion::LineEnd, 1)));
    assert_eq!(text_of(&c), "hello");
}

/// `D` clears to the end of the line without joining the next one. A single-line fixture hides
/// this: the range gets clamped to the buffer length, so the extra step past `\n` is invisible.
#[test]
fn line_end_operator_stops_before_the_newline() {
    let mut c = core("abc\ndef\n");
    c.mode = EditMode::Normal;
    c.set_caret(2);

    c.apply(op(Operator::Delete, Target::Motion(Motion::LineEnd, 1)));

    assert_eq!(text_of(&c), "ab\ndef\n");
}

#[test]
fn replace_char_overwrites_without_entering_insert() {
    let mut c = core("abcd");
    c.mode = EditMode::Normal;
    c.set_caret(1);
    c.apply(EditCommand::ReplaceChar { ch: 'X', count: 2 });
    assert_eq!(text_of(&c), "aXXd");
    assert_eq!(c.primary().head, 2);
}

#[test]
fn replace_char_past_the_line_end_is_rejected() {
    let mut c = core("ab\ncd\n");
    c.mode = EditMode::Normal;
    c.set_caret(1);
    c.apply(EditCommand::ReplaceChar { ch: 'X', count: 4 });
    assert_eq!(text_of(&c), "ab\ncd\n");
}

#[test]
fn join_lines_collapses_indentation_to_one_space() {
    let mut c = core("foo\n    bar\nbaz\n");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::JoinLines {
        count: 2,
        spaces: true,
    });
    assert_eq!(text_of(&c), "foo bar\nbaz\n");
}

#[test]
fn join_without_spaces_keeps_text_adjacent() {
    let mut c = core("foo\n  bar\n");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::JoinLines {
        count: 2,
        spaces: false,
    });
    assert_eq!(text_of(&c), "foobar\n");
}

#[test]
fn open_line_inherits_indentation() {
    let mut c = core("    foo\n");
    c.mode = EditMode::Normal;
    c.set_caret(4);
    c.apply(EditCommand::OpenLine { above: false });
    assert_eq!(text_of(&c), "    foo\n    \n");
    assert_eq!(c.mode, EditMode::Insert);

    let mut c = core("    foo\n");
    c.mode = EditMode::Normal;
    c.set_caret(4);
    c.apply(EditCommand::OpenLine { above: true });
    assert_eq!(text_of(&c), "    \n    foo\n");
}

#[test]
fn linewise_put_before_inserts_above_the_current_line() {
    let mut c = core("one\ntwo\n");
    c.mode = EditMode::Normal;
    c.registers.set_unnamed(RegisterValue::linewise("new\n"));
    c.set_caret(c.buffer.coords_to_char(1, 0));
    c.apply(put(true));
    assert_eq!(text_of(&c), "one\nnew\ntwo\n");
}

#[test]
fn put_repeats_with_a_count() {
    let mut c = core("a");
    c.mode = EditMode::Normal;
    c.registers.set_unnamed(RegisterValue::charwise("X"));
    c.set_caret(0);
    c.apply(EditCommand::Put {
        before: false,
        count: 3,
        register: None,
    });
    assert_eq!(text_of(&c), "aXXX");
}

#[test]
fn named_register_round_trips_through_an_operator() {
    let mut c = core("one\ntwo\n");
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(EditCommand::Op {
        operator: Operator::Yank,
        target: Target::Line(1),
        register: Some('a'),
    });
    c.set_caret(c.buffer.coords_to_char(1, 0));
    c.apply(EditCommand::Put {
        before: false,
        count: 1,
        register: Some('a'),
    });
    assert_eq!(text_of(&c), "one\ntwo\none\n");
}

#[test]
fn cursor_pos_visual_col_for_wide_chars() {
    let mut c = core("あb");
    c.set_caret(1);
    assert_eq!(
        c.cursor_pos(),
        CursorPos {
            line: 0,
            row: 0,
            col: 2
        }
    );
}

#[test]
fn typing_over_selection_replaces() {
    let mut c = core("abcdef");
    c.selections = vec![Selection { anchor: 1, head: 4 }];
    c.apply(EditCommand::InsertText("X".into()));
    assert_eq!(text_of(&c), "aXef");
}

#[test]
fn undo_redo_roundtrip() {
    let mut c = core("");
    c.apply(EditCommand::InsertText("abc".into()));
    c.apply(EditCommand::SetMode(EditMode::Normal));
    c.apply(EditCommand::InsertText("X".into()));
    c.apply(EditCommand::Undo);
    assert_eq!(text_of(&c), "abc");
    c.apply(EditCommand::Undo);
    assert_eq!(text_of(&c), "");
    c.apply(EditCommand::Redo);
    assert_eq!(text_of(&c), "abc");
}

#[test]
fn typing_run_is_one_undo() {
    let mut c = core("");
    c.apply(EditCommand::InsertText("h".into()));
    c.apply(EditCommand::InsertText("i".into()));
    c.apply(EditCommand::Undo);
    assert_eq!(text_of(&c), "");
}

#[test]
fn replace_text_is_one_undoable_edit() {
    let mut c = core("old");
    c.set_caret(2);
    c.apply(EditCommand::ReplaceText("new text".into()));
    assert_eq!(text_of(&c), "new text");
    assert_eq!(c.primary().head, 2);
    c.apply(EditCommand::Undo);
    assert_eq!(text_of(&c), "old");
}

#[test]
fn replace_text_preserves_caret_in_an_unchanged_body() {
    let old = "---\ntitle: Old\n---\nBody text";
    let new = "---\ntitle: Old\ntags:\n  - note\n---\nBody text";
    let mut c = core(old);
    c.set_caret(old.find("text").unwrap());
    c.apply(EditCommand::ReplaceText(new.into()));
    assert_eq!(c.primary().head, new.find("text").unwrap());
}

#[test]
fn yank_and_paste() {
    let mut c = core("abcdef");
    c.set_caret(0);
    c.mode = EditMode::Visual;
    c.apply(EditCommand::Select(Motion::Right));
    c.apply(EditCommand::Select(Motion::Right));
    let out = c.apply(op(Operator::Yank, Target::Selection));
    assert_eq!(out.yank, Some(RegisterValue::charwise("abc")));
    c.mode = EditMode::Insert;
    c.set_caret(6);
    c.apply(put(true));
    assert_eq!(text_of(&c), "abcdefabc");
}

#[test]
fn delete_forward_removes_selection() {
    let mut c = core("abcdef");
    c.selections = vec![Selection { anchor: 1, head: 4 }];
    c.apply(EditCommand::DeleteForward);
    assert_eq!(text_of(&c), "aef");
}

#[test]
fn undo_back_to_saved_is_clean() {
    let mut c = core("");
    c.apply(EditCommand::InsertText("ab".into()));
    c.mark_saved();
    assert!(!c.dirty);
    c.apply(EditCommand::InsertText("c".into()));
    assert!(c.dirty);
    c.apply(EditCommand::Undo);
    assert_eq!(text_of(&c), "ab");
    assert!(!c.dirty, "undo to saved revision clears dirty");
}

#[test]
fn delete_back_is_grapheme_aware() {
    let mut c = core("ae\u{0301}");
    c.set_caret(c.buffer.len_chars());
    c.apply(EditCommand::DeleteBack);
    assert_eq!(text_of(&c), "a");
}

#[test]
fn paste_after_vs_before_in_normal() {
    let mut c = core("ac");
    c.registers.set_unnamed(RegisterValue::charwise("X"));
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(put(false));
    assert_eq!(text_of(&c), "aXc");
    let mut c2 = core("ac");
    c2.registers.set_unnamed(RegisterValue::charwise("X"));
    c2.mode = EditMode::Normal;
    c2.set_caret(0);
    c2.apply(put(true));
    assert_eq!(text_of(&c2), "Xac");
}

#[test]
fn repeated_pastes_undo_one_at_a_time() {
    let mut c = core("a");
    c.registers.set_unnamed(RegisterValue::charwise("X"));
    c.mode = EditMode::Normal;
    c.set_caret(0);
    c.apply(put(false));
    c.apply(put(false));
    assert_eq!(text_of(&c), "aXX");
    c.apply(EditCommand::Undo);
    assert_eq!(text_of(&c), "aX");
    c.apply(EditCommand::Undo);
    assert_eq!(text_of(&c), "a");
}

#[test]
fn autoscroll_follows_caret_down() {
    let mut c = core("a\nb\nc\nd\ne\nf\n");
    c.rows = 3;
    c.set_caret(c.buffer.coords_to_char(5, 0));
    assert_eq!(c.autoscroll(0, 3), Some(3));
}

#[test]
fn down_skips_collapsed_body() {
    let mut c = core("a\nb\nc\nd\ne\n");
    let mut fs = crate::fold::FoldState::default();
    fs.set_regions(vec![crate::fold::FoldRegion { start: 0, end: 2 }]);
    fs.close(0);
    c.fold_view = fs.view(c.buffer.len_lines() as u32);
    c.set_caret(0);
    c.apply(EditCommand::Move(Motion::Down));
    let (line, _) = c.buffer.char_to_coords(c.primary().head);
    assert_eq!(line, 3);
}

#[test]
fn vertical_motion_collapses_selection_before_moving() {
    let mut c = core("first\nsecond\nthird\n");
    let start = c.buffer.coords_to_char(1, 1);
    let end = c.buffer.coords_to_char(2, 3);
    c.selections = vec![Selection {
        anchor: start,
        head: end,
    }];

    c.apply(EditCommand::Move(Motion::Up));

    assert_eq!(c.primary(), Selection::caret(start));
}

#[test]
fn vertical_motion_preserves_column_across_empty_lines() {
    for mode in [EditMode::Insert, EditMode::Normal] {
        let mut c = core("abcdefghij\n\nabcdefghij\n");
        c.fold_view = crate::fold::FoldState::default().view(c.buffer.len_lines() as u32);
        c.mode = mode;
        c.set_caret(c.buffer.coords_to_char(0, 5));

        c.apply(EditCommand::Move(Motion::Down));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 0));

        c.apply(EditCommand::Move(Motion::Down));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (2, 5));
    }
}
