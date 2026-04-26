use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Download a file from `url` to `dest`, streaming to disk.
/// Returns the SHA-256 hex digest of the downloaded file.
pub fn download_file(url: &str, dest: &Path) -> Result<String, Error> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(Error::Io)?;
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("Vmux/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(Error::Http)?;

    let mut resp = client.get(url).send().map_err(Error::Http)?;
    if !resp.status().is_success() {
        return Err(Error::HttpStatus(resp.status()));
    }

    let mut file = std::fs::File::create(dest).map_err(Error::Io)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];

    loop {
        let n = resp.read(&mut buf).map_err(Error::Io)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(Error::Io)?;
        hasher.update(&buf[..n]);
    }

    file.flush().map_err(Error::Io)?;
    drop(file);

    let hash = format!("{:x}", hasher.finalize());
    Ok(hash)
}

/// Fetch the expected SHA-256 hash from a `.sha256` URL.
/// The file is expected to contain just the hex digest (no filename).
pub fn fetch_expected_sha256(url: &str) -> Result<String, Error> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("Vmux/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(Error::Http)?;

    let resp = client.get(url).send().map_err(Error::Http)?;
    if !resp.status().is_success() {
        return Err(Error::HttpStatus(resp.status()));
    }

    let text = resp.text().map_err(Error::Http)?;
    Ok(text.trim().to_string())
}

/// Download tarball + sha256, verify checksum.
/// Returns (path to downloaded tarball, verified SHA-256 hex digest).
pub fn download_and_verify(
    tarball_url: &str,
    sha256_url: &str,
    download_dir: &Path,
) -> Result<(PathBuf, String), Error> {
    // Clean up any previous partial download
    if download_dir.exists() {
        std::fs::remove_dir_all(download_dir).map_err(Error::Io)?;
    }

    let tarball_path = download_dir.join("update.tar.gz");

    let expected_hash = fetch_expected_sha256(sha256_url)?;
    let actual_hash = download_file(tarball_url, &tarball_path)?;

    if actual_hash != expected_hash {
        let _ = std::fs::remove_dir_all(download_dir);
        return Err(Error::ChecksumMismatch {
            expected: expected_hash,
            actual: actual_hash,
        });
    }

    Ok((tarball_path, actual_hash))
}

#[derive(Debug)]
pub enum Error {
    Http(reqwest::Error),
    HttpStatus(reqwest::StatusCode),
    Io(std::io::Error),
    ChecksumMismatch { expected: String, actual: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(e) => write!(f, "download HTTP error: {e}"),
            Error::HttpStatus(s) => write!(f, "download HTTP status: {s}"),
            Error::Io(e) => write!(f, "download I/O error: {e}"),
            Error::ChecksumMismatch { expected, actual } => {
                write!(f, "checksum mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    #[test]
    fn sha256_of_known_data() {
        let data = b"hello vmux update";
        let hash = format!("{:x}", Sha256::digest(data));
        assert_eq!(hash.len(), 64);
        let hash2 = format!("{:x}", Sha256::digest(data));
        assert_eq!(hash, hash2);
    }
}
