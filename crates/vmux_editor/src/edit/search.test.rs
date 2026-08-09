use super::*;

#[test]
fn magic_mode_flips_which_operators_need_a_backslash() {
    assert_eq!(translate("a\\+b"), "a+b");
    assert_eq!(translate("a+b"), "a\\+b");
    assert_eq!(translate("foo\\|bar"), "foo|bar");
    assert_eq!(translate("a.c"), "a.c");
}

#[test]
fn word_boundaries_become_regex_boundaries() {
    assert_eq!(translate("\\<word\\>"), "\\bword\\b");
}

#[test]
fn very_nomagic_escapes_everything() {
    assert_eq!(translate("\\Va.c+"), regex::escape("a.c+"));
}

#[test]
fn very_magic_passes_operators_through() {
    assert_eq!(translate("\\v(a|b)+"), "(a|b)+");
}

#[test]
fn case_insensitive_flag_is_hoisted() {
    assert_eq!(translate("foo\\c"), "(?i)foo");
}

/// `\a` used to reach `regex` as BEL and match the wrong thing; `\l`, `\u` and `\x` are not
/// `regex` escapes at all, so `Search::new` dropped the pattern entirely.
#[test]
fn character_class_aliases_translate_rather_than_leak() {
    assert_eq!(translate("\\a"), "[A-Za-z]");
    assert_eq!(translate("\\l\\u"), "[a-z][A-Z]");
    assert_eq!(translate("\\d\\x"), "[0-9][0-9A-Fa-f]");
    assert_eq!(translate("\\h"), "[A-Za-z_]");

    assert!(Search::new("\\afoo", true).is_some());
    assert!(Search::new("\\x2", true).is_some());
}

#[test]
fn stepping_wraps_at_both_ends() {
    let m = vec![2..5, 10..12];
    assert_eq!(step(&m, 0, true), Some(2));
    assert_eq!(step(&m, 2, true), Some(10));
    assert_eq!(step(&m, 10, true), Some(2));
    assert_eq!(step(&m, 12, false), Some(10));
    assert_eq!(step(&m, 2, false), Some(10));
    assert_eq!(step(&[], 0, true), None);
}

#[test]
fn an_invalid_pattern_yields_no_search() {
    assert!(Search::new("\\v(unclosed", true).is_none());
}
