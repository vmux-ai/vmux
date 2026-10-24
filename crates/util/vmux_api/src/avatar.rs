use base64::Engine;

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
        match Self::brand_color(segment) {
            Some(color) => color.to_string(),
            None => Self::color(segment),
        }
    }

    pub fn brand_color(segment: &str) -> Option<&'static str> {
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
}
const USER_COLOR: &str = "#3b82f6";

const IDENTICON_PALETTES: [(&str, &str, &str); 6] = [
    ("#fff1f2", "#e11d48", "#fb7185"),
    ("#fff7ed", "#ea580c", "#fb923c"),
    ("#fefce8", "#ca8a04", "#facc15"),
    ("#f0fdf4", "#16a34a", "#4ade80"),
    ("#ecfeff", "#0891b2", "#22d3ee"),
    ("#f5f3ff", "#7c3aed", "#a78bfa"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct IdenticonSpec {
    pub background: &'static str,
    pub foreground: &'static str,
    pub accent: &'static str,
    pub tiles: [bool; 9],
}
impl IdenticonSpec {
    pub fn from_seed(seed: &str) -> Self {
        let mut random = IdenticonRandom::from(seed);
        let (background, foreground, accent) =
            IDENTICON_PALETTES[random.below(IDENTICON_PALETTES.len())];
        let mut tiles = [false; 9];
        for row in 0..3 {
            for column in 0..2 {
                let visible = random.yes();
                tiles[row * 3 + column] = visible;
                tiles[row * 3 + (2 - column)] = visible;
            }
        }
        tiles[4] = true;
        Self {
            background,
            foreground,
            accent,
            tiles,
        }
    }

    pub fn favicon(seed: &str) -> String {
        let identicon = Self::from_seed(seed);
        let mut shapes = String::new();
        for (index, visible) in identicon.tiles.iter().copied().enumerate() {
            if !visible {
                continue;
            }
            let color = if index % 2 == 0 {
                identicon.foreground
            } else {
                identicon.accent
            };
            shapes.push_str(&format!(
                r#"<rect x="{}" y="{}" width="8" height="8" rx="2.25" fill="{}"/>"#,
                Self::coordinate(index % 3),
                Self::coordinate(index / 3),
                color
            ));
        }
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36"><rect width="36" height="36" rx="8" fill="{}"/>{shapes}</svg>"#,
            identicon.background
        );
        format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg)
        )
    }

    pub fn coordinate(index: usize) -> usize {
        5 + index * 9
    }
}

struct IdenticonRandom(u64);

impl From<&str> for IdenticonRandom {
    fn from(seed: &str) -> Self {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in seed.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Self(hash)
    }
}

impl IdenticonRandom {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, ceiling: usize) -> usize {
        self.next() as usize % ceiling
    }

    fn yes(&mut self) -> bool {
        self.below(2) == 0
    }
}

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
        assert!(AvatarSpec::brand_color("some-acp-agent").is_none());
    }

    #[test]
    fn identicon_is_stable_for_one_project_identity() {
        assert_eq!(
            IdenticonSpec::from_seed("vmux://projects/github.com/vmux-ai/vmux"),
            IdenticonSpec::from_seed("vmux://projects/github.com/vmux-ai/vmux")
        );
        assert_ne!(
            IdenticonSpec::from_seed("vmux://projects/github.com/vmux-ai/vmux"),
            IdenticonSpec::from_seed("vmux://projects/gitlab.com/vmux-ai/vmux")
        );
    }

    #[test]
    fn identicon_favicon_is_an_svg_data_url() {
        assert!(
            IdenticonSpec::favicon("vmux://projects/github.com/vmux-ai/vmux")
                .starts_with("data:image/svg+xml;base64,")
        );
    }
}
