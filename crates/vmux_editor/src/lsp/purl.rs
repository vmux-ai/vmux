/// A parsed Package URL (subset of the spec) as used by mason-registry, e.g.
/// `pkg:github/rust-lang/rust-analyzer@2026-05-25` or `pkg:npm/typescript-language-server`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Purl {
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
    pub version: Option<String>,
}

pub fn parse(s: &str) -> Option<Purl> {
    let rest = s.strip_prefix("pkg:")?;
    let (path, version) = match rest.split_once('@') {
        Some((p, v)) => (p, Some(v.to_string())),
        None => (rest, None),
    };
    let mut it = path.splitn(3, '/');
    let kind = it.next()?.to_string();
    let a = it.next()?;
    let b = it.next();
    let (namespace, name) = match b {
        Some(n) => (Some(a.to_string()), n.to_string()),
        None => (None, a.to_string()),
    };
    if kind.is_empty() || name.is_empty() {
        return None;
    }
    Some(Purl {
        kind,
        namespace,
        name,
        version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_with_namespace_and_version() {
        let p = parse("pkg:github/rust-lang/rust-analyzer@2026-05-25").unwrap();
        assert_eq!(p.kind, "github");
        assert_eq!(p.namespace.as_deref(), Some("rust-lang"));
        assert_eq!(p.name, "rust-analyzer");
        assert_eq!(p.version.as_deref(), Some("2026-05-25"));
    }

    #[test]
    fn npm_no_namespace_no_version() {
        let p = parse("pkg:npm/typescript-language-server").unwrap();
        assert_eq!(p.kind, "npm");
        assert_eq!(p.namespace, None);
        assert_eq!(p.name, "typescript-language-server");
        assert_eq!(p.version, None);
    }

    #[test]
    fn cargo_with_version() {
        let p = parse("pkg:cargo/taplo-cli@0.9.0").unwrap();
        assert_eq!(p.kind, "cargo");
        assert_eq!(p.name, "taplo-cli");
        assert_eq!(p.version.as_deref(), Some("0.9.0"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("rust-analyzer").is_none());
        assert!(parse("pkg:").is_none());
    }
}
