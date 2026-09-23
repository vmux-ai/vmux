pub mod event;
pub mod view;

pub const FILES_HOST: &str = "files";
pub const GIT_PAGE_URL: &str = "git://";
pub const GIT_DOCUMENT_URL: &str = "vmux://git/";

pub struct GitUrl;

impl GitUrl {
    pub fn parse(value: &str) -> Option<std::path::PathBuf> {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        Self::parse_with_home(value, home.as_deref())
    }

    fn parse_with_home(value: &str, home: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
        let rest = value
            .strip_prefix(GIT_PAGE_URL)
            .or_else(|| value.strip_prefix("GIT://"))?;
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        if rest.is_empty() {
            return None;
        }
        let encoded = if rest.starts_with('/') {
            format!("file://{rest}")
        } else {
            format!("file:///{rest}")
        };
        let path = url::Url::parse(&encoded).ok()?.to_file_path().ok()?;
        let relative = path.strip_prefix("/~").ok();
        match (home, relative) {
            (Some(home), Some(relative)) => Some(home.join(relative)),
            _ => Some(path),
        }
    }

    pub fn from_path(path: &std::path::Path) -> Option<String> {
        let file_url = url::Url::from_file_path(path).ok()?;
        let rest = file_url.as_str().strip_prefix("file:///")?;
        Some(format!("{GIT_PAGE_URL}{rest}"))
    }
}

#[cfg(ui)]
pub mod page;
#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_url_round_trips_an_absolute_repository_path() {
        let path = std::path::Path::new("/Users/me/a repo");
        let url = GitUrl::from_path(path).unwrap();

        assert_eq!(url, "git://Users/me/a%20repo");
        assert_eq!(GitUrl::parse(&url), Some(path.to_path_buf()));
    }

    #[test]
    fn git_url_accepts_the_standard_triple_slash_form() {
        assert_eq!(
            GitUrl::parse("git:///Users/me/repo"),
            Some(std::path::PathBuf::from("/Users/me/repo"))
        );
    }

    #[test]
    fn git_url_expands_a_home_relative_path() {
        assert_eq!(
            GitUrl::parse_with_home(
                "git://~/.vmux/projects/github.com/vmux-ai/vmux-cloud",
                Some(std::path::Path::new("/Users/me")),
            ),
            Some(std::path::PathBuf::from(
                "/Users/me/.vmux/projects/github.com/vmux-ai/vmux-cloud"
            ))
        );
    }
}
