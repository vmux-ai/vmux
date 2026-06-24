pub fn bootstrap_profile_name() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        vmux_core::profile::display_name()
    }
    #[cfg(target_arch = "wasm32")]
    {
        "Personal".to_string()
    }
}

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
        bootstrap_space_record()
    }
}

pub fn bootstrap_space_record() -> SpaceRecord {
    SpaceRecord {
        id: BOOTSTRAP_SPACE_ID.to_string(),
        name: BOOTSTRAP_SPACE_NAME.to_string(),
        profile: bootstrap_profile_name(),
    }
}

fn slug_segment(input: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in input.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            out.push(ch);
            pending_dash = false;
        } else if !out.is_empty() {
            pending_dash = true;
        }
    }
    out
}

/// Slugs a space name into a filesystem-relative id, preserving `/` as a
/// nested-directory separator (each segment is slugged independently).
pub fn normalize_space_id(input: &str) -> String {
    let segments: Vec<String> = input
        .split('/')
        .map(slug_segment)
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() {
        "space".to_string()
    } else {
        segments.join("/")
    }
}

pub fn unique_space_id_among(existing: &std::collections::HashSet<String>, name: &str) -> String {
    let base = normalize_space_id(name);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_ids_are_slugged() {
        assert_eq!(normalize_space_id("Client A!"), "client-a");
        assert_eq!(normalize_space_id("  "), "space");
    }

    #[test]
    fn normalize_keeps_slash_as_nested_separator() {
        assert_eq!(normalize_space_id("vmux-ai/vmux"), "vmux-ai/vmux");
        assert_eq!(normalize_space_id("Org Name/Repo!"), "org-name/repo");
        assert_eq!(normalize_space_id("a//b/"), "a/b");
    }

    #[test]
    fn unique_space_id_skips_existing() {
        let existing: std::collections::HashSet<String> =
            ["work".to_string(), "work-2".to_string()]
                .into_iter()
                .collect();
        assert_eq!(unique_space_id_among(&existing, "Work"), "work-3");
    }
}
