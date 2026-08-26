use std::path::PathBuf;

pub struct FileUrl<'a>(&'a str);

impl<'a> FileUrl<'a> {
    pub fn parse(url: &'a str) -> Option<Self> {
        let rest = url
            .strip_prefix("file://")
            .or_else(|| url.strip_prefix("FILE://"))?;
        Some(Self(rest))
    }

    pub fn path(&self) -> Option<PathBuf> {
        let rest = match self.0.strip_prefix("localhost") {
            Some(after) if after.is_empty() || after.starts_with('/') => after,
            _ => self.0,
        };
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        if rest.is_empty() {
            return None;
        }
        let raw = match rest.starts_with('/') {
            true => rest.to_string(),
            false => format!("/{rest}"),
        };
        let decoded = percent_encoding::percent_decode_str(&raw)
            .decode_utf8()
            .ok()?;
        let path = PathBuf::from(decoded.as_ref());
        path.is_absolute().then_some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_of(url: &str) -> Option<PathBuf> {
        FileUrl::parse(url)?.path()
    }

    #[test]
    fn a_file_url_names_the_path_after_the_scheme() {
        assert_eq!(
            path_of("file:///Users/me/src/main.rs"),
            Some(PathBuf::from("/Users/me/src/main.rs"))
        );
        assert_eq!(path_of("file:///"), Some(PathBuf::from("/")));
    }

    #[test]
    fn a_file_url_is_percent_decoded_and_stripped_of_query_and_fragment() {
        assert_eq!(
            path_of("file:///Users/me/a%20b.rs"),
            Some(PathBuf::from("/Users/me/a b.rs"))
        );
        assert_eq!(
            path_of("file:///Users/me/a.rs?vmux-raw=1#L3"),
            Some(PathBuf::from("/Users/me/a.rs"))
        );
    }

    #[test]
    fn a_file_url_never_reads_its_first_segment_as_a_host() {
        assert_eq!(
            path_of("file://Users/me/a.rs"),
            Some(PathBuf::from("/Users/me/a.rs"))
        );
        assert_eq!(
            path_of("file://localhost/Users/me/a.rs"),
            Some(PathBuf::from("/Users/me/a.rs"))
        );
        assert_eq!(
            path_of("file://localhost-notes/a.rs"),
            Some(PathBuf::from("/localhost-notes/a.rs"))
        );
    }

    #[test]
    fn another_scheme_is_not_a_file_url() {
        assert!(FileUrl::parse("vmux://terminal/").is_none());
        assert!(FileUrl::parse("https://example.com/a.rs").is_none());
    }
}
