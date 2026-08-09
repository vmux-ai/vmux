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
#[path = "ranking.test.rs"]
mod tests;
