#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Asset {
    pub target: String,
    pub file: String,
    #[serde(default)]
    pub bin: Option<String>,
}

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
#[path = "target.test.rs"]
mod tests;
