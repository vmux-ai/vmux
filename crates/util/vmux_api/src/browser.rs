#[vmux_api::agent]
pub struct AgentOpenBrowser {
    pub url: String,
    pub source: Option<String>,
}

#[vmux_api::ui_event]
pub struct OpenExternalBrowserRequest {
    pub url: String,
}

pub struct Url(String);

impl Url {
    pub fn parse(value: &str) -> Result<Self, String> {
        if value.trim() != value || value.chars().any(char::is_control) {
            return Err("browser URL contains whitespace or control characters".into());
        }
        let url =
            url::Url::parse(value).map_err(|error| format!("invalid browser URL: {error}"))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err("browser URL must use HTTP or HTTPS".into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_navigation_rejects_native_schemes_and_command_arguments() {
        for url in [
            "file:///etc/passwd",
            "vmux://sessions/",
            "javascript:alert(1)",
            "--args",
            " https://example.test/",
            "https://example.test/\n",
        ] {
            assert!(Url::parse(url).is_err(), "unexpected browser URL: {url:?}");
        }
    }
}
