use base64::Engine;
use serde::Deserialize;
use std::sync::LazyLock;

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/acp-logos/catalog.json"))
        .expect("bundled ACP logo catalog must be valid")
});

#[derive(Deserialize)]
struct Catalog {
    logos: Vec<Logo>,
}

#[derive(Deserialize)]
pub struct Logo {
    id: String,
    aliases: Vec<String>,
    svg: String,
}

impl Logo {
    pub fn of(agent_id: &str) -> Option<&'static Self> {
        let id = agent_id.to_ascii_lowercase();
        for logo in &CATALOG.logos {
            if logo.id == id || logo.aliases.iter().any(|alias| alias == &id) {
                return Some(logo);
            }
        }
        for token in id.split(|character: char| !character.is_ascii_alphanumeric()) {
            let canonical = match token {
                "claude" => "claude-acp",
                "codex" => "codex-acp",
                "gemini" => "gemini",
                "mistral" | "vibe" => "mistral-vibe",
                _ => continue,
            };
            for logo in &CATALOG.logos {
                if logo.id == canonical {
                    return Some(logo);
                }
            }
        }
        None
    }

    pub fn from_registry(source: &str) -> Option<&'static Self> {
        let path = source.strip_prefix("https://cdn.agentclientprotocol.com/registry/")?;
        let filename = path.rsplit('/').next()?;
        let id = filename.strip_suffix(".svg")?;
        CATALOG
            .logos
            .iter()
            .find(|logo| logo.id == id || logo.aliases.iter().any(|alias| alias == id))
    }

    pub fn source(&self) -> String {
        let mut svg = self.svg.clone();
        if svg.contains("currentColor")
            && let Some(root) = svg.find("<svg")
            && let Some(end) = svg[root..].find('>')
        {
            svg.insert_str(root + end + 1, "<style>svg{color:#52525b}@media(prefers-color-scheme:dark){svg{color:#d4d4d8}}</style>");
        }
        format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_brand_assets_are_offline_and_keep_their_upstream_colors() {
        for (id, color) in [
            ("claude", "#D97757"),
            ("codex-acp", "#3941FF"),
            ("gemini-cli", "#3186FF"),
            ("mistral-vibe", "#FA500F"),
        ] {
            let source = Logo::of(id).unwrap().source();
            let data = source.strip_prefix("data:image/svg+xml;base64,").unwrap();
            let svg = String::from_utf8(
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .unwrap(),
            )
            .unwrap();
            assert!(
                svg.contains(color),
                "upstream brand color is missing for {id}"
            );
            assert!(svg.contains("<svg"));
        }
    }

    #[test]
    fn catalog_is_complete_unique_and_keeps_unmatched_agent_shapes() {
        let expected = [
            "agoragentic-acp",
            "amp-acp",
            "antigravity-acp",
            "auggie",
            "autohand",
            "bob",
            "claude-acp",
            "cline",
            "codebuddy-code",
            "codex-acp",
            "cortex-code",
            "corust-agent",
            "crow-cli",
            "cursor",
            "deepagents",
            "devin",
            "dimcode",
            "dirac",
            "factory-droid",
            "fast-agent",
            "gemini",
            "github-copilot-cli",
            "glm-acp-agent",
            "goose",
            "grok-build",
            "harn",
            "junie",
            "kilo",
            "kimchi",
            "kimi",
            "kimi-code",
            "kiro",
            "minimax-code",
            "minion-code",
            "mistral-vibe",
            "nova",
            "opencode",
            "opencode-v2",
            "pi-acp",
            "poolside",
            "qoder",
            "qwen-code",
            "sigit",
            "stakpak",
            "vtcode",
        ];
        let actual: Vec<_> = CATALOG.logos.iter().map(|logo| logo.id.as_str()).collect();
        assert_eq!(actual, expected);
        for id in ["bob", "nova", "pi-acp"] {
            let logo = Logo::of(id).unwrap();
            assert!(logo.svg.contains("currentColor"));
            let encoded = logo.source();
            let svg = base64::engine::general_purpose::STANDARD
                .decode(encoded.split(',').nth(1).unwrap())
                .unwrap();
            assert!(
                String::from_utf8(svg)
                    .unwrap()
                    .contains("prefers-color-scheme:dark")
            );
        }
        assert!(Logo::of("unknown-provider").is_none());
    }
}
