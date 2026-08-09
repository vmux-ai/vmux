#[derive(Clone, Copy)]
pub struct AgentAccent {
    pub glow_top: &'static str,
    pub glow_bottom: &'static str,
    pub grad: &'static str,
    pub accent_text: &'static str,
    pub accent_bg: &'static str,
    pub cta_shadow: &'static str,
    pub rain_rgb: &'static str,
}

pub fn agent_accent(segment: &str) -> AgentAccent {
    match segment {
        "claude" | "claude-acp" => AgentAccent {
            glow_top: "pointer-events-none absolute -top-1/3 left-1/2 h-[60vh] w-[60vh] -translate-x-1/2 rounded-full bg-rose-500/20 blur-[120px]",
            glow_bottom: "pointer-events-none absolute -bottom-1/4 right-1/4 h-[44vh] w-[44vh] rounded-full bg-orange-400/10 blur-[120px]",
            grad: "from-orange-400 to-rose-500",
            accent_text: "text-rose-600 dark:text-rose-400",
            accent_bg: "bg-rose-400",
            cta_shadow: "shadow-lg shadow-rose-500/25 hover:shadow-rose-500/40",
            rain_rgb: "251 113 133",
        },
        "codex" | "codex-acp" => AgentAccent {
            glow_top: "pointer-events-none absolute -top-1/3 left-1/2 h-[60vh] w-[60vh] -translate-x-1/2 rounded-full bg-emerald-500/20 blur-[120px]",
            glow_bottom: "pointer-events-none absolute -bottom-1/4 right-1/4 h-[44vh] w-[44vh] rounded-full bg-teal-400/10 blur-[120px]",
            grad: "from-emerald-500 to-teal-600",
            accent_text: "text-emerald-600 dark:text-emerald-400",
            accent_bg: "bg-emerald-400",
            cta_shadow: "shadow-lg shadow-emerald-500/25 hover:shadow-emerald-500/40",
            rain_rgb: "52 211 153",
        },
        "terminal" => AgentAccent {
            glow_top: "pointer-events-none absolute -top-1/3 left-1/2 h-[60vh] w-[60vh] -translate-x-1/2 rounded-full bg-[#00ff41]/20 blur-[120px]",
            glow_bottom: "pointer-events-none absolute -bottom-1/4 right-1/4 h-[44vh] w-[44vh] rounded-full bg-[#00ff41]/10 blur-[120px]",
            grad: "from-[#00ff41] to-[#008f11]",
            accent_text: "text-[#00a82d] dark:text-[#00ff41]",
            accent_bg: "bg-[#00ff41]",
            cta_shadow: "shadow-lg shadow-[#00ff41]/25 hover:shadow-[#00ff41]/40",
            rain_rgb: "0 255 65",
        },
        _ => AgentAccent {
            glow_top: "pointer-events-none absolute -top-1/3 left-1/2 h-[60vh] w-[60vh] -translate-x-1/2 rounded-full bg-orange-500/20 blur-[120px]",
            glow_bottom: "pointer-events-none absolute -bottom-1/4 right-1/4 h-[44vh] w-[44vh] rounded-full bg-amber-400/10 blur-[120px]",
            grad: "from-orange-500 to-amber-600",
            accent_text: "text-orange-600 dark:text-orange-400",
            accent_bg: "bg-orange-400",
            cta_shadow: "shadow-lg shadow-orange-500/25 hover:shadow-orange-500/40",
            rain_rgb: "251 146 60",
        },
    }
}

#[cfg(test)]
#[path = "agent_accent.test.rs"]
mod tests;
