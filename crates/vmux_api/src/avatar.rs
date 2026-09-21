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
            initials: initials_of(name),
            color: USER_COLOR.into(),
        }
    }

    pub fn for_registry(name: &str, seed: &str) -> Self {
        Self {
            initials: initials_of(name),
            color: hash_color(seed),
        }
    }
}

const USER_COLOR: &str = "#3b82f6";

pub fn agent_segment_color(segment: &str) -> Option<&'static str> {
    match segment {
        "claude" => Some("#d97757"),
        "codex" => Some("#10a37f"),
        "vibe" => Some("#7c3aed"),
        _ => None,
    }
}

pub fn agent_color(segment: &str) -> String {
    match agent_segment_color(segment) {
        Some(color) => color.to_string(),
        None => hash_color(segment),
    }
}

pub fn initials_of(name: &str) -> String {
    let initials: String = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(2)
        .filter_map(|word| word.chars().next())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if initials.is_empty() {
        "?".to_string()
    } else {
        initials
    }
}

pub fn hash_color(seed: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_agents_keep_their_brand_colours() {
        assert_eq!(agent_color("claude"), "#d97757");
        assert_eq!(agent_color("codex"), "#10a37f");
        assert_eq!(agent_color("vibe"), "#7c3aed");
    }

    #[test]
    fn a_registry_agent_hashes_to_a_stable_palette_colour() {
        let first = agent_color("some-acp-agent");
        assert_eq!(first, agent_color("some-acp-agent"));
        assert_ne!(first, agent_color("another-acp-agent"));
        assert!(agent_segment_color("some-acp-agent").is_none());
    }
}
