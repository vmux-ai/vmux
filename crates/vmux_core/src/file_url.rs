//! The `file://` scheme, which is a path wearing a url rather than a url in the usual sense.

use std::path::PathBuf;

/// A url naming a file on this machine.
///
/// Read off the raw string rather than through `Url`, because everything after the scheme is the
/// path here and the parser does not treat it that way. `file://Users/me/a.rs` — two slashes
/// instead of three, the usual typo — parses `Users` as a *host*, so reading `.path()` silently
/// opens `/me/a.rs`: a different file, or more often a missing one blamed on a path nobody
/// typed. A host is also case-folded, so `Users` cannot be put back afterwards. `localhost` is
/// the one host that really does mean this machine, and it is the one that gets dropped.
pub struct FileUrl<'a>(&'a str);

impl<'a> FileUrl<'a> {
    /// `None` unless `url` carries the scheme, so a caller cannot ask a `vmux://` page for a path.
    pub fn parse(url: &'a str) -> Option<Self> {
        let rest = url
            .strip_prefix("file://")
            .or_else(|| url.strip_prefix("FILE://"))?;
        Some(Self(rest))
    }

    /// The absolute path named, with any query, fragment and percent-encoding taken off.
    pub fn path(&self) -> Option<PathBuf> {
        // Only where the whole host is `localhost`. Without the boundary check the two-slash form
        // turns `file://localhost-notes/a.rs` into `/-notes/a.rs`, which is the very failure this
        // type exists to stop.
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

    /// The two-slash typo must keep every segment, and only a whole `localhost` may be dropped.
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
