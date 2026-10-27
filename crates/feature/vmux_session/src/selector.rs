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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_mcp_and_other_selector_arguments() {
        assert_eq!(SelectorMode::from_draft("hello"), SelectorMode::None);
        assert_eq!(
            SelectorMode::from_draft("/res"),
            SelectorMode::Commands("res")
        );
        assert_eq!(
            SelectorMode::from_draft("/resume"),
            SelectorMode::Commands("resume")
        );
        assert_eq!(
            SelectorMode::from_draft("/resume "),
            SelectorMode::Resume("")
        );
        assert_eq!(
            SelectorMode::from_draft("/model"),
            SelectorMode::Commands("model")
        );
        assert_eq!(
            SelectorMode::from_draft("/model son"),
            SelectorMode::Models("son")
        );
        assert_eq!(SelectorMode::from_draft("/mcp"), SelectorMode::Mcp(""));
        assert_eq!(
            SelectorMode::from_draft("/mcp lin"),
            SelectorMode::Mcp("lin")
        );
        assert_eq!(
            SelectorMode::from_draft("/resume  SID-9"),
            SelectorMode::Resume("SID-9")
        );
        assert_eq!(SelectorMode::from_draft("/unknown arg"), SelectorMode::None);
    }
}
