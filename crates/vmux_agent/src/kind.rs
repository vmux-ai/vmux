pub use vmux_core::agent::AgentKind;

use crate::AgentVariant;

pub fn app_url_prefix(provider: &str, model: &str) -> String {
    format!("vmux://agent/{provider}/{model}/")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentUrl {
    Cli {
        kind: AgentKind,
        sid: String,
    },
    App {
        provider: String,
        model: String,
        sid: String,
    },
}

impl AgentUrl {
    pub fn parse(url: &str) -> Option<Self> {
        let body = url.strip_prefix("vmux://agent/")?;
        let segs: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
        match segs.as_slice() {
            [kind_seg, sid] => {
                let kind = AgentKind::from_url_segment(kind_seg)?;
                Some(AgentUrl::Cli {
                    kind,
                    sid: (*sid).to_string(),
                })
            }
            [provider, model, sid] => Some(AgentUrl::App {
                provider: (*provider).to_string(),
                model: (*model).to_string(),
                sid: (*sid).to_string(),
            }),
            _ => None,
        }
    }

    pub fn variant(&self) -> AgentVariant {
        match self {
            AgentUrl::Cli { .. } => AgentVariant::Cli,
            AgentUrl::App { .. } => AgentVariant::App,
        }
    }

    pub fn sid(&self) -> &str {
        match self {
            AgentUrl::Cli { sid, .. } => sid,
            AgentUrl::App { sid, .. } => sid,
        }
    }

    pub fn format(&self) -> String {
        match self {
            AgentUrl::Cli { kind, sid } => format!("{}{sid}", kind.cli_url_prefix()),
            AgentUrl::App {
                provider,
                model,
                sid,
            } => format!("{}{sid}", app_url_prefix(provider, model)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_url_prefix_returns_four_segment_form() {
        assert_eq!(
            app_url_prefix("openai", "gpt-5.5"),
            "vmux://agent/openai/gpt-5.5/"
        );
    }

    #[test]
    fn cli_url_parses_three_segments() {
        let parsed = AgentUrl::parse("vmux://agent/vibe/abc-123").unwrap();
        assert_eq!(
            parsed,
            AgentUrl::Cli {
                kind: AgentKind::Vibe,
                sid: "abc-123".into(),
            }
        );
    }

    #[test]
    fn app_url_parses_four_segments() {
        let parsed = AgentUrl::parse("vmux://agent/openai/gpt-5.5/xHigh").unwrap();
        assert_eq!(
            parsed,
            AgentUrl::App {
                provider: "openai".into(),
                model: "gpt-5.5".into(),
                sid: "xHigh".into(),
            }
        );
    }

    #[test]
    fn unknown_cli_kind_returns_none() {
        assert!(AgentUrl::parse("vmux://agent/nope/abc").is_none());
    }

    #[test]
    fn url_format_round_trips_cli() {
        let u = AgentUrl::Cli {
            kind: AgentKind::Codex,
            sid: "xyz".into(),
        };
        assert_eq!(u.format(), "vmux://agent/codex/xyz");
        assert_eq!(AgentUrl::parse(&u.format()), Some(u));
    }

    #[test]
    fn url_format_round_trips_app() {
        let u = AgentUrl::App {
            provider: "anthropic".into(),
            model: "claude-opus-4.7".into(),
            sid: "xyz".into(),
        };
        assert_eq!(u.format(), "vmux://agent/anthropic/claude-opus-4.7/xyz");
        assert_eq!(AgentUrl::parse(&u.format()), Some(u));
    }

    #[test]
    fn trailing_garbage_rejected() {
        assert_eq!(AgentUrl::parse("vmux://agent/vibe/abc/extra/junk"), None);
        assert_eq!(AgentUrl::parse("vmux://agent/openai/gpt/sid/extra"), None);
    }

    #[test]
    fn prefix_only_url_rejected() {
        assert_eq!(AgentUrl::parse("vmux://agent/vibe/"), None);
        assert_eq!(AgentUrl::parse("vmux://agent/openai/gpt-5.5/"), None);
    }

    #[test]
    fn variant_returned_correctly() {
        let cli = AgentUrl::Cli {
            kind: AgentKind::Vibe,
            sid: "x".into(),
        };
        assert_eq!(cli.variant(), AgentVariant::Cli);
        let app = AgentUrl::App {
            provider: "p".into(),
            model: "m".into(),
            sid: "x".into(),
        };
        assert_eq!(app.variant(), AgentVariant::App);
    }
}
