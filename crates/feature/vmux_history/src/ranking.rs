pub struct HistoryRank {
    visit_count: u32,
    last_visited_at: i64,
    now: i64,
}

impl HistoryRank {
    pub fn new(visit_count: u32, last_visited_at: i64, now: i64) -> Self {
        Self {
            visit_count,
            last_visited_at,
            now,
        }
    }

    pub fn score(&self, query: &str, url: &str, title: &str) -> f32 {
        let match_strength = Self::match_strength(query, url, title);
        if match_strength == 0.0 {
            return 0.0;
        }
        self.frecency() * match_strength
    }

    fn frecency(&self) -> f32 {
        let age_hours = ((self.now - self.last_visited_at).max(0) as f32) / 3_600_000.0;
        let decay = 1.0 / (1.0 + age_hours / 24.0);
        (self.visit_count as f32) * decay
    }

    fn match_strength(query: &str, url: &str, title: &str) -> f32 {
        if query.is_empty() {
            return 1.0;
        }
        let query = query.to_lowercase();
        let url = url.to_lowercase();
        let title = title.to_lowercase();
        let mut score = 0.0;
        if url.starts_with(&query) {
            score += 3.0;
        }
        if title.starts_with(&query) {
            score += 2.0;
        }
        if url.contains(&query) && !url.starts_with(&query) {
            score += 1.0;
        }
        if title.contains(&query) && !title.starts_with(&query) {
            score += 1.0;
        }
        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frecency_decays_with_age() {
        let now = 1_000_000_000;
        let recent = HistoryRank::new(10, now - 3_600_000, now).frecency();
        let old = HistoryRank::new(10, now - 100 * 3_600_000, now).frecency();
        assert!(recent > old);
    }

    #[test]
    fn match_strength_url_prefix_beats_substring() {
        let pfx = HistoryRank::match_strength("git", "github.com", "GitHub");
        let mid = HistoryRank::match_strength("hub", "github.com", "GitHub");
        assert!(pfx > mid);
    }

    #[test]
    fn match_strength_zero_on_miss() {
        assert_eq!(
            HistoryRank::match_strength("xyz", "github.com", "GitHub"),
            0.0
        );
    }

    #[test]
    fn match_strength_one_when_query_empty() {
        assert_eq!(HistoryRank::match_strength("", "github.com", "GitHub"), 1.0);
    }

    #[test]
    fn higher_visit_count_ranks_higher_at_equal_match() {
        let now = 1_000_000_000;
        let a = HistoryRank::new(20, now, now).score("git", "github.com", "GitHub");
        let b = HistoryRank::new(2, now, now).score("git", "github.com", "GitHub");
        assert!(a > b);
    }
}
