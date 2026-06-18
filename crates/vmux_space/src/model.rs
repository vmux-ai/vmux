pub const BOOTSTRAP_PROFILE_NAME: &str = "Personal";
pub const BOOTSTRAP_SPACE_ID: &str = "space-1";
pub const BOOTSTRAP_SPACE_NAME: &str = "Space 1";

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
        profile: BOOTSTRAP_PROFILE_NAME.to_string(),
    }
}

pub fn normalize_space_id(input: &str) -> String {
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
    if out.is_empty() {
        "space".to_string()
    } else {
        out
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
    fn unique_space_id_skips_existing() {
        let existing: std::collections::HashSet<String> =
            ["work".to_string(), "work-2".to_string()]
                .into_iter()
                .collect();
        assert_eq!(unique_space_id_among(&existing, "Work"), "work-3");
    }
}
