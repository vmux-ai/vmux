#[derive(Clone, Copy, Debug)]
pub struct NavigationText<'a>(&'a str);

impl<'a> NavigationText<'a> {
    pub const fn new(value: &'a str) -> Self {
        Self(value)
    }

    pub fn native_root(self) -> Option<&'static str> {
        let root = self
            .0
            .trim()
            .split(['?', '#'])
            .next()?
            .trim_end_matches('/');
        if root.eq_ignore_ascii_case("file:") || root.eq_ignore_ascii_case("vmux://files") {
            return Some("file://");
        }
        if root.eq_ignore_ascii_case("git:") || root.eq_ignore_ascii_case("vmux://git") {
            return Some("git://");
        }
        None
    }

    pub fn is_data_uri(self) -> bool {
        self.0
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
    }

    pub fn looks_like_url(self) -> bool {
        let value = self.0.trim();
        if Self::new(value).is_data_uri() {
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

    pub fn looks_like_path(self) -> bool {
        if self.looks_like_url() {
            return false;
        }
        let value = self.0;
        value.starts_with('/')
            || value.starts_with("~/")
            || value.starts_with("./")
            || value.starts_with("../")
            || (value.contains('/') && !value.contains(' '))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_root_aliases_do_not_rewrite_file_or_repository_locations() {
        for (input, expected) in [
            ("file://", Some("file://")),
            ("file:///", Some("file://")),
            ("vmux://files/", Some("file://")),
            ("git://", Some("git://")),
            ("vmux://git/", Some("git://")),
            ("file:///tmp/README.md", None),
            ("git:///tmp/repo?view=commits", None),
            ("vmux://settings/", None),
        ] {
            assert_eq!(NavigationText::new(input).native_root(), expected);
        }
    }
}
