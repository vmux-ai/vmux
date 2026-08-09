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
#[path = "purl.test.rs"]
mod tests;
