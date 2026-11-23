use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostSpawnRoute {
    kind: HostSpawnRouteKind,
    target: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostSpawnRouteKind {
    Page,
    Subtree,
    Scheme,
}

impl HostSpawnRoute {
    pub const fn page(url: &'static str) -> Self {
        Self {
            kind: HostSpawnRouteKind::Page,
            target: url,
        }
    }

    pub const fn subtree(url: &'static str) -> Self {
        Self {
            kind: HostSpawnRouteKind::Subtree,
            target: url,
        }
    }

    pub const fn scheme(scheme: &'static str) -> Self {
        Self {
            kind: HostSpawnRouteKind::Scheme,
            target: scheme,
        }
    }

    pub fn answers_for(&self, url: &str) -> bool {
        match self.kind {
            HostSpawnRouteKind::Page | HostSpawnRouteKind::Subtree => {
                let (Some(page), Some(candidate)) = (
                    vmux_api::VmuxRoute::parse(self.target),
                    vmux_api::VmuxRoute::parse(url),
                ) else {
                    return false;
                };
                match self.kind {
                    HostSpawnRouteKind::Subtree => candidate.in_subtree(&page),
                    HostSpawnRouteKind::Page => candidate.same_page(&page),
                    HostSpawnRouteKind::Scheme => false,
                }
            }
            HostSpawnRouteKind::Scheme => url
                .split_once(':')
                .is_some_and(|(candidate, _)| candidate.eq_ignore_ascii_case(self.target)),
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
    fn subtree_route_matches_only_the_canonical_host() {
        let route = HostSpawnRoute::subtree("vmux://sessions/");
        assert!(route.answers_for("vmux://sessions/session-1"));
        assert!(!route.answers_for("vmux://agent/codex/cli"));
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
