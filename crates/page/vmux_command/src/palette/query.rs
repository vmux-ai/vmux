use vmux_api::open_target::OpenTarget;

#[derive(Clone, Copy, Debug)]
pub(crate) struct PaletteQuery<'a>(pub(crate) &'a str);

impl PaletteQuery<'_> {
    pub(crate) fn opens_typed_url_on_enter(
        &self,
        open_target: Option<OpenTarget>,
        nav_mode: bool,
    ) -> bool {
        let query = self.0.trim();
        matches!(open_target, Some(OpenTarget::InPlace))
            && !nav_mode
            && !query.is_empty()
            && !self.0.trim_start().starts_with('>')
            && looks_like_url(query)
    }

    pub(crate) fn is_start_prompt(&self) -> bool {
        let query = self.0.trim();
        !query.is_empty()
            && !query.starts_with('>')
            && !looks_like_url(query)
            && !looks_like_explicit_path(query)
    }

    pub(crate) fn slash_token(&self) -> Option<(&str, &str)> {
        let rest = self.0.trim_start().strip_prefix('/')?;
        let (name, tail) = match rest.find(char::is_whitespace) {
            Some(at) => (&rest[..at], rest[at..].trim_start()),
            None => (rest, ""),
        };
        if name.is_empty() {
            return None;
        }
        let named = name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        });
        named.then_some((name, tail))
    }

    pub(crate) fn mcp_filter(&self) -> Option<&str> {
        let rest = self.0.strip_prefix("/mcp")?;
        if rest.is_empty() {
            return Some("");
        }
        rest.chars()
            .next()?
            .is_whitespace()
            .then(|| rest.trim_start())
    }
}

pub fn is_data_uri(value: &str) -> bool {
    value
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
}

pub fn looks_like_url(value: &str) -> bool {
    let value = value.trim();
    if is_data_uri(value) {
        return true;
    }
    if value.chars().any(char::is_whitespace)
        || value.starts_with('/')
        || value.starts_with("~/")
        || value.starts_with("./")
        || value.starts_with("../")
    {
        return false;
    }
    if value.contains("://") {
        return true;
    }
    let before_slash = value.split('/').next().unwrap_or(value);
    before_slash.contains('.')
}

pub fn looks_like_path(value: &str) -> bool {
    if looks_like_url(value) {
        return false;
    }
    value.starts_with('/')
        || value.starts_with("~/")
        || value.starts_with("./")
        || value.starts_with("../")
        || (value.contains('/') && !value.contains(' '))
}

fn looks_like_explicit_path(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with('~')
        || value.starts_with("./")
        || value.starts_with("../")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_token_is_a_bare_name_and_never_a_path() {
        assert_eq!(PaletteQuery("/resume").slash_token(), Some(("resume", "")));
        assert_eq!(
            PaletteQuery("/resume yesterday").slash_token(),
            Some(("resume", "yesterday"))
        );
        assert_eq!(PaletteQuery("  /model").slash_token(), Some(("model", "")));
        assert_eq!(PaletteQuery("/Users/jun/notes.md").slash_token(), None);
        assert_eq!(PaletteQuery("/").slash_token(), None);
        assert_eq!(PaletteQuery("resume").slash_token(), None);
    }

    #[test]
    fn mcp_filter_requires_the_complete_command() {
        assert_eq!(PaletteQuery("/mcp").mcp_filter(), Some(""));
        assert_eq!(PaletteQuery("/mcp linear").mcp_filter(), Some("linear"));
        assert_eq!(PaletteQuery("/mcpx").mcp_filter(), None);
    }

    #[test]
    fn url_and_path_classification_is_unambiguous() {
        assert!(looks_like_url("https://example.com/path"));
        assert!(looks_like_url("google.com/maps"));
        assert!(looks_like_url("data:text/html,<h1>hi</h1>"));
        assert!(!looks_like_url("src/main.rs"));
        assert!(!looks_like_url("search query"));
        assert!(looks_like_path("/usr/bin"));
        assert!(looks_like_path("~/projects"));
        assert!(looks_like_path("src/main.rs"));
        assert!(!looks_like_path("https://example.com/path"));
        assert!(!looks_like_path("some query / thing"));
    }

    #[test]
    fn start_prompt_rejects_navigation_and_commands() {
        assert!(PaletteQuery("fix the failing test").is_start_prompt());
        assert!(PaletteQuery("codex").is_start_prompt());
        for query in [
            "https://example.com",
            "example.com",
            "vmux://settings/",
            "/tmp/file",
            "~/project",
            "./src",
            "../repo",
            "> close tab",
        ] {
            assert!(!PaletteQuery(query).is_start_prompt(), "{query}");
        }
    }

    #[test]
    fn typed_url_open_requires_in_place_without_navigation() {
        assert!(
            PaletteQuery("https://example.com")
                .opens_typed_url_on_enter(Some(OpenTarget::InPlace), false)
        );
        assert!(
            !PaletteQuery("https://example.com")
                .opens_typed_url_on_enter(Some(OpenTarget::InPlace), true)
        );
        assert!(
            !PaletteQuery("> close").opens_typed_url_on_enter(Some(OpenTarget::InPlace), false)
        );
    }
}
