use std::borrow::Cow;

use base64::Engine;
use dioxus::prelude::*;

use crate::cn::cn;

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum AgentIconVariant {
    #[default]
    Color,
    Mono,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[vmux_api::variant_names]
pub enum AgentIcon {
    Amp,
    Antigravity,
    Anthropic,
    Claude,
    Cline,
    CodeBuddy,
    Codex,
    Cursor,
    DeepSeek,
    Devin,
    Gemini,
    GitHubCopilot,
    Glm,
    Google,
    Goose,
    Grok,
    HuggingFace,
    Junie,
    KiloCode,
    Kimi,
    Kiro,
    Lovable,
    MetaAi,
    Minimax,
    Mistral,
    Nova,
    Ollama,
    OpenAi,
    OpenCode,
    Perplexity,
    Poolside,
    Qoder,
    Qwen,
    Replit,
    RooCode,
    Snowflake,
    Trae,
    V0,
    Windsurf,
}

impl AgentIcon {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "Amp" => Some(Self::Amp),
            "Antigravity" => Some(Self::Antigravity),
            "Anthropic" => Some(Self::Anthropic),
            "Claude" => Some(Self::Claude),
            "Cline" => Some(Self::Cline),
            "CodeBuddy" => Some(Self::CodeBuddy),
            "Codex" => Some(Self::Codex),
            "Cursor" => Some(Self::Cursor),
            "DeepSeek" => Some(Self::DeepSeek),
            "Devin" => Some(Self::Devin),
            "Gemini" => Some(Self::Gemini),
            "GitHubCopilot" => Some(Self::GitHubCopilot),
            "Glm" => Some(Self::Glm),
            "Google" => Some(Self::Google),
            "Goose" => Some(Self::Goose),
            "Grok" => Some(Self::Grok),
            "HuggingFace" => Some(Self::HuggingFace),
            "Junie" => Some(Self::Junie),
            "KiloCode" => Some(Self::KiloCode),
            "Kimi" => Some(Self::Kimi),
            "Kiro" => Some(Self::Kiro),
            "Lovable" => Some(Self::Lovable),
            "MetaAi" => Some(Self::MetaAi),
            "Minimax" => Some(Self::Minimax),
            "Mistral" => Some(Self::Mistral),
            "Nova" => Some(Self::Nova),
            "Ollama" => Some(Self::Ollama),
            "OpenAi" => Some(Self::OpenAi),
            "OpenCode" => Some(Self::OpenCode),
            "Perplexity" => Some(Self::Perplexity),
            "Poolside" => Some(Self::Poolside),
            "Qoder" => Some(Self::Qoder),
            "Qwen" => Some(Self::Qwen),
            "Replit" => Some(Self::Replit),
            "RooCode" => Some(Self::RooCode),
            "Snowflake" => Some(Self::Snowflake),
            "Trae" => Some(Self::Trae),
            "V0" => Some(Self::V0),
            "Windsurf" => Some(Self::Windsurf),
            _ => None,
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        let lower = value.trim().to_ascii_lowercase();
        if lower.is_empty() {
            return None;
        }
        if let Some(exact) = match lower.as_str() {
            "claude-acp" | "claude-code" | "claudecode" => Some(Self::Claude),
            "codex-acp" => Some(Self::Codex),
            "gemini-cli" | "geminicli" => Some(Self::Gemini),
            "mistral-vibe" => Some(Self::Mistral),
            "github-copilot-cli" | "github-copilot" | "githubcopilot" => Some(Self::GitHubCopilot),
            "glm-acp-agent" | "chatglm" | "glmv" => Some(Self::Glm),
            "cortex-code" => Some(Self::Snowflake),
            "opencode-v2" => Some(Self::OpenCode),
            "kimi-code" => Some(Self::Kimi),
            "minimax-code" => Some(Self::Minimax),
            "qwen-code" => Some(Self::Qwen),
            "codebuddy-code" => Some(Self::CodeBuddy),
            "grok-build" => Some(Self::Grok),
            "amp-acp" => Some(Self::Amp),
            "antigravity-acp" => Some(Self::Antigravity),
            _ => None,
        } {
            return Some(exact);
        }
        for token in lower.split(|character: char| !character.is_ascii_alphanumeric()) {
            let matched = match token {
                "amp" => Some(Self::Amp),
                "antigravity" => Some(Self::Antigravity),
                "anthropic" => Some(Self::Anthropic),
                "claude" | "claudecode" => Some(Self::Claude),
                "cline" => Some(Self::Cline),
                "codebuddy" => Some(Self::CodeBuddy),
                "codex" => Some(Self::Codex),
                "cursor" => Some(Self::Cursor),
                "deepseek" => Some(Self::DeepSeek),
                "devin" => Some(Self::Devin),
                "gemini" | "geminicli" => Some(Self::Gemini),
                "copilot" | "githubcopilot" => Some(Self::GitHubCopilot),
                "glm" | "glmv" | "chatglm" | "zhipu" => Some(Self::Glm),
                "google" => Some(Self::Google),
                "goose" => Some(Self::Goose),
                "grok" | "xai" => Some(Self::Grok),
                "huggingface" => Some(Self::HuggingFace),
                "junie" => Some(Self::Junie),
                "kilo" | "kilocode" => Some(Self::KiloCode),
                "kimi" | "moonshot" => Some(Self::Kimi),
                "kiro" => Some(Self::Kiro),
                "lovable" => Some(Self::Lovable),
                "meta" | "metaai" | "llama" => Some(Self::MetaAi),
                "minimax" => Some(Self::Minimax),
                "mistral" | "vibe" => Some(Self::Mistral),
                "nova" => Some(Self::Nova),
                "ollama" => Some(Self::Ollama),
                "openai" => Some(Self::OpenAi),
                "opencode" => Some(Self::OpenCode),
                "perplexity" => Some(Self::Perplexity),
                "poolside" => Some(Self::Poolside),
                "qoder" => Some(Self::Qoder),
                "qwen" => Some(Self::Qwen),
                "replit" => Some(Self::Replit),
                "roo" | "roocode" => Some(Self::RooCode),
                "snowflake" | "cortex" => Some(Self::Snowflake),
                "trae" => Some(Self::Trae),
                "v0" => Some(Self::V0),
                "windsurf" | "codeium" => Some(Self::Windsurf),
                _ => None,
            };
            if matched.is_some() {
                return matched;
            }
        }
        None
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Amp => "Amp",
            Self::Antigravity => "Antigravity",
            Self::Anthropic => "Anthropic",
            Self::Claude => "Claude",
            Self::Cline => "Cline",
            Self::CodeBuddy => "CodeBuddy",
            Self::Codex => "Codex",
            Self::Cursor => "Cursor",
            Self::DeepSeek => "DeepSeek",
            Self::Devin => "Devin",
            Self::Gemini => "Gemini",
            Self::GitHubCopilot => "GitHub Copilot",
            Self::Glm => "GLM",
            Self::Google => "Google",
            Self::Goose => "Goose",
            Self::Grok => "Grok",
            Self::HuggingFace => "Hugging Face",
            Self::Junie => "Junie",
            Self::KiloCode => "Kilo Code",
            Self::Kimi => "Kimi",
            Self::Kiro => "Kiro",
            Self::Lovable => "Lovable",
            Self::MetaAi => "Meta AI",
            Self::Minimax => "MiniMax",
            Self::Mistral => "Mistral",
            Self::Nova => "Nova",
            Self::Ollama => "Ollama",
            Self::OpenAi => "OpenAI",
            Self::OpenCode => "OpenCode",
            Self::Perplexity => "Perplexity",
            Self::Poolside => "Poolside",
            Self::Qoder => "Qoder",
            Self::Qwen => "Qwen",
            Self::Replit => "Replit",
            Self::RooCode => "Roo Code",
            Self::Snowflake => "Snowflake",
            Self::Trae => "Trae",
            Self::V0 => "v0",
            Self::Windsurf => "Windsurf",
        }
    }

    pub fn has_color(self) -> bool {
        self.color_svg().is_some()
    }

    pub fn mono_svg(self) -> &'static str {
        match self {
            Self::Amp => include_str!("../assets/agent-icons/amp.svg"),
            Self::Antigravity => include_str!("../assets/agent-icons/antigravity.svg"),
            Self::Anthropic => include_str!("../assets/agent-icons/anthropic.svg"),
            Self::Claude => include_str!("../assets/agent-icons/claude.svg"),
            Self::Cline => include_str!("../assets/agent-icons/cline.svg"),
            Self::CodeBuddy => include_str!("../assets/agent-icons/codebuddy.svg"),
            Self::Codex => include_str!("../assets/agent-icons/codex.svg"),
            Self::Cursor => include_str!("../assets/agent-icons/cursor.svg"),
            Self::DeepSeek => include_str!("../assets/agent-icons/deepseek.svg"),
            Self::Devin => include_str!("../assets/agent-icons/devin.svg"),
            Self::Gemini => include_str!("../assets/agent-icons/gemini.svg"),
            Self::GitHubCopilot => include_str!("../assets/agent-icons/githubcopilot.svg"),
            Self::Glm => include_str!("../assets/agent-icons/glmv.svg"),
            Self::Google => include_str!("../assets/agent-icons/google.svg"),
            Self::Goose => include_str!("../assets/agent-icons/goose.svg"),
            Self::Grok => include_str!("../assets/agent-icons/grok.svg"),
            Self::HuggingFace => include_str!("../assets/agent-icons/huggingface.svg"),
            Self::Junie => include_str!("../assets/agent-icons/junie.svg"),
            Self::KiloCode => include_str!("../assets/agent-icons/kilocode.svg"),
            Self::Kimi => include_str!("../assets/agent-icons/kimi.svg"),
            Self::Kiro => include_str!("../assets/agent-icons/kiro.svg"),
            Self::Lovable => include_str!("../assets/agent-icons/lovable.svg"),
            Self::MetaAi => include_str!("../assets/agent-icons/metaai.svg"),
            Self::Minimax => include_str!("../assets/agent-icons/minimax.svg"),
            Self::Mistral => include_str!("../assets/agent-icons/mistral.svg"),
            Self::Nova => include_str!("../assets/agent-icons/nova.svg"),
            Self::Ollama => include_str!("../assets/agent-icons/ollama.svg"),
            Self::OpenAi => include_str!("../assets/agent-icons/openai.svg"),
            Self::OpenCode => include_str!("../assets/agent-icons/opencode.svg"),
            Self::Perplexity => include_str!("../assets/agent-icons/perplexity.svg"),
            Self::Poolside => include_str!("../assets/agent-icons/poolside.svg"),
            Self::Qoder => include_str!("../assets/agent-icons/qoder.svg"),
            Self::Qwen => include_str!("../assets/agent-icons/qwen.svg"),
            Self::Replit => include_str!("../assets/agent-icons/replit.svg"),
            Self::RooCode => include_str!("../assets/agent-icons/roocode.svg"),
            Self::Snowflake => include_str!("../assets/agent-icons/snowflake.svg"),
            Self::Trae => include_str!("../assets/agent-icons/trae.svg"),
            Self::V0 => include_str!("../assets/agent-icons/v0.svg"),
            Self::Windsurf => include_str!("../assets/agent-icons/windsurf.svg"),
        }
    }

    pub fn color_svg(self) -> Option<&'static str> {
        match self {
            Self::Amp => Some(include_str!("../assets/agent-icons/amp-color.svg")),
            Self::Antigravity => Some(include_str!("../assets/agent-icons/antigravity-color.svg")),
            Self::Claude => Some(include_str!("../assets/agent-icons/claude-color.svg")),
            Self::CodeBuddy => Some(include_str!("../assets/agent-icons/codebuddy-color.svg")),
            Self::Codex => Some(include_str!("../assets/agent-icons/codex-color.svg")),
            Self::DeepSeek => Some(include_str!("../assets/agent-icons/deepseek-color.svg")),
            Self::Devin => Some(include_str!("../assets/agent-icons/devin-color.svg")),
            Self::Gemini => Some(include_str!("../assets/agent-icons/gemini-color.svg")),
            Self::Glm => Some(include_str!("../assets/agent-icons/glmv-color.svg")),
            Self::Google => Some(include_str!("../assets/agent-icons/google-color.svg")),
            Self::HuggingFace => Some(include_str!("../assets/agent-icons/huggingface-color.svg")),
            Self::Junie => Some(include_str!("../assets/agent-icons/junie-color.svg")),
            Self::Kimi => Some(include_str!("../assets/agent-icons/kimi-color.svg")),
            Self::Kiro => Some(include_str!("../assets/agent-icons/kiro-color.svg")),
            Self::Lovable => Some(include_str!("../assets/agent-icons/lovable-color.svg")),
            Self::MetaAi => Some(include_str!("../assets/agent-icons/metaai-color.svg")),
            Self::Minimax => Some(include_str!("../assets/agent-icons/minimax-color.svg")),
            Self::Mistral => Some(include_str!("../assets/agent-icons/mistral-color.svg")),
            Self::Nova => Some(include_str!("../assets/agent-icons/nova-color.svg")),
            Self::Perplexity => Some(include_str!("../assets/agent-icons/perplexity-color.svg")),
            Self::Poolside => Some(include_str!("../assets/agent-icons/poolside-color.svg")),
            Self::Qoder => Some(include_str!("../assets/agent-icons/qoder-color.svg")),
            Self::Qwen => Some(include_str!("../assets/agent-icons/qwen-color.svg")),
            Self::Replit => Some(include_str!("../assets/agent-icons/replit-color.svg")),
            Self::Snowflake => Some(include_str!("../assets/agent-icons/snowflake-color.svg")),
            Self::Trae => Some(include_str!("../assets/agent-icons/trae-color.svg")),
            _ => None,
        }
    }

    pub fn svg(self, variant: AgentIconVariant) -> &'static str {
        match variant {
            AgentIconVariant::Color => self.color_svg().unwrap_or_else(|| self.mono_svg()),
            AgentIconVariant::Mono => self.mono_svg(),
        }
    }

    pub fn scoped_svg(self, variant: AgentIconVariant, scope: usize) -> Cow<'static, str> {
        let markup = self.svg(variant);
        if !markup.contains("url(#") {
            return Cow::Borrowed(markup);
        }
        let prefix = format!("ai-{scope}-");
        let rewritten = markup
            .replace("id=\"", &format!("id=\"{prefix}"))
            .replace("url(#", &format!("url(#{prefix}"));
        Cow::Owned(rewritten)
    }

    pub fn data_uri(self, variant: AgentIconVariant) -> String {
        let raw = self.svg(variant);
        let standalone = if raw.contains("currentColor") {
            Cow::Owned(raw.replace("currentColor", "#8b949e"))
        } else {
            Cow::Borrowed(raw)
        };
        format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(standalone.as_bytes())
        )
    }
}

#[component]
pub fn AgentIconView(
    icon: AgentIcon,
    #[props(default)] variant: AgentIconVariant,
    #[props(default)] class: String,
) -> Element {
    let scope = use_hook(|| dioxus::core::current_scope_id().0);
    let markup = icon.scoped_svg(variant, scope);
    let class = cn([
        "inline-flex shrink-0 items-center justify-center [&>svg]:size-full",
        class.as_str(),
    ]);
    rsx! {
        span {
            class,
            role: "img",
            aria_label: icon.label(),
            dangerous_inner_html: "{markup}",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_declared_agent_icon_parses_and_provides_valid_svg() {
        for &name in AgentIcon::VARIANT_NAMES {
            let icon = AgentIcon::parse(name).expect(name);
            let mono = icon.mono_svg();
            assert!(mono.contains("<svg"), "{name} mono");
            assert!(mono.contains("viewBox="), "{name} mono viewBox");
            assert!(mono.contains("currentColor"), "{name} mono currentColor");
            if let Some(color) = icon.color_svg() {
                assert!(color.contains("<svg"), "{name} color");
                assert!(color.contains("viewBox="), "{name} color viewBox");
            }
            assert!(
                icon.data_uri(AgentIconVariant::Color)
                    .starts_with("data:image/svg+xml;base64,"),
                "{name} data uri"
            );
        }
    }

    #[test]
    fn resolves_acp_registry_ids_and_rejects_partial_word_matches() {
        for (id, expected) in [
            ("claude-acp", AgentIcon::Claude),
            ("codex-acp", AgentIcon::Codex),
            ("gemini", AgentIcon::Gemini),
            ("mistral-vibe", AgentIcon::Mistral),
            ("amp-acp", AgentIcon::Amp),
            ("antigravity-acp", AgentIcon::Antigravity),
            ("cline", AgentIcon::Cline),
            ("codebuddy-code", AgentIcon::CodeBuddy),
            ("cortex-code", AgentIcon::Snowflake),
            ("cursor", AgentIcon::Cursor),
            ("devin", AgentIcon::Devin),
            ("github-copilot-cli", AgentIcon::GitHubCopilot),
            ("glm-acp-agent", AgentIcon::Glm),
            ("goose", AgentIcon::Goose),
            ("grok-build", AgentIcon::Grok),
            ("junie", AgentIcon::Junie),
            ("kilo", AgentIcon::KiloCode),
            ("kimi-code", AgentIcon::Kimi),
            ("kiro", AgentIcon::Kiro),
            ("minimax-code", AgentIcon::Minimax),
            ("nova", AgentIcon::Nova),
            ("opencode", AgentIcon::OpenCode),
            ("opencode-v2", AgentIcon::OpenCode),
            ("poolside", AgentIcon::Poolside),
            ("qoder", AgentIcon::Qoder),
            ("qwen-code", AgentIcon::Qwen),
        ] {
            assert_eq!(AgentIcon::from_id(id), Some(expected), "{id}");
        }
        assert_eq!(AgentIcon::from_id("claudex"), None);
        assert_eq!(AgentIcon::from_id("unknown-agent"), None);
        assert_eq!(AgentIcon::from_id(""), None);
    }

    #[test]
    fn scoped_svg_isolates_gradient_ids_per_component_instance() {
        let first = AgentIcon::Gemini.scoped_svg(AgentIconVariant::Color, 7);
        let second = AgentIcon::Gemini.scoped_svg(AgentIconVariant::Color, 8);
        assert!(first.contains("id=\"ai-7-"));
        assert!(first.contains("url(#ai-7-"));
        assert!(second.contains("id=\"ai-8-"));
        assert!(second.contains("url(#ai-8-"));
        assert_ne!(first, second);
    }
}
