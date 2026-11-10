#[cfg(host)]
pub use host::{
    FileGit, GitCheckForUpdatesRequest, GitDiffSource, GitError, GitPlugin, GitRepository,
    GitReviewProviderCapability, GitReviewProviderRegistry, GitReviewPublished, GitReviewSnapshot,
    GitReviewSubmission, GitReviewSubmissionFinished, GitRuntime, GitTask, RepoInfoCache,
    RepositoryProvisionCompleted, RepositoryProvisionOperation, RepositoryProvisionRequest,
    worktree,
};
#[cfg(host)]
pub use process::GitProcess;
use sha2::Digest;

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod event;
#[cfg(host)]
pub mod process;
pub mod review;
pub mod state;

pub const FILES_HOST: &str = "files";
pub const GIT_PAGE_URL: &str = "git://";
const LEGACY_GIT_PAGE_URL: &str = "vmux://git/";

pub struct GitUrl;

impl GitUrl {
    pub fn file_hash(path: &[u8]) -> String {
        format!("diff-{:x}", sha2::Sha256::digest(path))
    }

    pub fn is_page(value: &str) -> bool {
        value.starts_with(GIT_PAGE_URL) || value.starts_with(LEGACY_GIT_PAGE_URL)
    }

    pub fn parse(value: &str) -> Option<std::path::PathBuf> {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        Self::parse_with_home(value, home.as_deref())
    }

    fn parse_with_home(value: &str, home: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
        let path = if value.starts_with(LEGACY_GIT_PAGE_URL) {
            let parsed = url::Url::parse(value).ok()?;
            let mut segments = parsed.path_segments()?;
            crate::state::GitPanel::from_route(segments.next()?)?;
            let locator = percent_encoding::percent_decode_str(segments.next()?)
                .decode_utf8()
                .ok()?;
            url::Url::parse(&locator).ok()?.to_file_path().ok()?
        } else {
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
            url::Url::parse(&encoded).ok()?.to_file_path().ok()?
        };
        let relative = path.strip_prefix("/~").ok();
        match (home, relative) {
            (Some(home), Some(relative)) => Some(home.join(relative)),
            _ => Some(path),
        }
    }

    pub fn from_path(path: &std::path::Path) -> Option<String> {
        Self::from_view(path, crate::state::GitPanel::Files, None)
    }

    pub fn from_view(
        path: &std::path::Path,
        panel: crate::state::GitPanel,
        selection: Option<&str>,
    ) -> Option<String> {
        let locator = url::Url::from_file_path(path).ok()?;
        let mut page = url::Url::parse(&format!(
            "git://{}",
            locator.as_str().strip_prefix("file://")?
        ))
        .ok()?;
        {
            let mut query = page.query_pairs_mut();
            query.append_pair("view", panel.route());
            if matches!(
                panel,
                crate::state::GitPanel::Commits | crate::state::GitPanel::Stash
            ) && let Some(selection) = selection.filter(|value| !value.is_empty())
            {
                query.append_pair(
                    if panel == crate::state::GitPanel::Commits {
                        "commit"
                    } else {
                        "stash"
                    },
                    selection,
                );
            }
        }
        Some(page.to_string())
    }

    pub fn from_path_with_commit(path: &std::path::Path, commit: Option<&str>) -> Option<String> {
        let panel = if commit.is_some_and(|value| !value.is_empty()) {
            crate::state::GitPanel::Commits
        } else {
            crate::state::GitPanel::Files
        };
        Self::from_view(path, panel, commit)
    }

    pub fn panel(value: &str) -> Option<crate::state::GitPanel> {
        let parsed = url::Url::parse(value).ok()?;
        if parsed.scheme() == "git" {
            for (key, value) in parsed.query_pairs() {
                if key == "view" {
                    return crate::state::GitPanel::from_route(&value);
                }
            }
            return Self::legacy_commit(value).map(|_| crate::state::GitPanel::Commits);
        }
        if parsed.scheme() != "vmux" || parsed.host_str() != Some("git") {
            return Self::legacy_commit(value).map(|_| crate::state::GitPanel::Commits);
        }
        crate::state::GitPanel::from_route(parsed.path_segments()?.next()?)
    }

    pub fn selection(value: &str) -> Option<String> {
        if value.starts_with(LEGACY_GIT_PAGE_URL) {
            let parsed = url::Url::parse(value).ok()?;
            if Self::panel(value) == Some(crate::state::GitPanel::Branches) {
                return None;
            }
            let parts = parsed.path_segments()?.collect::<Vec<_>>();
            let encoded = match parts.as_slice() {
                [_, encoded]
                    if !percent_encoding::percent_decode_str(encoded)
                        .decode_utf8()
                        .ok()?
                        .starts_with("file:") =>
                {
                    encoded
                }
                [_, _, encoded, ..] => encoded,
                _ => return None,
            };
            return percent_encoding::percent_decode_str(encoded)
                .decode_utf8()
                .ok()
                .map(|value| value.into_owned());
        }
        let parsed = url::Url::parse(value).ok()?;
        for (key, value) in parsed.query_pairs() {
            if matches!(key.as_ref(), "commit" | "stash") && !value.is_empty() {
                return Some(value.into_owned());
            }
        }
        None
    }

    pub fn from_path_preserving_commit(
        path: &std::path::Path,
        current_url: &str,
    ) -> Option<String> {
        let selection = if Self::parse(current_url).as_deref() == Some(path) {
            Self::selection(current_url)
        } else {
            None
        };
        Self::from_view(
            path,
            Self::panel(current_url).unwrap_or_default(),
            selection.as_deref(),
        )
    }

    #[cfg(host)]
    pub(crate) fn canonical_legacy(value: &str) -> Option<String> {
        if !value.starts_with(LEGACY_GIT_PAGE_URL) {
            return None;
        }
        let Some(path) = Self::parse(value) else {
            return Some(GIT_PAGE_URL.to_string());
        };
        let url = Self::from_path_preserving_commit(&path, value)?;
        Some(vmux_api::page_location::PageLocationUiState::from_url(value).url(&url))
    }

    pub fn commit(value: &str) -> Option<String> {
        if Self::panel(value) == Some(crate::state::GitPanel::Commits) {
            Self::selection(value)
        } else {
            Self::legacy_commit(value)
        }
    }

    fn legacy_commit(value: &str) -> Option<String> {
        let query = value
            .split_once('?')?
            .1
            .split('#')
            .next()
            .unwrap_or_default();
        url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
            (key == "commit" && !value.is_empty()).then(|| value.into_owned())
        })
    }
}

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_hash_matches_githubs_pull_request_diff_anchor() {
        assert_eq!(
            GitUrl::file_hash(b".github/workflows/ci.yml"),
            "diff-b803fcb7f17ed9235f1e5cb1fcd2f5d3b2838429d4368ae4c57ce4436577f03f"
        );
    }

    #[test]
    fn git_urls_use_the_repository_scheme_without_embedded_file_urls() {
        let path = std::path::Path::new("/Users/me/a repo");
        let url = GitUrl::from_path(path).unwrap();

        assert_eq!(url, "git:///Users/me/a%20repo?view=changes");
        assert_eq!(GitUrl::parse(&url).as_deref(), Some(path));
    }

    #[test]
    fn canonical_git_views_keep_object_selections_and_drop_workspace_locators() {
        let path = std::path::Path::new("/Users/me/a repo");
        for (panel, selection) in [
            (crate::state::GitPanel::Files, None),
            (crate::state::GitPanel::Branches, Some("feature/日本語")),
            (crate::state::GitPanel::Commits, Some("abcdef012345")),
            (crate::state::GitPanel::Stash, Some("stash@{0}")),
        ] {
            let page = GitUrl::from_view(path, panel, selection).unwrap();
            assert_eq!(GitUrl::parse(&page).as_deref(), Some(path));
            assert_eq!(GitUrl::panel(&page), Some(panel));
            assert_eq!(
                GitUrl::selection(&page).as_deref(),
                if panel == crate::state::GitPanel::Branches {
                    None
                } else {
                    selection
                }
            );
        }
        assert_eq!(
            GitUrl::parse("git://Users/me/a%20repo").as_deref(),
            Some(path)
        );
        assert_eq!(
            GitUrl::commit("git://Users/me/a%20repo?commit=abcdef").as_deref(),
            Some("abcdef")
        );
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

    #[test]
    fn git_url_round_trips_a_selected_commit() {
        let path = std::path::Path::new("/Users/me/a repo");
        let url = GitUrl::from_path_with_commit(path, Some("abc/123")).unwrap();

        assert_eq!(
            url,
            "git:///Users/me/a%20repo?view=commits&commit=abc%2F123"
        );
        assert_eq!(GitUrl::parse(&url).as_deref(), Some(path));
        assert_eq!(GitUrl::commit(&url).as_deref(), Some("abc/123"));
        assert_eq!(
            GitUrl::from_path_with_commit(path, None),
            GitUrl::from_path(path)
        );
        assert_eq!(
            GitUrl::from_path_preserving_commit(path, &url),
            Some(url.clone())
        );
        assert_eq!(
            GitUrl::from_path_preserving_commit(std::path::Path::new("/Users/me/other"), &url),
            GitUrl::from_view(
                std::path::Path::new("/Users/me/other"),
                crate::state::GitPanel::Commits,
                None
            )
        );
    }

    #[test]
    fn git_page_identity_excludes_project_hosts() {
        assert!(GitUrl::is_page("git://"));
        assert!(GitUrl::is_page("git://Users/me/repo"));
        assert!(GitUrl::is_page("vmux://git/"));
        assert!(!GitUrl::is_page("vmux://projects/"));
    }

    #[cfg(host)]
    #[test]
    fn legacy_workspace_routes_drop_the_embedded_locator_and_branch() {
        let old =
            "vmux://git/branch-changes/file:%2F%2F%2FUsers%2Fme%2Fa%2520repo/feature%2Fui#diff-123";
        assert_eq!(
            GitUrl::canonical_legacy(old).as_deref(),
            Some("git:///Users/me/a%20repo?view=branch-changes#diff-123")
        );
    }
}
