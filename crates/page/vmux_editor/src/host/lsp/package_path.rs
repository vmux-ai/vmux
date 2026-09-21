use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageName(String);

impl PackageName {
    pub fn parse(value: &str) -> Result<Self, String> {
        if value.is_empty()
            || value == "."
            || value == ".."
            || value.contains('/')
            || value.contains('\\')
            || value.contains('\0')
        {
            return Err(format!("invalid package name: {value}"));
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PackageName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackagePath(PathBuf);

impl PackagePath {
    pub fn parse(value: &str) -> Result<Self, String> {
        let path = Path::new(value);
        if value.is_empty() || value.contains('\\') || path.is_absolute() {
            return Err(format!("invalid package path: {value}"));
        }
        let mut count = 0usize;
        for component in path.components() {
            if !matches!(component, Component::Normal(_)) {
                return Err(format!("invalid package path: {value}"));
            }
            count += 1;
        }
        if count == 0 {
            return Err(format!("invalid package path: {value}"));
        }
        Ok(Self(path.to_path_buf()))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.to_str().unwrap_or_default()
    }
}

impl std::fmt::Display for PackagePath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.display().fmt(formatter)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.strip_prefix("sha256:").unwrap_or(value);
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("invalid SHA-256 digest".to_string());
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_reject_path_syntax() {
        for value in ["", ".", "..", "a/b", "a\\b"] {
            assert!(PackageName::parse(value).is_err(), "{value}");
        }
        assert_eq!(
            PackageName::parse("rust-analyzer").unwrap().as_str(),
            "rust-analyzer"
        );
    }

    #[test]
    fn package_paths_are_relative_and_normal() {
        for value in [
            "",
            "/bin/server",
            "../server",
            "bin/../server",
            "bin\\server",
        ] {
            assert!(PackagePath::parse(value).is_err(), "{value}");
        }
        assert_eq!(
            PackagePath::parse("node_modules/.bin/server")
                .unwrap()
                .as_path(),
            Path::new("node_modules/.bin/server")
        );
    }

    #[test]
    fn digest_requires_sha256_hex() {
        let value = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(Sha256Digest::parse(value).unwrap().as_str(), value);
        assert_eq!(
            Sha256Digest::parse(&format!("sha256:{value}"))
                .unwrap()
                .as_str(),
            value
        );
        assert!(Sha256Digest::parse("sha256:nope").is_err());
    }
}
