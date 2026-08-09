use super::*;

#[test]
fn url_segment_round_trips() {
    for v in [AgentVariant::Page, AgentVariant::Cli] {
        assert_eq!(AgentVariant::from_url_segment(v.as_url_segment()), Some(v));
    }
}

#[test]
fn empty_segment_resolves_to_page() {
    assert_eq!(
        AgentVariant::from_url_segment(Some("")),
        Some(AgentVariant::Page)
    );
    assert_eq!(
        AgentVariant::from_url_segment(None),
        Some(AgentVariant::Page)
    );
}

#[test]
fn unknown_segment_returns_none() {
    assert_eq!(AgentVariant::from_url_segment(Some("nope")), None);
}
