use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostSpawnRoute {
    Page {
        url: &'static str,
        owns_subtree: bool,
    },
    Scheme(&'static str),
}

impl HostSpawnRoute {
    pub const fn page(url: &'static str) -> Self {
        Self::Page {
            url,
            owns_subtree: false,
        }
    }

    pub const fn subtree(url: &'static str) -> Self {
        Self::Page {
            url,
            owns_subtree: true,
        }
    }

    pub const fn scheme(scheme: &'static str) -> Self {
        Self::Scheme(scheme)
    }

    pub fn answers_for(&self, url: &str) -> bool {
        match self {
            Self::Page {
                url: page_url,
                owns_subtree,
            } => {
                let (Some(page), Some(candidate)) = (
                    vmux_api::VmuxRoute::parse(page_url),
                    vmux_api::VmuxRoute::parse(url),
                ) else {
                    return false;
                };
                match owns_subtree {
                    true => candidate.in_subtree(&page),
                    false => candidate.same_page(&page),
                }
            }
            Self::Scheme(scheme) => url
                .split_once(':')
                .is_some_and(|(candidate, _)| candidate.eq_ignore_ascii_case(scheme)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_route_matches_on_boundary() {
        let route = HostSpawnRoute::page("vmux://terminal/");
        assert!(route.answers_for("vmux://terminal/"));
        assert!(route.answers_for("vmux://terminal/?pid=1"));
        assert!(!route.answers_for("vmux://terminals/"));
    }

    #[test]
    fn subtree_route_canonicalizes_legacy_aliases() {
        let route = HostSpawnRoute::subtree("vmux://sessions/");
        assert!(route.answers_for("vmux://sessions/codex/cli"));
        assert!(route.answers_for("vmux://agent/codex/cli"));
        assert!(!route.answers_for("vmux://start/"));
    }

    #[test]
    fn scheme_route_matches_case_insensitively() {
        let route = HostSpawnRoute::scheme("git");
        assert!(route.answers_for("git://Users/me/repo"));
        assert!(route.answers_for("GIT://Users/me/repo"));
        assert!(!route.answers_for("https://example.com"));
    }
}
