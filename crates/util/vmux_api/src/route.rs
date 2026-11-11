use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoutePattern<'a> {
    url: &'a str,
    owns_subtree: bool,
}

impl<'a> RoutePattern<'a> {
    pub const fn new(url: &'a str, owns_subtree: bool) -> Self {
        Self { url, owns_subtree }
    }

    pub fn matches(self, candidate: &str) -> bool {
        self.specificity(candidate).is_some()
    }

    pub fn specificity(self, candidate: &str) -> Option<(bool, usize)> {
        if let Some(scheme) = self.url.strip_suffix("://") {
            let matches = candidate
                .split_once(':')
                .is_some_and(|(candidate, _)| candidate.eq_ignore_ascii_case(scheme));
            return matches.then_some((true, self.url.len()));
        }
        if let (Some(base), Some(candidate)) =
            (VmuxRoute::parse(self.url), VmuxRoute::parse(candidate))
        {
            let exact = candidate.same_page(&base);
            let matches = exact || (self.owns_subtree && candidate.in_subtree(&base));
            return matches.then_some((exact, self.url.len()));
        }
        let (Ok(base), Ok(candidate)) = (url::Url::parse(self.url), url::Url::parse(candidate))
        else {
            return None;
        };
        if base.scheme() != candidate.scheme() || base.host_str() != candidate.host_str() {
            return None;
        }
        let base_path = base.path().trim_end_matches('/');
        let candidate_path = candidate.path().trim_end_matches('/');
        let exact = candidate_path == base_path;
        let matches = exact
            || (self.owns_subtree
                && candidate_path
                    .strip_prefix(base_path)
                    .is_some_and(|suffix| suffix.starts_with('/')));
        matches.then_some((exact, self.url.len()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VmuxRoute(url::Url);

impl VmuxRoute {
    pub const SESSIONS_ROOT: &str = "vmux://sessions/";

    pub fn parse(value: &str) -> Option<Self> {
        Self::try_from(value).ok()
    }

    pub fn canonical(value: &str) -> Option<String> {
        Self::parse(value).map(|route| route.canonicalized().to_string())
    }

    pub fn host(&self) -> &str {
        self.0.host_str().expect("validated vmux route host")
    }

    pub fn path(&self) -> &str {
        self.0.path()
    }

    pub fn query(&self) -> Option<&str> {
        self.0.query()
    }

    pub fn fragment(&self) -> Option<&str> {
        self.0.fragment()
    }

    pub fn is_host(&self, host: &str) -> bool {
        self.host().eq_ignore_ascii_case(host)
    }

    pub fn is_root(&self) -> bool {
        self.path() == "/"
    }

    pub fn is_agent(&self) -> bool {
        self.is_host("sessions")
    }

    pub fn is_terminal(&self) -> bool {
        self.is_host("terminal")
    }

    pub fn agent_id(&self) -> Option<&str> {
        if !self.is_agent() {
            return None;
        }
        self.path_segments().next()
    }

    pub fn supports_inline_transition(&self) -> bool {
        if !self.is_agent() {
            return false;
        }
        let segments = self.path_segments().collect::<Vec<_>>();
        matches!(segments.as_slice(), [] | [_])
    }

    pub fn path_segments(&self) -> impl Iterator<Item = &str> {
        self.path().split('/').filter(|segment| !segment.is_empty())
    }

    pub fn same_page(&self, other: &Self) -> bool {
        let left = self.clone().canonicalized();
        let right = other.clone().canonicalized();
        left.host() == right.host()
            && left.path().trim_end_matches('/') == right.path().trim_end_matches('/')
    }

    pub fn in_subtree(&self, root: &Self) -> bool {
        let candidate = self.clone().canonicalized();
        if candidate.host() != root.host() {
            return false;
        }
        let root_path = root.path().trim_end_matches('/');
        let path = candidate.path().trim_end_matches('/');
        path == root_path
            || path
                .strip_prefix(root_path)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    pub fn canonicalized(&self) -> Self {
        let mut url = self.0.clone();
        let root = url.path() == "/";
        let host = url.host_str().unwrap_or_default().to_string();
        if host == "extensions" && root {
            url.set_host(Some("tools")).expect("static vmux route host");
            url.set_path("/extensions");
            return Self(url);
        }
        match (host.as_str(), root) {
            ("containers", true) => {
                url.set_host(Some("docker"))
                    .expect("static vmux route host");
                url.set_path("/containers");
            }
            ("docker", true) => url.set_path("/containers"),
            ("agents", true) => {
                url.set_host(Some("tools")).expect("static vmux route host");
                url.set_path("/acp");
            }
            ("tools", true) => url.set_path("/acp"),
            ("lsp", true) => {
                url.set_host(Some("tools")).expect("static vmux route host");
                url.set_path("/lsp");
            }
            ("cheatsheet" | "cheetsheet", true) => {
                url.set_host(Some("shortcuts"))
                    .expect("static vmux route host");
                url.set_path("/");
            }
            _ => {}
        }
        Self(url)
    }

    fn normalize(mut url: url::Url) -> Self {
        if url.path().is_empty() {
            url.set_path("/");
        }
        Self(url)
    }
}

impl fmt::Display for VmuxRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

impl FromStr for VmuxRoute {
    type Err = InvalidVmuxRoute;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value)
    }
}

impl TryFrom<&str> for VmuxRoute {
    type Error = InvalidVmuxRoute;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let url = url::Url::parse(value.trim()).map_err(|_| InvalidVmuxRoute::InvalidUrl)?;
        if url.scheme() != "vmux" {
            return Err(InvalidVmuxRoute::WrongScheme);
        }
        if url.host_str().is_none() {
            return Err(InvalidVmuxRoute::MissingHost);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(InvalidVmuxRoute::Credentials);
        }
        if url.port().is_some() {
            return Err(InvalidVmuxRoute::Port);
        }
        Ok(Self::normalize(url))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidVmuxRoute {
    InvalidUrl,
    WrongScheme,
    MissingHost,
    Credentials,
    Port,
}

impl fmt::Display for InvalidVmuxRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidUrl => "invalid URL",
            Self::WrongScheme => "route scheme is not vmux",
            Self::MissingHost => "vmux route has no host",
            Self::Credentials => "vmux route cannot contain credentials",
            Self::Port => "vmux route cannot contain a port",
        })
    }
}

impl std::error::Error for InvalidVmuxRoute {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_root_routes_and_legacy_aliases() {
        let cases = [
            (" vmux://terminal ", "vmux://terminal/"),
            ("vmux://tools/", "vmux://tools/acp"),
            ("vmux://agents", "vmux://tools/acp"),
            ("vmux://lsp/", "vmux://tools/lsp"),
            ("vmux://extensions", "vmux://tools/extensions"),
            ("vmux://tools/extensions", "vmux://tools/extensions"),
            ("vmux://cheatsheet/", "vmux://shortcuts/"),
            ("vmux://cheetsheet", "vmux://shortcuts/"),
            ("vmux://containers/", "vmux://docker/containers"),
            ("vmux://docker", "vmux://docker/containers"),
            ("vmux://docker/containers", "vmux://docker/containers"),
        ];
        for (input, expected) in cases {
            assert_eq!(VmuxRoute::canonical(input).as_deref(), Some(expected));
        }
    }

    #[test]
    fn canonicalization_preserves_query_and_fragment() {
        assert_eq!(
            VmuxRoute::canonical("vmux://lsp/?source=menu#catalog").as_deref(),
            Some("vmux://tools/lsp?source=menu#catalog")
        );
        assert_eq!(
            VmuxRoute::canonical("vmux://containers/?source=bookmark#logs").as_deref(),
            Some("vmux://docker/containers?source=bookmark#logs")
        );
    }

    #[test]
    fn agent_id_reads_session_routes_only() {
        let route = VmuxRoute::parse("vmux://sessions/codex/cli").unwrap();
        assert_eq!(route.agent_id(), Some("codex"));
        assert_eq!(
            VmuxRoute::parse("vmux://agent/codex/cli")
                .unwrap()
                .agent_id(),
            None
        );
        assert_eq!(
            VmuxRoute::parse("vmux://terminal/").unwrap().agent_id(),
            None
        );
    }

    #[test]
    fn rejects_non_vmux_and_ambiguous_authorities() {
        for input in [
            "https://terminal/",
            "vmux:///terminal",
            "vmux://user@terminal/",
            "vmux://terminal:42/",
        ] {
            assert!(VmuxRoute::try_from(input).is_err(), "accepted {input}");
        }
    }

    #[test]
    fn canonical_routes_round_trip() {
        for input in [
            "vmux://start/",
            "vmux://sessions/codex/cli/session",
            "vmux://error/?title=Not%20Found#details",
        ] {
            let route = VmuxRoute::try_from(input).expect("route");
            let reparsed = VmuxRoute::try_from(route.to_string().as_str()).expect("round trip");
            assert_eq!(reparsed, route);
        }
    }

    #[test]
    fn page_and_subtree_matching_use_route_boundaries() {
        let root = VmuxRoute::try_from("vmux://tools/").expect("root");
        let child = VmuxRoute::try_from("vmux://tools/acp/server").expect("child");
        let sibling = VmuxRoute::try_from("vmux://toolbox/acp").expect("sibling");
        assert!(root.same_page(&VmuxRoute::try_from("vmux://tools/acp").expect("canonical root")));
        assert!(child.in_subtree(&root));
        assert!(!sibling.in_subtree(&root));
    }

    #[test]
    fn route_patterns_ignore_query_fragment_and_trailing_slash() {
        let exact = RoutePattern::new("vmux://projects/", false);
        assert!(exact.matches("vmux://projects?view=grid#active"));
        assert!(!exact.matches("vmux://projects/one"));

        let containers = RoutePattern::new("vmux://docker/containers", false);
        assert!(containers.matches("vmux://containers/?source=bookmark"));
        assert!(containers.matches("vmux://docker/"));
        assert!(!containers.matches("vmux://docker/images"));
        assert!(!containers.matches("vmux://kubernetes/pods"));

        let subtree = RoutePattern::new("https://example.com/tools/", true);
        assert!(subtree.matches("https://example.com/tools/git?branch=main"));
        assert!(!subtree.matches("https://example.com/toolsmith"));
    }

    #[test]
    fn inline_transition_accepts_only_page_agent_routes() {
        for url in ["vmux://sessions/", "vmux://sessions/session-1"] {
            assert!(
                VmuxRoute::try_from(url)
                    .expect("route")
                    .supports_inline_transition()
            );
        }
        for url in [
            "vmux://sessions/provider/session",
            "vmux://agent/codex",
            "vmux://terminal/",
        ] {
            assert!(
                !VmuxRoute::try_from(url)
                    .expect("route")
                    .supports_inline_transition()
            );
        }
    }
}
