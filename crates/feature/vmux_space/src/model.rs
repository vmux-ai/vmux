pub const BOOTSTRAP_SPACE_ID: &str = "space-1";

const PLANET_NAMES: &[&str] = &[
    "mercury", "venus", "earth", "mars", "jupiter", "saturn", "uranus", "neptune", "ceres",
    "pluto", "haumea", "makemake", "eris",
];

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpaceRecord {
    pub id: String,
    pub name: String,
    pub profile: String,
}

impl Default for SpaceRecord {
    fn default() -> Self {
        Self::bootstrap()
    }
}

impl SpaceRecord {
    pub fn bootstrap() -> Self {
        Self::bootstrap_for("Personal")
    }

    pub fn bootstrap_for(profile: impl Into<String>) -> Self {
        Self {
            id: BOOTSTRAP_SPACE_ID.to_string(),
            name: Self::suggested_planet_name(&std::collections::HashSet::new()),
            profile: profile.into(),
        }
    }

    pub fn suggested_planet_name(existing: &std::collections::HashSet<String>) -> String {
        let start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos() as usize)
            .unwrap_or_default()
            % PLANET_NAMES.len();
        for offset in 0..PLANET_NAMES.len() {
            let name = PLANET_NAMES[(start + offset) % PLANET_NAMES.len()];
            if !existing.contains(name) {
                return name.to_string();
            }
        }
        Self::unique_id(existing, PLANET_NAMES[start])
    }

    pub fn is_planet_name(name: &str) -> bool {
        PLANET_NAMES.contains(&name)
    }

    pub fn normalized_id(input: &str) -> String {
        let segments: Vec<String> = input
            .split('/')
            .map(Self::slug_segment)
            .filter(|segment| !segment.is_empty())
            .collect();
        if segments.is_empty() {
            "space".to_string()
        } else {
            segments.join("/")
        }
    }

    pub fn unique_id(existing: &std::collections::HashSet<String>, name: &str) -> String {
        let base = Self::normalized_id(name);
        if !existing.contains(&base) {
            return base;
        }
        for idx in 2usize.. {
            let candidate = format!("{base}-{idx}");
            if !existing.contains(&candidate) {
                return candidate;
            }
        }
        unreachable!()
    }

    fn slug_segment(input: &str) -> String {
        let mut output = String::new();
        let mut pending_dash = false;
        for character in input.chars().flat_map(char::to_lowercase) {
            if character.is_ascii_alphanumeric() {
                if pending_dash && !output.is_empty() {
                    output.push('-');
                }
                output.push(character);
                pending_dash = false;
            } else if !output.is_empty() {
                pending_dash = true;
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_ids_are_slugged() {
        assert_eq!(SpaceRecord::normalized_id("Client A!"), "client-a");
        assert_eq!(SpaceRecord::normalized_id("  "), "space");
    }

    #[test]
    fn normalize_keeps_slash_as_nested_separator() {
        assert_eq!(SpaceRecord::normalized_id("vmux-ai/vmux"), "vmux-ai/vmux");
        assert_eq!(
            SpaceRecord::normalized_id("Org Name/Repo!"),
            "org-name/repo"
        );
        assert_eq!(SpaceRecord::normalized_id("a//b/"), "a/b");
    }

    #[test]
    fn unique_space_id_skips_existing() {
        let existing: std::collections::HashSet<String> =
            ["work".to_string(), "work-2".to_string()]
                .into_iter()
                .collect();
        assert_eq!(SpaceRecord::unique_id(&existing, "Work"), "work-3");
    }

    #[test]
    fn suggested_name_is_a_filesystem_safe_planet() {
        let name = SpaceRecord::suggested_planet_name(&std::collections::HashSet::new());

        assert!(SpaceRecord::is_planet_name(&name));
        assert_eq!(SpaceRecord::normalized_id(&name), name);
    }

    #[test]
    fn suggested_name_skips_existing_planets() {
        let existing = PLANET_NAMES
            .iter()
            .map(|name| name.to_string())
            .collect::<std::collections::HashSet<_>>();

        let name = SpaceRecord::suggested_planet_name(&existing);

        assert!(name.contains('-'));
        assert!(!existing.contains(&name));
    }
}
