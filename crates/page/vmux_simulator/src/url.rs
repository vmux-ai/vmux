use std::cmp::Ordering;

use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

pub const PAGE_HOST: &str = "simulator";
pub const PAGE_URL: &str = "vmux://simulator/";
pub const PLATFORM: &str = "ios";

pub const UNPINNED_URL: &str = "vmux://simulator/ios";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IosVersion(String);

impl IosVersion {
    pub fn from_runtime_key(key: &str) -> Option<Self> {
        let suffix = key.rsplit_once(".SimRuntime.")?.1;
        let digits = suffix.strip_prefix("iOS-")?;
        Self::parse(&digits.replace('-', "."))
    }

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

    fn numbers(&self) -> impl Iterator<Item = u32> + '_ {
        self.0.split('.').map(|part| part.parse().unwrap_or(0))
    }
}

impl Ord for IosVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.numbers()
            .cmp(other.numbers())
            .then_with(|| self.0.cmp(&other.0))
    }
}

impl PartialOrd for IosVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for IosVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulatorRoute {
    Unpinned,
    Pinned {
        version: IosVersion,
        device_name: Option<String>,
    },
}

impl SimulatorRoute {
    pub fn parse(pathname: &str) -> Option<Self> {
        let mut segments = pathname.split('/').filter(|s| !s.is_empty());
        if !segments.next()?.eq_ignore_ascii_case(PLATFORM) {
            return None;
        }
        let Some(version) = segments.next() else {
            return Some(Self::Unpinned);
        };
        let version = IosVersion::parse(version)?;
        let device_name = match segments.next() {
            Some(segment) => {
                let decoded = percent_decode_str(segment).decode_utf8().ok()?.into_owned();
                if decoded.is_empty() {
                    return None;
                }
                Some(decoded)
            }
            None => None,
        };
        if segments.next().is_some() {
            return None;
        }
        Some(Self::Pinned {
            version,
            device_name,
        })
    }

    fn parse_url(url: &str) -> Option<Self> {
        let rest = url.strip_prefix("vmux://")?;
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        if host != PAGE_HOST {
            return None;
        }
        let path = path.split(['?', '#']).next().unwrap_or(path);
        if path.is_empty() {
            return Some(Self::Unpinned);
        }
        Self::parse(path)
    }

    pub fn version(&self) -> Option<&IosVersion> {
        match self {
            Self::Unpinned => None,
            Self::Pinned { version, .. } => Some(version),
        }
    }

    pub fn device_name(&self) -> Option<&str> {
        match self {
            Self::Unpinned => None,
            Self::Pinned { device_name, .. } => device_name.as_deref(),
        }
    }

    pub fn path(version: &IosVersion, device_name: Option<&str>) -> String {
        let mut path = format!("/{PLATFORM}/{version}");
        if let Some(device_name) = device_name {
            path.push('/');
            path.push_str(&utf8_percent_encode(device_name, NON_ALPHANUMERIC).to_string());
        }
        path
    }

    pub fn url(version: &IosVersion, device_name: Option<&str>) -> String {
        format!("vmux://{PAGE_HOST}{}", Self::path(version, device_name))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSimulatorRoute;

impl std::fmt::Display for InvalidSimulatorRoute {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid simulator route")
    }
}

impl std::error::Error for InvalidSimulatorRoute {}

impl TryFrom<&str> for SimulatorRoute {
    type Error = InvalidSimulatorRoute;

    fn try_from(url: &str) -> Result<Self, Self::Error> {
        Self::parse_url(url).ok_or(InvalidSimulatorRoute)
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
        assert_eq!(
            SimulatorRoute::parse("/iOS"),
            Some(SimulatorRoute::Unpinned)
        );
    }

    #[test]
    fn a_version_segment_pins_the_runtime() {
        let route = SimulatorRoute::parse("/ios/27.0").expect("route");

        assert_eq!(route.version().map(IosVersion::as_str), Some("27.0"));
        assert_eq!(route.device_name(), None);
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
        assert_eq!(SimulatorRoute::parse("/ios/27.0/iPhone/extra"), None);
        assert_eq!(SimulatorRoute::parse("/ios/27.x"), None);
    }

    #[test]
    fn whole_urls_route_here_only_for_this_host() {
        assert_eq!(
            SimulatorRoute::try_from(UNPINNED_URL),
            Ok(SimulatorRoute::Unpinned)
        );
        assert_eq!(
            SimulatorRoute::try_from("vmux://simulator/ios/27.0")
                .ok()
                .and_then(|r| r.version().map(IosVersion::as_str).map(str::to_string)),
            Some("27.0".to_string())
        );
        assert!(SimulatorRoute::try_from("vmux://simulator/ios/27.0?x=1").is_ok());
    }

    #[test]
    fn bare_simulator_urls_are_unpinned() {
        assert_eq!(
            SimulatorRoute::try_from("vmux://simulator/"),
            Ok(SimulatorRoute::Unpinned)
        );
        assert_eq!(
            SimulatorRoute::try_from("vmux://simulator"),
            Ok(SimulatorRoute::Unpinned)
        );
    }

    #[test]
    fn another_host_does_not_route_here() {
        assert!(SimulatorRoute::try_from("vmux://terminal/ios").is_err());
        assert!(SimulatorRoute::try_from("vmux://simulator/android/15").is_err());
        assert!(SimulatorRoute::try_from("https://simulator/ios").is_err());
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
    fn versions_sort_numerically() {
        let older = IosVersion::parse("9.5").expect("version");
        let newer = IosVersion::parse("10.0").expect("version");

        assert!(newer > older);
    }

    #[test]
    fn a_parsed_version_round_trips_through_its_url() {
        let version = IosVersion::from_runtime_key("com.apple.CoreSimulator.SimRuntime.iOS-27-0")
            .expect("version");

        let path = SimulatorRoute::path(&version, None);
        let route = SimulatorRoute::parse(&path).expect("route");

        assert_eq!(
            route,
            SimulatorRoute::Pinned {
                version: version.clone(),
                device_name: None,
            }
        );
        assert_eq!(
            SimulatorRoute::url(&version, None),
            "vmux://simulator/ios/27.0"
        );
    }

    #[test]
    fn a_device_name_round_trips_through_its_url() {
        let version = IosVersion::parse("27.0").expect("version");
        let url = SimulatorRoute::url(&version, Some("iPhone 17 Pro"));

        let route = SimulatorRoute::try_from(url.as_str()).expect("route");

        assert_eq!(url, "vmux://simulator/ios/27.0/iPhone%2017%20Pro");
        assert_eq!(route.device_name(), Some("iPhone 17 Pro"));
    }
}
