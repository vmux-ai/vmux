pub fn resolve_url(cmd_url: Option<&str>, startup_url: Option<&str>) -> String {
    cmd_url
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| startup_url.filter(|s| !s.is_empty()).map(|s| s.to_string()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_url_prefers_explicit_url() {
        let resolved = resolve_url(Some("https://explicit"), Some("https://startup"));
        assert_eq!(resolved, "https://explicit");
    }

    #[test]
    fn resolve_url_falls_back_to_startup_when_none() {
        let resolved = resolve_url(None, Some("https://startup"));
        assert_eq!(resolved, "https://startup");
    }

    #[test]
    fn resolve_url_empty_string_is_treated_as_none() {
        let resolved = resolve_url(Some(""), Some("https://startup"));
        assert_eq!(resolved, "https://startup");
    }

    #[test]
    fn resolve_url_empty_when_neither_provided() {
        let resolved = resolve_url(None, None);
        assert_eq!(resolved, "");
    }
}
