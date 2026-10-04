use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::lsp::package_path::Sha256Digest;

pub const CATALOG_MAX_BYTES: u64 = 32 * 1024 * 1024;
pub const PACKAGE_MAX_BYTES: u64 = 512 * 1024 * 1024;
pub const CHECKSUM_MAX_BYTES: u64 = 1024 * 1024;
const GITHUB_RELEASE_MAX_BYTES: u64 = 8 * 1024 * 1024;

struct BoundedResponse(reqwest::blocking::Response);

impl BoundedResponse {
    fn read(self, max_bytes: u64, subject: &str) -> Result<Vec<u8>, String> {
        if self
            .0
            .content_length()
            .is_some_and(|length| length > max_bytes)
        {
            return Err(format!("{subject} exceeds {max_bytes} bytes"));
        }
        let mut bytes = Vec::new();
        self.0
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > max_bytes {
            return Err(format!("{subject} exceeds {max_bytes} bytes"));
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteArtifact {
    pub url: String,
    pub sha256: Sha256Digest,
}

impl Sha256Digest {
    pub fn from_manifest(url: &str, filename: &str, max_bytes: u64) -> Result<Self, String> {
        let url = checked_url(url)?;
        let response = client()?
            .get(url)
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("http {}", response.status()));
        }
        let bytes = BoundedResponse(response).read(max_bytes, "checksum manifest")?;
        let text = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
        for line in text.lines() {
            let mut fields = line.split_whitespace();
            let Some(digest) = fields.next() else {
                continue;
            };
            let Some(name) = fields.next() else {
                continue;
            };
            if name.trim_start_matches('*') == filename {
                return Self::parse(digest);
            }
        }
        Err(format!("checksum not found for {filename}"))
    }
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("too many redirects");
            }
            if trusted_url(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("redirect URL must use HTTPS")
            }
        }))
        .user_agent(concat!("vmux/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())
}

fn checked_url(url: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(url).map_err(|error| error.to_string())?;
    trusted_url(&url)
        .then_some(url)
        .ok_or_else(|| "download URL must use HTTPS".to_string())
}

fn trusted_url(url: &url::Url) -> bool {
    if url.scheme() == "https" {
        return true;
    }
    let loopback = url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    url.scheme() == "http" && loopback
}

impl RemoteArtifact {
    pub fn new(url: impl Into<String>, sha256: Sha256Digest) -> Self {
        Self {
            url: url.into(),
            sha256,
        }
    }

    pub fn github_release(
        owner: &str,
        repository: &str,
        tag: Option<&str>,
        asset_name: &str,
        max_bytes: u64,
    ) -> Result<Self, String> {
        let mut endpoint = url::Url::parse("https://api.github.com").map_err(|e| e.to_string())?;
        {
            let mut path = endpoint
                .path_segments_mut()
                .map_err(|_| "invalid GitHub API base URL".to_string())?;
            path.extend(["repos", owner, repository, "releases"]);
            match tag {
                Some(tag) => path.extend(["tags", tag]),
                None => path.push("latest"),
            };
        }
        let mut request = client()?.get(endpoint);
        if let Ok(token) = std::env::var("GITHUB_TOKEN")
            && !token.trim().is_empty()
        {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("GitHub API returned {}", response.status()));
        }
        let bytes =
            BoundedResponse(response).read(GITHUB_RELEASE_MAX_BYTES, "GitHub release metadata")?;
        let release: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let asset = release
            .get("assets")
            .and_then(serde_json::Value::as_array)
            .and_then(|assets| {
                assets.iter().find(|asset| {
                    asset.get("name").and_then(serde_json::Value::as_str) == Some(asset_name)
                })
            })
            .ok_or_else(|| format!("GitHub release asset not found: {asset_name}"))?;
        let size = asset
            .get("size")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("GitHub release asset size missing: {asset_name}"))?;
        if size > max_bytes {
            return Err(format!("download exceeds {max_bytes} bytes"));
        }
        let url = asset
            .get("browser_download_url")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("GitHub release asset URL missing: {asset_name}"))?;
        checked_url(url)?;
        let digest = asset
            .get("digest")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("GitHub release asset digest missing: {asset_name}"))?;
        Ok(Self::new(url, Sha256Digest::parse(digest)?))
    }

    pub fn download_to(
        &self,
        destination: &Path,
        max_bytes: u64,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<(), String> {
        let url = checked_url(&self.url)?;
        let mut response = client()?
            .get(url)
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("http {}", response.status()));
        }
        let total = response.content_length();
        if total.is_some_and(|total| total > max_bytes) {
            return Err(format!("download exceeds {max_bytes} bytes"));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        let mut file =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        let mut buffer = [0u8; 8192];
        let mut downloaded = 0u64;
        let mut hasher = Sha256::new();
        loop {
            let count = response
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            downloaded = downloaded
                .checked_add(count as u64)
                .ok_or_else(|| "download size overflow".to_string())?;
            if downloaded > max_bytes {
                return Err(format!("download exceeds {max_bytes} bytes"));
            }
            file.write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
            hasher.update(&buffer[..count]);
            progress(downloaded, total);
        }
        let actual = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if actual != self.sha256.as_str() {
            return Err(format!(
                "SHA-256 mismatch: expected {}, got {actual}",
                self.sha256.as_str()
            ));
        }
        file.flush().map_err(|error| error.to_string())?;
        file.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        file.persist(destination)
            .map_err(|error| error.error.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn serve_once(body: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut req = [0u8; 1024];
                let _ = stream.read(&mut req);
                let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body);
            }
        });
        format!("http://{addr}/file")
    }

    #[test]
    fn downloads_and_hashes() {
        let url = serve_once(b"hello vmux lsp");
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.bin");
        let mut last = 0u64;
        let digest =
            Sha256Digest::parse("ed16a0c68a7df1e55597fcb7c884140ce292def6116cbaab1fc05045433494b9")
                .unwrap();
        RemoteArtifact::new(url, digest)
            .download_to(&dest, 1024, |downloaded, _| last = downloaded)
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"hello vmux lsp");
        assert_eq!(last, 14);
    }

    #[test]
    fn oversized_and_mismatched_downloads_are_not_activated() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.bin");
        let digest =
            Sha256Digest::parse("0000000000000000000000000000000000000000000000000000000000000000")
                .unwrap();
        assert!(
            RemoteArtifact::new(serve_once(b"too large"), digest.clone())
                .download_to(&dest, 3, |_, _| {})
                .is_err()
        );
        assert!(!dest.exists());
        assert!(
            RemoteArtifact::new(serve_once(b"wrong"), digest)
                .download_to(&dest, 1024, |_, _| {})
                .is_err()
        );
        assert!(!dest.exists());
    }

    #[test]
    fn bounded_responses_reject_declared_overflow() {
        let declared = client()
            .unwrap()
            .get(serve_once(b"oversized"))
            .send()
            .unwrap();
        assert!(BoundedResponse(declared).read(3, "response").is_err());
    }

    #[test]
    fn parses_bounded_checksum_manifest() {
        let filename = "package.tar.gz";
        let hash = "ed16a0c68a7df1e55597fcb7c884140ce292def6116cbaab1fc05045433494b9";
        let body = format!("{hash}  {filename}\n").into_bytes().leak();
        let digest = Sha256Digest::from_manifest(&serve_once(body), filename, 1024).unwrap();
        assert_eq!(digest.as_str(), hash);
    }

    #[test]
    fn download_urls_allow_https_and_loopback_http_only() {
        assert!(checked_url("https://example.com/file").is_ok());
        assert!(checked_url("http://127.0.0.1/file").is_ok());
        assert!(checked_url("http://localhost/file").is_ok());
        assert!(checked_url("http://example.com/file").is_err());
        assert!(checked_url("file:///tmp/file").is_err());
    }
}
