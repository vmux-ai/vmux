use vmux_api::open_target::OpenTarget;
use vmux_core::input::NavigationText;

#[derive(Clone, Copy, Debug)]
pub struct PaletteQuery<'a>(&'a str);

impl<'a> PaletteQuery<'a> {
    pub const fn new(value: &'a str) -> Self {
        Self(value)
    }

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
            && Self::new(query).looks_like_url()
    }

    pub(crate) fn is_start_prompt(&self) -> bool {
        let query = self.0.trim();
        !query.is_empty()
            && !query.starts_with('>')
            && !Self::new(query).looks_like_url()
            && !Self::new(query).looks_like_explicit_path()
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

    #[cfg(any(host, test))]
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

    pub fn is_data_uri(&self) -> bool {
        NavigationText::new(self.0).is_data_uri()
    }

    pub fn looks_like_url(&self) -> bool {
        NavigationText::new(self.0).looks_like_url()
    }

    pub fn looks_like_path(&self) -> bool {
        NavigationText::new(self.0).looks_like_path()
    }

    fn looks_like_explicit_path(&self) -> bool {
        self.0.starts_with('/')
            || self.0.starts_with('~')
            || self.0.starts_with("./")
            || self.0.starts_with("../")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_token_is_a_bare_name_and_never_a_path() {
        assert_eq!(
            PaletteQuery::new("/resume").slash_token(),
            Some(("resume", ""))
        );
        assert_eq!(
            PaletteQuery::new("/resume yesterday").slash_token(),
            Some(("resume", "yesterday"))
        );
        assert_eq!(
            PaletteQuery::new("  /model").slash_token(),
            Some(("model", ""))
        );
        assert_eq!(PaletteQuery::new("/Users/jun/notes.md").slash_token(), None);
        assert_eq!(PaletteQuery::new("/").slash_token(), None);
        assert_eq!(PaletteQuery::new("resume").slash_token(), None);
    }

    #[test]
    fn mcp_filter_requires_the_complete_command() {
        assert_eq!(PaletteQuery::new("/mcp").mcp_filter(), Some(""));
        assert_eq!(
            PaletteQuery::new("/mcp linear").mcp_filter(),
            Some("linear")
        );
        assert_eq!(PaletteQuery::new("/mcpx").mcp_filter(), None);
    }

    #[test]
    fn url_and_path_classification_is_unambiguous() {
        assert!(PaletteQuery::new("https://example.com/path").looks_like_url());
        assert!(PaletteQuery::new("google.com/maps").looks_like_url());
        assert!(PaletteQuery::new("data:text/html,<h1>hi</h1>").looks_like_url());
        assert!(!PaletteQuery::new("src/main.rs").looks_like_url());
        assert!(!PaletteQuery::new("search query").looks_like_url());
        assert!(PaletteQuery::new("/usr/bin").looks_like_path());
        assert!(PaletteQuery::new("~/projects").looks_like_path());
        assert!(PaletteQuery::new("src/main.rs").looks_like_path());
        assert!(!PaletteQuery::new("https://example.com/path").looks_like_path());
        assert!(!PaletteQuery::new("some query / thing").looks_like_path());
    }

    #[test]
    fn start_prompt_rejects_navigation_and_commands() {
        assert!(PaletteQuery::new("fix the failing test").is_start_prompt());
        assert!(PaletteQuery::new("codex").is_start_prompt());
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
            assert!(!PaletteQuery::new(query).is_start_prompt(), "{query}");
        }
    }

    #[test]
    fn typed_url_open_requires_in_place_without_navigation() {
        assert!(
            PaletteQuery::new("https://example.com")
                .opens_typed_url_on_enter(Some(OpenTarget::InPlace), false)
        );
        assert!(
            !PaletteQuery::new("https://example.com")
                .opens_typed_url_on_enter(Some(OpenTarget::InPlace), true)
        );
        assert!(
            !PaletteQuery::new("> close")
                .opens_typed_url_on_enter(Some(OpenTarget::InPlace), false)
        );
    }
}
