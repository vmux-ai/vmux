#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectorMode<'a> {
    None,
    Commands(&'a str),
    Mcp(&'a str),
    Resume(&'a str),
    Models(&'a str),
}

pub fn selector_mode(draft: &str) -> SelectorMode<'_> {
    let Some(token) = draft.strip_prefix('/') else {
        return SelectorMode::None;
    };
    if let Some(rest) = token.strip_prefix("resume")
        && rest.chars().next().is_some_and(char::is_whitespace)
    {
        return SelectorMode::Resume(rest.trim_start_matches(char::is_whitespace));
    }
    if let Some(rest) = token.strip_prefix("model")
        && rest.chars().next().is_some_and(char::is_whitespace)
    {
        return SelectorMode::Models(rest.trim_start_matches(char::is_whitespace));
    }
    if let Some(rest) = token.strip_prefix("mcp")
        && (rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace))
    {
        return SelectorMode::Mcp(rest.trim_start_matches(char::is_whitespace));
    }
    if token.chars().any(char::is_whitespace) {
        SelectorMode::None
    } else {
        SelectorMode::Commands(token)
    }
}
