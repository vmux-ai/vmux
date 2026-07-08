pub use vmux_core::agent::AgentKind;

use crate::AgentVariant;

/// Reserved marker segment for CLI agents: `vmux://agent/<kind>/cli` opens a fresh CLI session,
/// `vmux://agent/<kind>/cli/<sid>` resumes the session named by `<sid>`. The plain two-segment
/// form `vmux://agent/<id>/<sid>` (no `cli` marker) belongs to ACP sessions.
pub const CLI_FRESH_SID: &str = "cli";

pub fn page_url_prefix(provider: &str, model: &str) -> String {
    format!("vmux://agent/{provider}/{model}/")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentUrl {
    Cli {
        kind: AgentKind,
        sid: String,
    },
    /// A registry-driven ACP agent. `sid` is the agent-assigned session id when known
    /// (`vmux://agent/<id>/<sid>`), or `None` for a fresh open (`vmux://agent/<id>`).
    Acp {
        id: String,
        sid: Option<String>,
    },
    Page {
        provider: String,
        model: String,
        sid: String,
    },
    PageDefault,
}

impl AgentUrl {
    pub fn parse(url: &str) -> Option<Self> {
        let body = url.strip_prefix("vmux://agent/")?;
        let segs: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
        match segs.as_slice() {
            [] => Some(AgentUrl::PageDefault),
            [id] => Some(AgentUrl::Acp {
                id: (*id).to_string(),
                sid: None,
            }),
            [x, y] => {
                if *y == CLI_FRESH_SID
                    && let Some(kind) = AgentKind::from_url_segment(x)
                {
                    // `vmux://agent/<kind>/cli` — fresh CLI session.
                    Some(AgentUrl::Cli {
                        kind,
                        sid: CLI_FRESH_SID.to_string(),
                    })
                } else {
                    // `vmux://agent/<id>/<sid>` — an ACP session.
                    Some(AgentUrl::Acp {
                        id: (*x).to_string(),
                        sid: Some((*y).to_string()),
                    })
                }
            }
            [x, y, z] => {
                if *y == CLI_FRESH_SID
                    && let Some(kind) = AgentKind::from_url_segment(x)
                {
                    // `vmux://agent/<kind>/cli/<sid>` — resume a CLI session.
                    Some(AgentUrl::Cli {
                        kind,
                        sid: (*z).to_string(),
                    })
                } else {
                    // `vmux://agent/<provider>/<model>/<sid>` — a Page session.
                    Some(AgentUrl::Page {
                        provider: (*x).to_string(),
                        model: (*y).to_string(),
                        sid: (*z).to_string(),
                    })
                }
            }
            _ => None,
        }
    }

    pub fn variant(&self) -> AgentVariant {
        match self {
            AgentUrl::Cli { .. } => AgentVariant::Cli,
            // ACP reuses the Page stream/UI infrastructure.
            AgentUrl::Acp { .. } | AgentUrl::Page { .. } | AgentUrl::PageDefault => {
                AgentVariant::Page
            }
        }
    }

    pub fn sid(&self) -> &str {
        match self {
            AgentUrl::Cli { sid, .. } => sid,
            AgentUrl::Acp { sid, .. } => sid.as_deref().unwrap_or(""),
            AgentUrl::Page { sid, .. } => sid,
            AgentUrl::PageDefault => "",
        }
    }

    pub fn format(&self) -> String {
        match self {
            AgentUrl::Cli { kind, sid } => {
                if sid == CLI_FRESH_SID {
                    format!("{}{CLI_FRESH_SID}", kind.cli_url_prefix())
                } else {
                    format!("{}{CLI_FRESH_SID}/{sid}", kind.cli_url_prefix())
                }
            }
            AgentUrl::Acp { id, sid } => match sid {
                Some(sid) => format!("vmux://agent/{id}/{sid}"),
                None => format!("vmux://agent/{id}"),
            },
            AgentUrl::Page {
                provider,
                model,
                sid,
            } => format!("{}{sid}", page_url_prefix(provider, model)),
            AgentUrl::PageDefault => "vmux://agent/".to_string(),
        }
    }

    /// The url that opens `(kind, sid)` in the requested runtime. ACP is only addressable when
    /// the kind's segment is a configured ACP id (e.g. claude, codex); otherwise this falls
    /// back to CLI so the url is always openable.
    pub fn for_session(kind: AgentKind, sid: &str, prefer_acp: bool, acp_ids: &[String]) -> Self {
        let seg = kind.as_url_segment();
        if prefer_acp && acp_ids.iter().any(|id| id == seg) {
            AgentUrl::Acp {
                id: seg.to_string(),
                sid: Some(sid.to_string()),
            }
        } else {
            AgentUrl::Cli {
                kind,
                sid: sid.to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
            AgentUrl::for_session(AgentKind::Vibe, "s2", true, &ids),
            AgentUrl::Cli {
                kind: AgentKind::Vibe,
                sid: "s2".into(),
            }
        );
        assert_eq!(
            AgentUrl::for_session(AgentKind::Claude, "s3", false, &ids),
            AgentUrl::Cli {
                kind: AgentKind::Claude,
                sid: "s3".into(),
            }
        );
    }
}
