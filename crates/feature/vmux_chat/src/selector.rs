#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectorMode<'a> {
    None,
    Commands(&'a str),
    Mcp(&'a str),
    Resume(&'a str),
    Models(&'a str),
}

impl<'a> SelectorMode<'a> {
    pub fn from_draft(draft: &'a str) -> Self {
        let Some(token) = draft.strip_prefix('/') else {
            return Self::None;
        };
        if let Some(rest) = token.strip_prefix("resume")
            && rest.chars().next().is_some_and(char::is_whitespace)
        {
            return Self::Resume(rest.trim_start_matches(char::is_whitespace));
        }
        if let Some(rest) = token.strip_prefix("model")
            && rest.chars().next().is_some_and(char::is_whitespace)
        {
            return Self::Models(rest.trim_start_matches(char::is_whitespace));
        }
        if let Some(rest) = token.strip_prefix("mcp")
            && (rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace))
        {
            return Self::Mcp(rest.trim_start_matches(char::is_whitespace));
        }
        if token.chars().any(char::is_whitespace) {
            Self::None
        } else {
            Self::Commands(token)
        }
    }
}
