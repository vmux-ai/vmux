//! Which embedded page a URL belongs to.

/// The page a URL resolves to: the `<host>` in `vmux://<host>`, and the key the web build's
/// page manifest is looked up by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageHost(String);

impl PageHost {
    /// Derived from a location's protocol and host. The vault is served from a real origin
    /// rather than `vmux://`, so it is recognised by name; any other http(s) origin is a remote
    /// host and gets the layout; a `file:` URL opens the editor. Anything else already names
    /// its own page.
    pub fn of(protocol: &str, host: &str) -> Self {
        if protocol == "https:" && host == "vault.vmux.ai" {
            return Self("vault".to_string());
        }
        if matches!(protocol, "http:" | "https:") {
            return Self("remote".to_string());
        }
        if protocol == "file:" {
            return Self("files".to_string());
        }
        Self(host.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_decides_the_page_before_the_host_does() {
        assert_eq!(PageHost::of("file:", "").as_str(), "files");
        assert_eq!(PageHost::of("vmux:", "terminal").as_str(), "terminal");
        assert_eq!(PageHost::of("https:", "vault.vmux.ai").as_str(), "vault");
        assert_eq!(PageHost::of("https:", "example.com").as_str(), "remote");
        assert_eq!(
            PageHost::of("https:", "mac.example.ts.net").as_str(),
            "remote"
        );
    }
}
