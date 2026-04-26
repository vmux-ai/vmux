use semver::Version;
use serde::Deserialize;

const APP_TARBALL_PATTERN: &str = "aarch64-apple-darwin.app.tar.gz";
const SHA256_SUFFIX: &str = ".sha256";

#[derive(Debug)]
pub struct ReleaseInfo {
    pub version: Version,
    pub tarball_url: String,
    pub sha256_url: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Fetch the latest release from GitHub and return info if it's newer than `current`.
pub fn check_for_update(
    current: &Version,
    repo_owner: &str,
    repo_name: &str,
) -> Result<Option<ReleaseInfo>, Error> {
    let url = format!("https://api.github.com/repos/{repo_owner}/{repo_name}/releases/latest");

    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("Vmux/{current}"))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(Error::Http)?;

    let resp = client.get(&url).send().map_err(Error::Http)?;

    if resp.status() == reqwest::StatusCode::FORBIDDEN
        || resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err(Error::RateLimited);
    }

    if !resp.status().is_success() {
        return Err(Error::HttpStatus(resp.status()));
    }

    let release: GitHubRelease = resp.json().map_err(Error::Http)?;

    let tag = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let latest = Version::parse(tag).map_err(Error::SemVer)?;

    if latest <= *current {
        return Ok(None);
    }

    let tarball = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(APP_TARBALL_PATTERN) && !a.name.ends_with(SHA256_SUFFIX))
        .ok_or(Error::MissingAsset("app tarball"))?;

    let sha256 = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(&format!("{APP_TARBALL_PATTERN}{SHA256_SUFFIX}")))
        .ok_or(Error::MissingAsset("sha256"))?;

    Ok(Some(ReleaseInfo {
        version: latest,
        tarball_url: tarball.browser_download_url.clone(),
        sha256_url: sha256.browser_download_url.clone(),
    }))
}

#[derive(Debug)]
pub enum Error {
    Http(reqwest::Error),
    HttpStatus(reqwest::StatusCode),
    RateLimited,
    SemVer(semver::Error),
    MissingAsset(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(e) => write!(f, "HTTP error: {e}"),
            Error::HttpStatus(s) => write!(f, "HTTP status: {s}"),
            Error::RateLimited => write!(f, "GitHub API rate limited"),
            Error::SemVer(e) => write!(f, "version parse error: {e}"),
            Error::MissingAsset(name) => write!(f, "missing release asset: {name}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_release_json() {
        let json = r#"{
            "tag_name": "v0.2.0",
            "assets": [
                {
                    "name": "Vmux-v0.2.0-aarch64-apple-darwin.app.tar.gz",
                    "browser_download_url": "https://example.com/app.tar.gz"
                },
                {
                    "name": "Vmux-v0.2.0-aarch64-apple-darwin.app.tar.gz.sha256",
                    "browser_download_url": "https://example.com/app.tar.gz.sha256"
                },
                {
                    "name": "vmux-v0.2.0-aarch64-apple-darwin.tar.gz",
                    "browser_download_url": "https://example.com/binary.tar.gz"
                }
            ]
        }"#;

        let release: GitHubRelease = serde_json::from_str(json).unwrap();
        let tag = release.tag_name.strip_prefix('v').unwrap();
        let version = Version::parse(tag).unwrap();
        assert_eq!(version, Version::new(0, 2, 0));

        let tarball = release
            .assets
            .iter()
            .find(|a| a.name.ends_with(APP_TARBALL_PATTERN) && !a.name.ends_with(SHA256_SUFFIX))
            .unwrap();
        assert_eq!(
            tarball.browser_download_url,
            "https://example.com/app.tar.gz"
        );

        let sha = release
            .assets
            .iter()
            .find(|a| a.name.ends_with(&format!("{APP_TARBALL_PATTERN}{SHA256_SUFFIX}")))
            .unwrap();
        assert_eq!(
            sha.browser_download_url,
            "https://example.com/app.tar.gz.sha256"
        );
    }

    #[test]
    fn current_version_is_latest_returns_none() {
        let current = Version::new(0, 2, 0);
        let latest = Version::parse("0.2.0").unwrap();
        assert!(latest <= current);
    }

    #[test]
    fn newer_version_detected() {
        let current = Version::new(0, 1, 0);
        let latest = Version::parse("0.2.0").unwrap();
        assert!(latest > current);
    }
}
