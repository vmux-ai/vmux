pub fn supports_inline_agent_transition(url: &str) -> bool {
    let Some(route) = crate::VmuxRoute::parse(url) else {
        return false;
    };
    if !route.is_agent() {
        return false;
    }
    let segments = route.path_segments().collect::<Vec<_>>();
    match segments.as_slice() {
        [] | [_] => true,
        [_, session] => !matches!(*session, "cli" | "setup"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_default_fresh_and_resumed_acp_routes() {
        assert!(supports_inline_agent_transition("vmux://sessions/"));
        assert!(supports_inline_agent_transition(
            "vmux://sessions/mistral-vibe"
        ));
        assert!(supports_inline_agent_transition(
            "vmux://sessions/mistral-vibe/session-1"
        ));
    }

    #[test]
    fn rejects_removed_or_invalid_routes() {
        assert!(!supports_inline_agent_transition(
            "vmux://sessions/codex/cli"
        ));
        assert!(!supports_inline_agent_transition(
            "vmux://sessions/vibe/setup"
        ));
        assert!(!supports_inline_agent_transition(
            "vmux://sessions/provider/model/session"
        ));
        assert!(!supports_inline_agent_transition("https://example.com"));
    }
}
