use super::*;

#[test]
fn bare_agent_url_parses_to_page_default() {
    assert_eq!(
        AgentUrl::parse("vmux://agent/"),
        Some(AgentUrl::PageDefault)
    );
}

#[test]
fn single_segment_is_acp_fresh() {
    assert_eq!(
        AgentUrl::parse("vmux://agent/claude"),
        Some(AgentUrl::Acp {
            id: "claude".into(),
            sid: None,
        })
    );
    assert_eq!(
        AgentUrl::parse("vmux://agent/mistral-vibe"),
        Some(AgentUrl::Acp {
            id: "mistral-vibe".into(),
            sid: None,
        })
    );
}

#[test]
fn two_segment_plain_is_acp_session() {
    assert_eq!(
        AgentUrl::parse("vmux://agent/claude/abc-123"),
        Some(AgentUrl::Acp {
            id: "claude".into(),
            sid: Some("abc-123".into()),
        })
    );
}

#[test]
fn two_segment_cli_marker_is_fresh_cli() {
    assert_eq!(
        AgentUrl::parse("vmux://agent/claude/cli"),
        Some(AgentUrl::Cli {
            kind: AgentKind::Claude,
            sid: CLI_FRESH_SID.into(),
        })
    );
}

#[test]
fn three_segment_cli_marker_is_cli_resume() {
    assert_eq!(
        AgentUrl::parse("vmux://agent/vibe/cli/abc-123"),
        Some(AgentUrl::Cli {
            kind: AgentKind::Vibe,
            sid: "abc-123".into(),
        })
    );
}

#[test]
fn three_segment_plain_is_page() {
    assert_eq!(
        AgentUrl::parse("vmux://agent/openai/gpt-5.5/xHigh"),
        Some(AgentUrl::Page {
            provider: "openai".into(),
            model: "gpt-5.5".into(),
            sid: "xHigh".into(),
        })
    );
}

#[test]
fn cli_marker_with_non_kind_falls_through_to_acp() {
    // `fast-agent` is not a CLI kind, so the `cli` word is just a session id for ACP.
    assert_eq!(
        AgentUrl::parse("vmux://agent/fast-agent/cli"),
        Some(AgentUrl::Acp {
            id: "fast-agent".into(),
            sid: Some("cli".into()),
        })
    );
}

#[test]
fn too_many_segments_rejected() {
    assert_eq!(AgentUrl::parse("vmux://agent/vibe/cli/abc/extra"), None);
    assert_eq!(AgentUrl::parse("vmux://agent/o/m/sid/extra"), None);
}

#[test]
fn acp_format_round_trips() {
    for u in [
        AgentUrl::Acp {
            id: "claude".into(),
            sid: None,
        },
        AgentUrl::Acp {
            id: "mistral-vibe".into(),
            sid: Some("sess-9".into()),
        },
    ] {
        assert_eq!(AgentUrl::parse(&u.format()), Some(u));
    }
}

#[test]
fn cli_format_round_trips_fresh_and_resume() {
    let fresh = AgentUrl::Cli {
        kind: AgentKind::Codex,
        sid: CLI_FRESH_SID.into(),
    };
    assert_eq!(fresh.format(), "vmux://agent/codex/cli");
    assert_eq!(AgentUrl::parse(&fresh.format()), Some(fresh));

    let resume = AgentUrl::Cli {
        kind: AgentKind::Codex,
        sid: "xyz".into(),
    };
    assert_eq!(resume.format(), "vmux://agent/codex/cli/xyz");
    assert_eq!(AgentUrl::parse(&resume.format()), Some(resume));
}

#[test]
fn page_format_round_trips() {
    let u = AgentUrl::Page {
        provider: "anthropic".into(),
        model: "claude-opus-4.7".into(),
        sid: "xyz".into(),
    };
    assert_eq!(u.format(), "vmux://agent/anthropic/claude-opus-4.7/xyz");
    assert_eq!(AgentUrl::parse(&u.format()), Some(u));
}

#[test]
fn page_default_round_trips() {
    assert_eq!(AgentUrl::PageDefault.format(), "vmux://agent/");
    assert_eq!(
        AgentUrl::parse(&AgentUrl::PageDefault.format()),
        Some(AgentUrl::PageDefault)
    );
}

#[test]
fn variant_returned_correctly() {
    assert_eq!(
        AgentUrl::Cli {
            kind: AgentKind::Vibe,
            sid: "x".into(),
        }
        .variant(),
        AgentVariant::Cli
    );
    assert_eq!(
        AgentUrl::Acp {
            id: "claude".into(),
            sid: None,
        }
        .variant(),
        AgentVariant::Page
    );
}

#[test]
fn for_session_prefers_acp_when_configured() {
    let ids = vec!["claude".to_string(), "codex".to_string()];
    assert_eq!(
        AgentUrl::for_session(AgentKind::Claude, "s1", true, &ids),
        AgentUrl::Acp {
            id: "claude".into(),
            sid: Some("s1".into()),
        }
    );
    assert_eq!(
        AgentUrl::for_session(AgentKind::Codex, "s2", true, &ids),
        AgentUrl::Acp {
            id: "codex".into(),
            sid: Some("s2".into()),
        }
    );
    assert_eq!(
        AgentUrl::for_session(AgentKind::Vibe, "s3", true, &ids),
        AgentUrl::Cli {
            kind: AgentKind::Vibe,
            sid: "s3".into(),
        }
    );
    assert_eq!(
        AgentUrl::for_session(AgentKind::Claude, "s4", false, &ids),
        AgentUrl::Cli {
            kind: AgentKind::Claude,
            sid: "s4".into(),
        }
    );
}
