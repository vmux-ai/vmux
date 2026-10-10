#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AvatarSpec {
    pub initials: String,
    pub color: String,
}

impl AvatarSpec {
    pub fn for_user() -> Self {
        Self {
            initials: "You".into(),
            color: USER_COLOR.into(),
        }
    }

    pub fn for_user_named(name: &str) -> Self {
        Self {
            initials: Self::initials(name),
            color: USER_COLOR.into(),
        }
    }

    pub fn for_registry(name: &str, seed: &str) -> Self {
        Self {
            initials: Self::initials(name),
            color: Self::agent_color(seed),
        }
    }

    pub fn agent_color(segment: &str) -> String {
        match Self::agent_segment_color(segment) {
            Some(color) => color.to_string(),
            None => Self::color(segment),
        }
    }

    pub fn initials(name: &str) -> String {
        let initials: String = name
            .split(|character: char| !character.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .take(2)
            .filter_map(|word| word.chars().next())
            .map(|character| character.to_ascii_uppercase())
            .collect();
        if initials.is_empty() {
            "?".to_string()
        } else {
            initials
        }
    }

    pub fn color(seed: &str) -> String {
        const PALETTE: [&str; 8] = [
            "#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#3b82f6", "#8b5cf6", "#ec4899",
        ];
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in seed.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        PALETTE[(hash % PALETTE.len() as u64) as usize].to_string()
    }

    fn agent_segment_color(segment: &str) -> Option<&'static str> {
        let segment = segment.to_ascii_lowercase();
        if segment.contains("claude") {
            return Some("#d97757");
        }
        if segment.contains("codex") {
            return Some("#10a37f");
        }
        if segment.contains("mistral") || segment.contains("vibe") {
            return Some("#ff7000");
        }
        None
    }
}

const USER_COLOR: &str = "#3b82f6";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_agents_keep_their_brand_colours() {
        assert_eq!(AvatarSpec::agent_color("claude"), "#d97757");
        assert_eq!(AvatarSpec::agent_color("Claude Agent"), "#d97757");
        assert_eq!(AvatarSpec::agent_color("codex"), "#10a37f");
        assert_eq!(AvatarSpec::agent_color("codex-acp"), "#10a37f");
        assert_eq!(AvatarSpec::agent_color("vibe"), "#ff7000");
        assert_eq!(AvatarSpec::agent_color("mistral"), "#ff7000");
        assert_eq!(AvatarSpec::agent_color("Mistral Vibe"), "#ff7000");
    }

    #[test]
    fn a_registry_agent_hashes_to_a_stable_palette_colour() {
        let first = AvatarSpec::agent_color("some-acp-agent");
        assert_eq!(first, AvatarSpec::agent_color("some-acp-agent"));
        assert_ne!(first, AvatarSpec::agent_color("another-acp-agent"));
        assert!(AvatarSpec::agent_segment_color("some-acp-agent").is_none());
    }
}
