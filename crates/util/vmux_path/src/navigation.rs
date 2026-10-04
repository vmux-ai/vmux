#[derive(Clone, Copy, Debug)]
pub struct NavigationText<'a>(&'a str);

impl<'a> NavigationText<'a> {
    pub const fn new(value: &'a str) -> Self {
        Self(value)
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
