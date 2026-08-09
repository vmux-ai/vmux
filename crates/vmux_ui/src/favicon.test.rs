use super::*;

#[test]
fn host_extracted_from_https_url() {
    assert_eq!(
        host_for_favicon_fallback("https://example.com/path"),
        Some("example.com")
    );
}

#[test]
fn host_extracted_from_http_url() {
    assert_eq!(
        host_for_favicon_fallback("http://example.com/"),
        Some("example.com")
    );
}

#[test]
fn host_extracted_when_query_string_present() {
    assert_eq!(
        host_for_favicon_fallback("https://www.google.com/search?q=mistral.ai"),
        Some("www.google.com")
    );
}

#[test]
fn host_extracted_when_fragment_present() {
    assert_eq!(
        host_for_favicon_fallback("https://example.com#frag"),
        Some("example.com")
    );
}

#[test]
fn host_none_for_non_http_scheme() {
    assert_eq!(host_for_favicon_fallback("vmux://history/"), None);
    assert_eq!(host_for_favicon_fallback("ftp://example.com"), None);
    assert_eq!(host_for_favicon_fallback(""), None);
}

#[test]
fn host_none_when_empty_after_scheme() {
    assert_eq!(host_for_favicon_fallback("https://"), None);
}

#[test]
fn agent_host_maps_vibe() {
    assert_eq!(
        agent_host("vmux://agent/vibe/chat/abc"),
        Some("chat.mistral.ai")
    );
    assert_eq!(
        agent_host("vmux://agent/vibe/cli/abc"),
        Some("chat.mistral.ai")
    );
    assert_eq!(
        agent_host("vmux://agent/mistral-vibe/session-1"),
        Some("chat.mistral.ai")
    );
}

#[test]
fn agent_host_maps_claude_and_codex() {
    assert_eq!(agent_host("vmux://agent/claude/x"), Some("claude.ai"));
    assert_eq!(agent_host("vmux://agent/claude-acp/x"), Some("claude.ai"));
    assert_eq!(agent_host("vmux://agent/codex/x"), Some("chatgpt.com"));
    assert_eq!(agent_host("vmux://agent/codex-acp/x"), Some("chatgpt.com"));
}

#[test]
fn agent_host_unknown_returns_none() {
    assert_eq!(agent_host("vmux://agent/unknown/x"), None);
    assert_eq!(agent_host("https://example.com"), None);
}

#[test]
fn favicon_src_returns_real_when_present() {
    assert_eq!(
        favicon_src_for_url("https://cdn.example.com/icon.png", "https://example.com/"),
        Some("https://cdn.example.com/icon.png".to_string())
    );
}

#[test]
fn favicon_src_prefers_agent_host_over_passed_icon() {
    // A registry icon is passed, but a known agent url still resolves to the brand favicon
    // so the agent reads consistently across every surface.
    assert_eq!(
        favicon_src_for_url("https://cdn.example/claude-acp.svg", "vmux://agent/claude"),
        Some("https://www.google.com/s2/favicons?domain=claude.ai&sz=64".to_string())
    );
}

#[test]
fn favicon_src_falls_back_to_google_s2_for_http_url() {
    assert_eq!(
        favicon_src_for_url("", "https://mistral.ai/"),
        Some("https://www.google.com/s2/favicons?domain=mistral.ai&sz=64".to_string())
    );
}

#[test]
fn favicon_src_falls_back_to_google_s2_for_google_search() {
    assert_eq!(
        favicon_src_for_url("", "https://www.google.com/search?q=mistral.ai"),
        Some("https://www.google.com/s2/favicons?domain=www.google.com&sz=64".to_string())
    );
}

#[test]
fn favicon_src_falls_back_to_agent_host() {
    assert_eq!(
        favicon_src_for_url("", "vmux://agent/vibe/chat/abc"),
        Some("https://www.google.com/s2/favicons?domain=chat.mistral.ai&sz=64".to_string())
    );
}

#[test]
fn favicon_src_none_for_vmux_scheme_without_agent() {
    assert_eq!(favicon_src_for_url("", "vmux://history/"), None);
    assert_eq!(favicon_src_for_url("", ""), None);
}

#[test]
fn agent_host_matches_single_segment_acp_url() {
    assert_eq!(agent_host("vmux://agent/claude"), Some("claude.ai"));
    assert_eq!(agent_host("vmux://agent/codex"), Some("chatgpt.com"));
    assert_eq!(agent_host("vmux://agent/claude/cli"), Some("claude.ai"));
}

#[test]
fn agent_host_maps_gemini() {
    assert_eq!(agent_host("vmux://agent/gemini"), Some("gemini.google.com"));
}

#[test]
fn agent_host_does_not_over_match_similar_ids() {
    assert_eq!(agent_host("vmux://agent/claudex"), None);
}
