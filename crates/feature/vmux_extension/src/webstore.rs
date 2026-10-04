pub struct ChromeWebStore;

impl ChromeWebStore {
    pub fn extension_id(input: &str) -> Option<String> {
        let trimmed = input.trim();
        if Self::is_extension_id(trimmed) {
            return Some(trimmed.to_string());
        }
        trimmed
            .split(['/', '?', '#'])
            .find(|segment| Self::is_extension_id(segment))
            .map(str::to_string)
    }

    pub fn crx_url(id: &str, prodversion: &str) -> String {
        format!(
            "https://clients2.google.com/service/update2/crx?response=redirect&acceptformat=crx2,crx3&prodversion={prodversion}&x=id%3D{id}%26installsource%3Dondemand%26uc"
        )
    }

    fn is_extension_id(value: &str) -> bool {
        value.len() == 32 && value.bytes().all(|byte| (b'a'..=b'p').contains(&byte))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_id_from_new_store_url() {
        let id = ChromeWebStore::extension_id(
        "https://chromewebstore.google.com/detail/ublock-origin/cjpalhdlnbpafiamejdnhcphjbkeiagm",
    )
    .unwrap();
        assert_eq!(id, "cjpalhdlnbpafiamejdnhcphjbkeiagm");
    }

    #[test]
    fn extracts_id_from_legacy_url() {
        let id = ChromeWebStore::extension_id(
            "https://chrome.google.com/webstore/detail/foo/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap();
        assert_eq!(id, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    }

    #[test]
    fn accepts_bare_id() {
        let id = ChromeWebStore::extension_id("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        assert_eq!(id, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    }

    #[test]
    fn rejects_junk() {
        assert!(ChromeWebStore::extension_id("not an extension").is_none());
        assert!(ChromeWebStore::extension_id("https://example.com").is_none());
    }

    #[test]
    fn rejects_wrong_length_or_chars() {
        assert!(ChromeWebStore::extension_id("zzzz").is_none());
        assert!(ChromeWebStore::extension_id("qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq").is_none());
    }

    #[test]
    fn builds_crx_url() {
        let url = ChromeWebStore::crx_url("cjpalhdlnbpafiamejdnhcphjbkeiagm", "120.0.0.0");
        assert!(url.contains("id%3Dcjpalhdlnbpafiamejdnhcphjbkeiagm"));
        assert!(url.contains("prodversion=120.0.0.0"));
        assert!(url.contains("acceptformat=crx2,crx3"));
    }
}
