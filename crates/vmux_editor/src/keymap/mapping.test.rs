use super::*;

fn spec(mode: &str, lhs: &str, rhs: &str) -> vmux_core::editor::KeyMapping {
    vmux_core::editor::KeyMapping {
        mode: mode.into(),
        lhs: lhs.into(),
        rhs: rhs.into(),
    }
}

#[test]
fn notation_expands_leader_and_named_keys() {
    let keys = parse_keys("<leader>w", " ");
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0].key, " ");
    assert_eq!(keys[1].key, "w");

    let esc = parse_keys("<Esc>", "");
    assert_eq!(esc[0].key, "Escape");
}

#[test]
fn modifier_notation_sets_mods() {
    let keys = parse_keys("<C-x>", "");
    assert!(keys[0].mods.ctrl);
    assert_eq!(keys[0].key, "x");
}

#[test]
fn an_exact_match_expands_and_a_prefix_pends() {
    let maps = Mappings::new(&[spec("n", "gh", "^"), spec("n", "ghi", "$")], " ");
    let g = parse_keys("g", "");
    assert!(matches!(
        maps.match_keys(EditMode::Normal, &g),
        MatchResult::Pending
    ));
    let gh = parse_keys("gh", "");
    assert!(matches!(
        maps.match_keys(EditMode::Normal, &gh),
        MatchResult::Expand(_)
    ));
    let zz = parse_keys("zz", "");
    assert!(matches!(
        maps.match_keys(EditMode::Normal, &zz),
        MatchResult::Miss
    ));
}

#[test]
fn scope_limits_which_mode_sees_a_mapping() {
    let maps = Mappings::new(&[spec("i", "jk", "<Esc>")], " ");
    let j = parse_keys("j", "");
    assert!(matches!(
        maps.match_keys(EditMode::Insert, &j),
        MatchResult::Pending
    ));
    assert!(matches!(
        maps.match_keys(EditMode::Normal, &j),
        MatchResult::Miss
    ));
}

#[test]
fn an_empty_mode_spec_covers_normal_and_visual() {
    let scope = MapScope::parse("");
    assert!(scope.covers(EditMode::Normal));
    assert!(scope.covers(EditMode::Visual));
    assert!(!scope.covers(EditMode::Insert));
}
