/// One downloadable asset for a github-sourced package (mason-registry shape).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Asset {
    pub target: String,
    pub file: String,
    #[serde(default)]
    pub bin: Option<String>,
}

/// The current host's Mason target id (e.g. `darwin_arm64`).
pub fn host_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "darwin_arm64",
        ("macos", "x86_64") => "darwin_x64",
        ("linux", "x86_64") => "linux_x64_gnu",
        ("linux", "aarch64") => "linux_arm64_gnu",
        ("windows", "x86_64") => "win_x64",
        ("windows", "aarch64") => "win_arm64",
        _ => "unsupported",
    }
}

/// Pick the asset matching `target`, with a gnu→musl fallback on linux x64.
pub fn pick_asset<'a>(assets: &'a [Asset], target: &str) -> Option<&'a Asset> {
    if let Some(a) = assets.iter().find(|a| a.target == target) {
        return Some(a);
    }
    if target == "linux_x64_gnu" {
        return assets.iter().find(|a| a.target == "linux_x64_musl");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(target: &str) -> Asset {
        Asset {
            target: target.into(),
            file: format!("file-{target}.gz"),
            bin: Some("bin".into()),
        }
    }

    #[test]
    fn host_target_is_known_on_this_machine() {
        assert_ne!(host_target(), "unsupported");
    }

    #[test]
    fn picks_exact_target() {
        let assets = vec![asset("darwin_arm64"), asset("linux_x64_gnu")];
        assert_eq!(
            pick_asset(&assets, "darwin_arm64").unwrap().target,
            "darwin_arm64"
        );
    }

    #[test]
    fn linux_x64_falls_back_to_musl() {
        let assets = vec![asset("linux_x64_musl"), asset("darwin_arm64")];
        assert_eq!(
            pick_asset(&assets, "linux_x64_gnu").unwrap().target,
            "linux_x64_musl"
        );
    }

    #[test]
    fn no_match_is_none() {
        let assets = vec![asset("win_x64")];
        assert!(pick_asset(&assets, "darwin_arm64").is_none());
    }
}
