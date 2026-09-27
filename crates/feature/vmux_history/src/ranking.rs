pub fn frecency(visit_count: u32, last_visited_at: i64, now: i64) -> f32 {
    let age_hours = ((now - last_visited_at).max(0) as f32) / 3_600_000.0;
    let decay = 1.0 / (1.0 + age_hours / 24.0);
    (visit_count as f32) * decay
}

pub fn match_strength(query: &str, url: &str, title: &str) -> f32 {
    if query.is_empty() {
        return 1.0;
    }
    let q = query.to_lowercase();
    let u = url.to_lowercase();
    let t = title.to_lowercase();
    let mut score = 0.0;
    if u.starts_with(&q) {
        score += 3.0;
    }
    if t.starts_with(&q) {
        score += 2.0;
    }
    if u.contains(&q) && !u.starts_with(&q) {
        score += 1.0;
    }
    if t.contains(&q) && !t.starts_with(&q) {
        score += 1.0;
    }
    score
}

pub fn score(
    visit_count: u32,
    last_visited_at: i64,
    now: i64,
    query: &str,
    url: &str,
    title: &str,
) -> f32 {
    let m = match_strength(query, url, title);
    if m == 0.0 {
        return 0.0;
    }
    frecency(visit_count, last_visited_at, now) * m
}

#[cfg(test)]
mod tests {
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
}
