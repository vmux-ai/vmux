#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChromeMatchPattern {
    scheme: Scheme,
    host: Host,
    path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Scheme {
    AllUrls,
    HttpAndHttps,
    Exact(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Host {
    Any,
    Domain(String),
    DomainAndSubdomains(String),
    Empty,
}

impl ChromeMatchPattern {
    pub fn parse(pattern: &str) -> Result<Self, String> {
        if pattern == "<all_urls>" {
            return Ok(Self {
                scheme: Scheme::AllUrls,
                host: Host::Any,
                path: "/*".into(),
            });
        }
        let (scheme, remainder) = pattern
            .split_once("://")
            .ok_or_else(|| format!("invalid Chrome match pattern: {pattern}"))?;
        let scheme = match scheme {
            "*" => Scheme::HttpAndHttps,
            "http" | "https" | "file" | "ftp" => Scheme::Exact(scheme.into()),
            _ => return Err(format!("invalid Chrome match pattern scheme: {pattern}")),
        };
        let slash = remainder
            .find('/')
            .ok_or_else(|| format!("Chrome match pattern has no path: {pattern}"))?;
        let host = &remainder[..slash];
        let path = &remainder[slash..];
        if path.is_empty() || !path.starts_with('/') {
            return Err(format!("invalid Chrome match pattern path: {pattern}"));
        }
        let host = match &scheme {
            Scheme::Exact(value) if value == "file" => {
                if !host.is_empty() {
                    return Err(format!("file match pattern has a host: {pattern}"));
                }
                Host::Empty
            }
            _ if host == "*" => Host::Any,
            _ if host.starts_with("*.") && host.len() > 2 && !host[2..].contains('*') => {
                Host::DomainAndSubdomains(host[2..].to_ascii_lowercase())
            }
            _ if !host.is_empty() && !host.contains('*') => Host::Domain(host.to_ascii_lowercase()),
            _ => return Err(format!("invalid Chrome match pattern host: {pattern}")),
        };
        Ok(Self {
            scheme,
            host,
            path: path.into(),
        })
    }

    pub fn matches(&self, url: &url::Url) -> bool {
        let scheme_matches = match &self.scheme {
            Scheme::AllUrls => matches!(url.scheme(), "http" | "https" | "file" | "ftp"),
            Scheme::HttpAndHttps => matches!(url.scheme(), "http" | "https"),
            Scheme::Exact(scheme) => scheme == url.scheme(),
        };
        if !scheme_matches {
            return false;
        }
        let url_host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        let host_matches = match &self.host {
            Host::Any => true,
            Host::Domain(host) => &url_host == host,
            Host::DomainAndSubdomains(host) => {
                &url_host == host || url_host.ends_with(&format!(".{host}"))
            }
            Host::Empty => url_host.is_empty(),
        };
        host_matches && wildcard_matches(&self.path, url.path())
    }
}

pub fn is_match_pattern_candidate(value: &str) -> bool {
    value == "<all_urls>" || value.contains("://")
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let starts_with_wildcard = pattern.starts_with('*');
    let ends_with_wildcard = pattern.ends_with('*');
    let parts = pattern
        .split('*')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return true;
    }
    let mut offset = 0;
    for (index, part) in parts.iter().enumerate() {
        let first = index == 0;
        let last = index + 1 == parts.len();
        if first && !starts_with_wildcard {
            if !value.starts_with(part) {
                return false;
            }
            offset = part.len();
            continue;
        }
        if last && !ends_with_wildcard {
            return value[offset..].ends_with(part);
        }
        let Some(found) = value[offset..].find(part) else {
            return false;
        };
        offset += found + part.len();
    }
    ends_with_wildcard || offset == value.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_matches_chrome_patterns() {
        let pattern = ChromeMatchPattern::parse("https://*.example.com/path/*").unwrap();
        assert!(pattern.matches(&url::Url::parse("https://login.example.com/path/x").unwrap()));
        assert!(!pattern.matches(&url::Url::parse("https://example.org/path/x").unwrap()));
        assert!(ChromeMatchPattern::parse("<all_urls>").is_ok());
        assert!(ChromeMatchPattern::parse("https://*evil.com/*").is_err());
        assert!(ChromeMatchPattern::parse("javascript://example.com/*").is_err());
        assert!(ChromeMatchPattern::parse("https://example.com").is_err());
    }
}
