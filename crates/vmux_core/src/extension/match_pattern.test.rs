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
