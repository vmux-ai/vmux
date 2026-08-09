use vmux_core::agent::AgentKind;

#[derive(Copy, Clone, Debug)]
pub struct BuiltinProvider {
    pub provider: &'static str,
    pub kind: AgentKind,
    pub default_model: &'static str,
    pub env_var: &'static str,
}

pub const ECHO_DEFAULT: BuiltinProvider = BuiltinProvider {
    provider: "echo",
    kind: AgentKind::Vibe,
    default_model: "echo",
    env_var: "",
};

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
        .or(Some(&ECHO_DEFAULT))
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
    fn no_keys_returns_echo_fallback() {
        clear_all_keys();
        let p = resolve_default_app_provider().unwrap();
        assert_eq!(p.provider, "echo");
    }
}
