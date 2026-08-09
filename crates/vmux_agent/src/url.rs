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
#[path = "url.test.rs"]
mod tests;
