use super::*;

#[test]
fn claude_uses_rose_orange() {
    for id in ["claude", "claude-acp"] {
        let a = agent_accent(id);
        assert_eq!(a.grad, "from-orange-400 to-rose-500");
        assert_eq!(a.accent_text, "text-rose-600 dark:text-rose-400");
        assert_eq!(a.accent_bg, "bg-rose-400");
        assert_eq!(a.rain_rgb, "251 113 133");
    }
}

#[test]
fn codex_uses_emerald_teal() {
    for id in ["codex", "codex-acp"] {
        let a = agent_accent(id);
        assert_eq!(a.grad, "from-emerald-500 to-teal-600");
        assert_eq!(a.accent_text, "text-emerald-600 dark:text-emerald-400");
        assert_eq!(a.rain_rgb, "52 211 153");
    }
}

#[test]
fn terminal_uses_green() {
    let a = agent_accent("terminal");
    assert_eq!(a.accent_text, "text-[#00a82d] dark:text-[#00ff41]");
    assert_eq!(a.rain_rgb, "0 255 65");
}

#[test]
fn unknown_falls_back_to_vibe_amber() {
    let a = agent_accent("nope");
    assert_eq!(a.grad, "from-orange-500 to-amber-600");
    assert_eq!(a.grad, agent_accent("vibe").grad);
    assert_eq!(a.rain_rgb, "251 146 60");
}
