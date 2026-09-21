use std::fmt;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VmuxRoute(url::Url);

impl VmuxRoute {
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
        self.is_host("sessions") || self.is_host("agent")
    }

    pub fn is_terminal(&self) -> bool {
        self.is_host("terminal")
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
        let root_host = match root.host() {
            "agent" => "sessions",
            host => host,
        };
        if candidate.host() != root_host {
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
        match (host.as_str(), root) {
            ("agent", _) => {
                url.set_host(Some("sessions"))
                    .expect("static vmux route host");
            }
            ("agents", true) => {
                url.set_host(Some("tools")).expect("static vmux route host");
                url.set_path("/acp");
            }
            ("tools", true) => url.set_path("/acp"),
            ("lsp", true) => {
                url.set_host(Some("tools")).expect("static vmux route host");
                url.set_path("/lsp");
            }
            ("extensions", true) => {
                url.set_host(Some("tools")).expect("static vmux route host");
                url.set_path("/extensions");
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
            ("vmux://cheatsheet/", "vmux://shortcuts/"),
            ("vmux://cheetsheet", "vmux://shortcuts/"),
            (
                "vmux://agent/codex/cli/session",
                "vmux://sessions/codex/cli/session",
            ),
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
}
