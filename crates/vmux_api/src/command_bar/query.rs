#[derive(Clone, Copy, Debug)]
pub struct CommandBarQuery<'a>(pub &'a str);

impl CommandBarQuery<'_> {
    pub fn opens_typed_url_on_enter(
        &self,
        open_target: Option<crate::open_target::OpenTarget>,
        nav_mode: bool,
    ) -> bool {
        let query = self.0.trim();
        matches!(open_target, Some(crate::open_target::OpenTarget::InPlace))
            && !nav_mode
            && !query.is_empty()
            && !self.0.trim_start().starts_with('>')
            && looks_like_url(query)
    }

    pub fn is_start_prompt(&self) -> bool {
        let query = self.0.trim();
        !query.is_empty()
            && !query.starts_with('>')
            && !looks_like_url(query)
            && !looks_like_explicit_path(query)
    }

    pub fn slash_token(&self) -> Option<(&str, &str)> {
        let rest = self.0.trim_start().strip_prefix('/')?;
        let (name, tail) = match rest.find(char::is_whitespace) {
            Some(at) => (&rest[..at], rest[at..].trim_start()),
            None => (rest, ""),
        };
        if name.is_empty() {
            return None;
        }
        let named = name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        named.then_some((name, tail))
    }

    pub fn mcp_filter(&self) -> Option<&str> {
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

#[vmux_api::ui_event(Default, Eq, targets = ["command-bar", "start", "layout"])]
pub struct PathCompleteRequest {
    pub request_id: u64,
    pub query: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct PathEntry {
    pub name: String,
    pub is_dir: bool,
    pub full_path: String,
    pub project: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct PathCompleteResponse {
    pub request_id: u64,
    pub completions: Vec<PathEntry>,
    pub truncated: bool,
    pub total: u32,
}

pub fn is_data_uri(s: &str) -> bool {
    s.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("data:"))
}

pub fn looks_like_url(s: &str) -> bool {
    let s = s.trim();
    if is_data_uri(s) {
        return true;
    }
    if s.chars().any(char::is_whitespace)
        || s.starts_with('/')
        || s.starts_with("~/")
        || s.starts_with("./")
        || s.starts_with("../")
    {
        return false;
    }
    if s.contains("://") {
        return true;
    }
    let before_slash = s.split('/').next().unwrap_or(s);
    before_slash.contains('.')
}

pub fn looks_like_path(s: &str) -> bool {
    if looks_like_url(s) {
        return false;
    }
    s.starts_with('/')
        || s.starts_with("~/")
        || s.starts_with("./")
        || s.starts_with("../")
        || (s.contains('/') && !s.contains(' '))
}

pub fn looks_like_explicit_path(s: &str) -> bool {
    s.starts_with('/') || s.starts_with('~') || s.starts_with("./") || s.starts_with("../")
}
