#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Purl {
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
    pub version: Option<String>,
}

impl Purl {
    pub fn parse(source: &str) -> Option<Self> {
        let rest = source.strip_prefix("pkg:")?;
        let (path, version) = match rest.split_once('@') {
            Some((path, version)) => (path, Some(version.to_string())),
            None => (rest, None),
        };
        let mut parts = path.splitn(3, '/');
        let kind = parts.next()?.to_string();
        let first = parts.next()?;
        let second = parts.next();
        let (namespace, name) = match second {
            Some(name) => (Some(first.to_string()), name.to_string()),
            None => (None, first.to_string()),
        };
        if kind.is_empty() || name.is_empty() {
            return None;
        }
        Some(Self {
            kind,
            namespace,
            name,
            version,
        })
    }

    pub fn toolchain(&self) -> Option<&'static str> {
        match self.kind.as_str() {
            "npm" => Some("npm"),
            "pypi" => Some("python3"),
            "cargo" => Some("cargo"),
            "golang" => Some("go"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_with_namespace_and_version() {
        let p = Purl::parse("pkg:github/rust-lang/rust-analyzer@2026-05-25").unwrap();
        assert_eq!(p.kind, "github");
        assert_eq!(p.namespace.as_deref(), Some("rust-lang"));
        assert_eq!(p.name, "rust-analyzer");
        assert_eq!(p.version.as_deref(), Some("2026-05-25"));
    }

    #[test]
    fn npm_no_namespace_no_version() {
        let p = Purl::parse("pkg:npm/typescript-language-server").unwrap();
        assert_eq!(p.kind, "npm");
        assert_eq!(p.namespace, None);
        assert_eq!(p.name, "typescript-language-server");
        assert_eq!(p.version, None);
    }

    #[test]
    fn cargo_with_version() {
        let p = Purl::parse("pkg:cargo/taplo-cli@0.9.0").unwrap();
        assert_eq!(p.kind, "cargo");
        assert_eq!(p.name, "taplo-cli");
        assert_eq!(p.version.as_deref(), Some("0.9.0"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(Purl::parse("rust-analyzer").is_none());
        assert!(Purl::parse("pkg:").is_none());
    }
}
