use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostSpawnRoute {
    Host(&'static str),
    Scheme(&'static str),
}

impl HostSpawnRoute {
    pub const fn host(host: &'static str) -> Self {
        Self::Host(host)
    }

    pub const fn scheme(scheme: &'static str) -> Self {
        Self::Scheme(scheme)
    }

    pub fn answers_for(&self, url: &str) -> bool {
        match self {
            Self::Host(host) => {
                vmux_api::VmuxRoute::parse(url).is_some_and(|route| route.host() == *host)
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
    fn host_route_matches_on_boundary() {
        let route = HostSpawnRoute::host("terminal");
        assert!(route.answers_for("vmux://terminal/"));
        assert!(route.answers_for("vmux://terminal/?pid=1"));
        assert!(!route.answers_for("vmux://terminals/"));
    }

    #[test]
    fn scheme_route_matches_case_insensitively() {
        let route = HostSpawnRoute::scheme("git");
        assert!(route.answers_for("git://Users/me/repo"));
        assert!(route.answers_for("GIT://Users/me/repo"));
        assert!(!route.answers_for("https://example.com"));
    }
}
