use crate::lsp::package_path::{PackagePath, Sha256Digest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub target: String,
    pub file: PackagePath,
    pub bin: Option<PackagePath>,
    pub sha256: Option<Sha256Digest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformTarget(&'static str);

impl PlatformTarget {
    pub fn current() -> Self {
        Self(match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "darwin_arm64",
            ("macos", "x86_64") => "darwin_x64",
            ("linux", "x86_64") => "linux_x64_gnu",
            ("linux", "aarch64") => "linux_arm64_gnu",
            ("windows", "x86_64") => "win_x64",
            ("windows", "aarch64") => "win_arm64",
            _ => "unsupported",
        })
    }

    pub fn as_str(self) -> &'static str {
        self.0
    }

    pub fn select(self, assets: &[Asset]) -> Option<&Asset> {
        if let Some(asset) = assets.iter().find(|asset| asset.target == self.0) {
            return Some(asset);
        }
        if self.0 == "linux_x64_gnu" {
            return assets.iter().find(|asset| asset.target == "linux_x64_musl");
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(target: &str) -> Asset {
        Asset {
            target: target.into(),
            file: PackagePath::parse(&format!("file-{target}.gz")).unwrap(),
            bin: Some(PackagePath::parse("bin").unwrap()),
            sha256: None,
        }
    }

    #[test]
    fn host_target_is_known_on_this_machine() {
        assert_ne!(PlatformTarget::current().as_str(), "unsupported");
    }

    #[test]
    fn picks_exact_target() {
        let assets = vec![asset("darwin_arm64"), asset("linux_x64_gnu")];
        assert_eq!(
            PlatformTarget("darwin_arm64")
                .select(&assets)
                .unwrap()
                .target,
            "darwin_arm64"
        );
    }

    #[test]
    fn linux_x64_falls_back_to_musl() {
        let assets = vec![asset("linux_x64_musl"), asset("darwin_arm64")];
        assert_eq!(
            PlatformTarget("linux_x64_gnu")
                .select(&assets)
                .unwrap()
                .target,
            "linux_x64_musl"
        );
    }

    #[test]
    fn no_match_is_none() {
        let assets = vec![asset("win_x64")];
        assert!(PlatformTarget("darwin_arm64").select(&assets).is_none());
    }
}
