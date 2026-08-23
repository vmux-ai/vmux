//! The page's URL space, shared by the host and the view.
//!
//! `vmux://simulator/ios` names "whichever iOS simulator is booted" and is canonicalised to
//! `vmux://simulator/ios/<version>` so a URL identifies one runtime rather than drifting with
//! whatever happens to be running.

pub const PAGE_HOST: &str = "simulator";
pub const PLATFORM: &str = "ios";

/// An iOS runtime version, as it appears in a URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IosVersion(String);

impl IosVersion {
    /// From a simctl runtime key such as `com.apple.CoreSimulator.SimRuntime.iOS-27-0`.
    pub fn from_runtime_key(key: &str) -> Option<Self> {
        let suffix = key.rsplit_once(".SimRuntime.")?.1;
        let digits = suffix.strip_prefix("iOS-")?;
        if digits.is_empty() || !digits.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        Some(Self(digits.replace('-', ".")))
    }

    /// From a URL segment, rejecting anything that is not a dotted version.
    pub fn parse(segment: &str) -> Option<Self> {
        if segment.is_empty() {
            return None;
        }
        let valid = segment
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
        valid.then(|| Self(segment.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for IosVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a `vmux://simulator/...` path names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulatorRoute {
    /// `/ios` — no runtime chosen yet; the view redirects once the host resolves one.
    Unpinned,
    /// `/ios/<version>` — one specific runtime.
    Pinned(IosVersion),
}

impl SimulatorRoute {
    pub fn parse(pathname: &str) -> Option<Self> {
        let mut segments = pathname.split('/').filter(|s| !s.is_empty());
        if segments.next()? != PLATFORM {
            return None;
        }
        let Some(version) = segments.next() else {
            return Some(Self::Unpinned);
        };
        if segments.next().is_some() {
            return None;
        }
        IosVersion::parse(version).map(Self::Pinned)
    }

    pub fn version(&self) -> Option<&IosVersion> {
        match self {
            Self::Unpinned => None,
            Self::Pinned(version) => Some(version),
        }
    }

    pub fn path(version: &IosVersion) -> String {
        format!("/{PLATFORM}/{version}")
    }

    pub fn url(version: &IosVersion) -> String {
        format!("vmux://{PAGE_HOST}/{PLATFORM}/{version}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_platform_path_is_unpinned() {
        assert_eq!(
            SimulatorRoute::parse("/ios"),
            Some(SimulatorRoute::Unpinned)
        );
        assert_eq!(
            SimulatorRoute::parse("/ios/"),
            Some(SimulatorRoute::Unpinned)
        );
    }

    #[test]
    fn a_version_segment_pins_the_runtime() {
        let route = SimulatorRoute::parse("/ios/27.0").expect("route");

        assert_eq!(route.version().map(IosVersion::as_str), Some("27.0"));
    }

    #[test]
    fn paths_outside_the_platform_do_not_route_here() {
        assert_eq!(SimulatorRoute::parse("/android/15"), None);
        assert_eq!(SimulatorRoute::parse("/"), None);
        assert_eq!(SimulatorRoute::parse(""), None);
    }

    #[test]
    fn a_non_version_or_extra_segment_is_rejected_rather_than_guessed() {
        assert_eq!(SimulatorRoute::parse("/ios/latest"), None);
        assert_eq!(SimulatorRoute::parse("/ios/27.0/extra"), None);
        assert_eq!(SimulatorRoute::parse("/ios/27.x"), None);
    }

    #[test]
    fn runtime_keys_become_dotted_versions() {
        let key = "com.apple.CoreSimulator.SimRuntime.iOS-27-0";

        assert_eq!(
            IosVersion::from_runtime_key(key).map(|v| v.to_string()),
            Some("27.0".to_string())
        );
        assert_eq!(
            IosVersion::from_runtime_key("com.apple.CoreSimulator.SimRuntime.iOS-26-5")
                .map(|v| v.to_string()),
            Some("26.5".to_string())
        );
    }

    #[test]
    fn non_ios_runtimes_are_not_this_page() {
        assert_eq!(
            IosVersion::from_runtime_key("com.apple.CoreSimulator.SimRuntime.watchOS-26-5"),
            None
        );
        assert_eq!(IosVersion::from_runtime_key("nonsense"), None);
    }

    #[test]
    fn a_parsed_version_round_trips_through_its_url() {
        let version = IosVersion::from_runtime_key("com.apple.CoreSimulator.SimRuntime.iOS-27-0")
            .expect("version");

        let path = SimulatorRoute::path(&version);
        let route = SimulatorRoute::parse(&path).expect("route");

        assert_eq!(route, SimulatorRoute::Pinned(version.clone()));
        assert_eq!(SimulatorRoute::url(&version), "vmux://simulator/ios/27.0");
    }
}
