use std::sync::Arc;

use crate::AgentKind;
use crate::client::page::strategy::AgentPageStrategy;
use crate::providers::anthropic::AnthropicStrategy;
use crate::providers::mistral::MistralStrategy;
use crate::providers::openai::OpenAiResponsesStrategy;

#[derive(Copy, Clone, Debug)]
pub struct BuiltinProvider {
    pub provider: &'static str,
    pub kind: AgentKind,
    pub default_model: &'static str,
    pub env_var: &'static str,
}

pub const BUILTIN_PROVIDERS: &[BuiltinProvider] = &[
    BuiltinProvider {
        provider: "mistral",
        kind: AgentKind::Vibe,
        default_model: "devstral-2",
        env_var: "MISTRAL_API_KEY",
    },
    BuiltinProvider {
        provider: "anthropic",
        kind: AgentKind::Claude,
        default_model: "claude-sonnet-4-6",
        env_var: "ANTHROPIC_API_KEY",
    },
    BuiltinProvider {
        provider: "openai",
        kind: AgentKind::Codex,
        default_model: "gpt-5",
        env_var: "OPENAI_API_KEY",
    },
];

pub fn resolve_default_app_provider() -> Option<&'static BuiltinProvider> {
    BUILTIN_PROVIDERS
        .iter()
        .find(|p| std::env::var(p.env_var).is_ok())
}

pub fn instantiate_builtin(p: &BuiltinProvider, model: &str) -> Arc<dyn AgentPageStrategy> {
    match p.provider {
        "mistral" => Arc::new(MistralStrategy::new(p.provider, model.to_string())),
        "anthropic" => Arc::new(AnthropicStrategy::new(p.provider, model.to_string())),
        "openai" => Arc::new(OpenAiResponsesStrategy::new(p.provider, model.to_string())),
        other => panic!("instantiate_builtin: unknown provider '{other}'"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn clear_all_keys() {
        for p in BUILTIN_PROVIDERS {
            unsafe { std::env::remove_var(p.env_var) };
        }
    }

    #[test]
    #[serial]
    fn priority_is_mistral_then_anthropic_then_openai() {
        clear_all_keys();
        unsafe { std::env::set_var("MISTRAL_API_KEY", "x") };
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "y") };
        unsafe { std::env::set_var("OPENAI_API_KEY", "z") };
        let p = resolve_default_app_provider().unwrap();
        assert_eq!(p.provider, "mistral");
        clear_all_keys();
    }

    #[test]
    #[serial]
    fn anthropic_wins_when_mistral_absent() {
        clear_all_keys();
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "y") };
        unsafe { std::env::set_var("OPENAI_API_KEY", "z") };
        let p = resolve_default_app_provider().unwrap();
        assert_eq!(p.provider, "anthropic");
        clear_all_keys();
    }

    #[test]
    #[serial]
    fn no_keys_returns_none() {
        clear_all_keys();
        assert!(resolve_default_app_provider().is_none());
    }

    #[test]
    fn instantiate_returns_correct_strategy_type() {
        let bp = &BUILTIN_PROVIDERS[0];
        let s = instantiate_builtin(bp, "devstral-2");
        assert_eq!(s.provider(), "mistral");
        assert_eq!(s.model(), "devstral-2");
    }
}
