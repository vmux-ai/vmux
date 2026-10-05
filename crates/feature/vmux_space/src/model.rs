pub const BOOTSTRAP_SPACE_ID: &str = "space-1";
pub const BOOTSTRAP_SPACE_NAME: &str = "space-1";

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
            name: BOOTSTRAP_SPACE_NAME.to_string(),
            profile: profile.into(),
        }
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
}
