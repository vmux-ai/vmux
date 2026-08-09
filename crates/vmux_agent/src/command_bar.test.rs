use super::*;

/// The id is a private round trip between the two halves of this file. A row whose id does not
/// survive it is published and then silently ignored when the user picks it.
#[test]
fn a_published_row_id_parses_back_to_what_named_it() {
    let id = app_agent_id("anthropic", "claude-opus-4");
    assert_eq!(
        parse_app_agent_id(&id),
        Some(("anthropic".to_string(), "claude-opus-4".to_string())),
        "model names contain the separator, so only the first underscore may split"
    );
}

/// Rows contributed by other crates land in the same reader; claiming them would start an
/// agent for something entirely unrelated.
#[test]
fn another_crates_row_is_left_alone() {
    assert_eq!(parse_app_agent_id("browser_open_history"), None);
    assert_eq!(parse_app_agent_id("app_new"), None);
    assert_eq!(parse_app_agent_id("app_onlyprovider_new"), None);
}

/// Only the bare urls stand for "the default agent". Claiming one that carries an id would
/// send the user to whichever agent is default instead of the one they named.
#[test]
fn only_the_bare_agent_url_is_claimed() {
    let contributions = CommandBarContributions {
        claimed_urls: DEFAULT_AGENT_URLS.map(str::to_string).to_vec(),
        ..Default::default()
    };

    assert!(contributions.claims_url("vmux://agent/"));
    assert!(contributions.claims_url("vmux://agent"));

    assert!(!contributions.claims_url("vmux://agent/codex"));
    assert!(!contributions.claims_url("vmux://agent/codex/cli"));
}
