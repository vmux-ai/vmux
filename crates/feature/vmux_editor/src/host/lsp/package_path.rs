use std::path::{Component, Path, PathBuf};

struct PortableComponent;

impl PortableComponent {
    fn accepts(value: &str) -> bool {
        if value.is_empty()
            || value == "."
            || value == ".."
            || value.ends_with(['.', ' '])
            || value.chars().any(|character| {
                character.is_control()
                    || matches!(
                        character,
                        '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                    )
            })
        {
            return false;
        }
        let stem = value
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        !matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageName(String);

impl PackageName {
    pub fn parse(value: &str) -> Result<Self, String> {
        if !PortableComponent::accepts(value) {
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

impl serde::Serialize for PackageName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for PackageName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
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
            let Component::Normal(component) = component else {
                return Err(format!("invalid package path: {value}"));
            };
            let Some(component) = component.to_str() else {
                return Err(format!("invalid package path: {value}"));
            };
            if !PortableComponent::accepts(component) {
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

impl serde::Serialize for PackagePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for PackagePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

impl From<&PackageName> for PackagePath {
    fn from(name: &PackageName) -> Self {
        Self(PathBuf::from(name.as_str()))
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
        for value in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "a:b",
            "name.",
            "name ",
            "CON",
            "com1.exe",
            "line\nbreak",
        ] {
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
            "bin/server.",
            "bin/NUL",
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
    fn package_name_is_a_valid_package_path() {
        let name = PackageName::parse("rust-analyzer").unwrap();
        assert_eq!(
            PackagePath::from(&name).as_path(),
            Path::new("rust-analyzer")
        );
    }

    #[test]
    fn serde_revalidates_names_and_paths() {
        assert!(serde_json::from_str::<PackageName>(r#""../escape""#).is_err());
        assert!(serde_json::from_str::<PackagePath>(r#""bin/../../escape""#).is_err());
        let name = serde_json::from_str::<PackageName>(r#""rust-analyzer""#).unwrap();
        let path = serde_json::from_str::<PackagePath>(r#""node_modules/.bin/server""#).unwrap();
        assert_eq!(name.as_str(), "rust-analyzer");
        assert_eq!(path.as_str(), "node_modules/.bin/server");
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
