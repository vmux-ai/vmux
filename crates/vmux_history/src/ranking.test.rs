use super::*;

#[test]
fn frecency_decays_with_age() {
    let now = 1_000_000_000;
    let recent = frecency(10, now - 3_600_000, now);
    let old = frecency(10, now - 100 * 3_600_000, now);
    assert!(recent > old);
}

#[test]
fn match_strength_url_prefix_beats_substring() {
    let pfx = match_strength("git", "github.com", "GitHub");
    let mid = match_strength("hub", "github.com", "GitHub");
    assert!(pfx > mid);
}

#[test]
fn match_strength_zero_on_miss() {
    assert_eq!(match_strength("xyz", "github.com", "GitHub"), 0.0);
}

#[test]
fn match_strength_one_when_query_empty() {
    assert_eq!(match_strength("", "github.com", "GitHub"), 1.0);
}

#[test]
fn higher_visit_count_ranks_higher_at_equal_match() {
    let now = 1_000_000_000;
    let a = score(20, now, now, "git", "github.com", "GitHub");
    let b = score(2, now, now, "git", "github.com", "GitHub");
    assert!(a > b);
}
