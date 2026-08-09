use super::*;

#[test]
fn bare_commands() {
    assert_eq!(parse("w"), Some(ExCommand::Write));
    assert_eq!(parse("wq"), Some(ExCommand::WriteQuit));
    assert_eq!(parse("q"), Some(ExCommand::Quit { force: false }));
    assert_eq!(parse("q!"), Some(ExCommand::Quit { force: true }));
    assert_eq!(parse("noh"), Some(ExCommand::NoHighlight));
    assert_eq!(parse("bogus"), None);
    assert_eq!(parse(""), None);
}

#[test]
fn a_bare_number_is_a_goto() {
    assert_eq!(parse("42"), Some(ExCommand::Goto(41)));
    assert_eq!(parse("1"), Some(ExCommand::Goto(0)));
}

#[test]
fn substitute_parses_pattern_replacement_and_flags() {
    assert_eq!(
        parse("%s/foo/bar/g"),
        Some(ExCommand::Substitute {
            range: ExRange::WholeFile,
            pattern: "foo".into(),
            replacement: "bar".into(),
            all: true,
        })
    );
    assert_eq!(
        parse("s/foo/bar"),
        Some(ExCommand::Substitute {
            range: ExRange::CurrentLine,
            pattern: "foo".into(),
            replacement: "bar".into(),
            all: false,
        })
    );
}

#[test]
fn substitute_accepts_a_line_range_and_a_selection() {
    assert_eq!(
        parse("2,5s/a/b/"),
        Some(ExCommand::Substitute {
            range: ExRange::Lines(1, 4),
            pattern: "a".into(),
            replacement: "b".into(),
            all: false,
        })
    );
    assert_eq!(
        parse("'<,'>s/a/b/"),
        Some(ExCommand::Substitute {
            range: ExRange::Selection,
            pattern: "a".into(),
            replacement: "b".into(),
            all: false,
        })
    );
}

#[test]
fn an_escaped_delimiter_stays_in_the_pattern() {
    assert_eq!(
        parse("s/a\\/b/c/"),
        Some(ExCommand::Substitute {
            range: ExRange::CurrentLine,
            pattern: "a/b".into(),
            replacement: "c".into(),
            all: false,
        })
    );
}

#[test]
fn a_non_slash_delimiter_works() {
    assert_eq!(
        parse("s#a#b#g"),
        Some(ExCommand::Substitute {
            range: ExRange::CurrentLine,
            pattern: "a".into(),
            replacement: "b".into(),
            all: true,
        })
    );
}

#[test]
fn ranged_delete_and_yank() {
    assert_eq!(parse("%d"), Some(ExCommand::Delete(ExRange::WholeFile)));
    assert_eq!(parse("3,4y"), Some(ExCommand::Yank(ExRange::Lines(2, 3))));
}

#[test]
fn a_word_starting_with_s_is_not_a_substitute() {
    assert_eq!(parse("sort"), None);
}
