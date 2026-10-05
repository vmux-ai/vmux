pub(super) struct HistoryQuery;

impl HistoryQuery {
    pub(super) fn parse(query: &str) -> Option<&str> {
        let trimmed = query.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('>')
            || trimmed.starts_with('/')
            || trimmed.starts_with('~')
            || trimmed.starts_with("vmux://")
            || trimmed.starts_with("file:")
        {
            return None;
        }
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_only_queries_page_like_text() {
        assert_eq!(HistoryQuery::parse("rust docs"), Some("rust docs"));
        assert_eq!(HistoryQuery::parse("example.com"), Some("example.com"));
        assert_eq!(HistoryQuery::parse(""), None);
        assert_eq!(HistoryQuery::parse("> close"), None);
        assert_eq!(HistoryQuery::parse("/usr/bin"), None);
        assert_eq!(HistoryQuery::parse("~/notes"), None);
        assert_eq!(HistoryQuery::parse("vmux://settings/"), None);
        assert_eq!(HistoryQuery::parse("file:///tmp/a"), None);
    }
}
