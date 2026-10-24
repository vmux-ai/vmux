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

const IDENTICON_PALETTES: [[&str; 5]; 6] = [
    ["#fff1f2", "#e11d48", "#fb7185", "#be123c", "#fecdd3"],
    ["#fff7ed", "#ea580c", "#fb923c", "#c2410c", "#fed7aa"],
    ["#fefce8", "#ca8a04", "#facc15", "#a16207", "#fef08a"],
    ["#f0fdf4", "#16a34a", "#4ade80", "#15803d", "#bbf7d0"],
    ["#ecfeff", "#0891b2", "#22d3ee", "#0e7490", "#a5f3fc"],
    ["#f5f3ff", "#7c3aed", "#a78bfa", "#6d28d9", "#ddd6fe"],
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdenticonSpec {
    pub background: &'static str,
    pub bar: IdenticonShape,
    pub circle: IdenticonShape,
    pub line: IdenticonShape,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdenticonShape {
    pub color: &'static str,
    pub translate_x: i32,
    pub translate_y: i32,
    pub rotate: u32,
    pub square: bool,
}

impl IdenticonSpec {
    pub fn from_seed(seed: &str) -> Self {
        let hash = IdenticonHash::of(seed);
        let colors = IDENTICON_PALETTES[hash.modulus(IDENTICON_PALETTES.len() as u32) as usize];
        let shape = |index: u32| IdenticonShape {
            color: colors[hash.offset(index).modulus(colors.len() as u32) as usize],
            translate_x: hash.scaled(index + 1).unit(23 - index as i32, Some(1)),
            translate_y: hash.scaled(index + 1).unit(23 - index as i32, Some(2)),
            rotate: hash.scaled(index + 1).modulus(360),
            square: hash.boolean(2),
        };
        Self {
            background: shape(0).color,
            bar: shape(1),
            circle: shape(2),
            line: shape(3),
        }
    }

    pub fn favicon(seed: &str) -> String {
        let svg = Self::from_seed(seed).svg();
        format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg)
        )
    }

    fn svg(&self) -> String {
        let bar_height = if self.bar.square { 80 } else { 10 };
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 80"><defs><clipPath id="avatar"><rect width="80" height="80" rx="18"/></clipPath></defs><g clip-path="url(#avatar)"><rect width="80" height="80" fill="{}"/><rect x="10" y="30" width="80" height="{}" fill="{}" transform="translate({} {}) rotate({} 40 40)"/><circle cx="40" cy="40" r="16" fill="{}" transform="translate({} {})"/><line x1="0" y1="40" x2="80" y2="40" stroke-width="2" stroke="{}" transform="translate({} {}) rotate({} 40 40)"/></g></svg>"#,
            self.background,
            bar_height,
            self.bar.color,
            self.bar.translate_x,
            self.bar.translate_y,
            self.bar.rotate,
            self.circle.color,
            self.circle.translate_x,
            self.circle.translate_y,
            self.line.color,
            self.line.translate_x,
            self.line.translate_y,
            self.line.rotate,
        )
    }
}

#[derive(Clone, Copy)]
struct IdenticonHash(u64);

impl IdenticonHash {
    fn of(seed: &str) -> Self {
        let mut hash = 0i32;
        for character in seed.encode_utf16() {
            hash = hash
                .wrapping_shl(5)
                .wrapping_sub(hash)
                .wrapping_add(i32::from(character));
        }
        Self(u64::from(hash.unsigned_abs()))
    }

    fn offset(self, value: u32) -> Self {
        Self(self.0 + u64::from(value))
    }

    fn scaled(self, value: u32) -> Self {
        Self(self.0 * u64::from(value))
    }

    fn modulus(self, range: u32) -> u32 {
        (self.0 % u64::from(range)) as u32
    }

    fn digit(self, place: u32) -> u32 {
        (self.0 / 10u64.pow(place) % 10) as u32
    }

    fn boolean(self, place: u32) -> bool {
        self.digit(place).is_multiple_of(2)
    }

    fn unit(self, range: i32, sign_digit: Option<u32>) -> i32 {
        let value = (self.0 % range as u64) as i32;
        match sign_digit {
            Some(place) if self.digit(place).is_multiple_of(2) => -value,
            _ => value,
        }
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
