use std::path::{Path, PathBuf};

pub const DEFAULT_SPACE_ID: &str = "default";
pub const DEFAULT_PROFILE_ID: &str = "default";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpaceRecord {
    pub id: String,
    pub name: String,
    pub profile: String,
}

impl Default for SpaceRecord {
    fn default() -> Self {
        default_space_record()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpaceRegistry {
    pub spaces: Vec<SpaceRecord>,
}

pub fn default_space_record() -> SpaceRecord {
    SpaceRecord {
        id: DEFAULT_SPACE_ID.to_string(),
        name: "Default".to_string(),
        profile: DEFAULT_PROFILE_ID.to_string(),
    }
}

pub fn registry_path(root: &Path) -> PathBuf {
    root.join("spaces.ron")
}

pub fn space_layout_path_for(root: &Path, space_id: &str, profile: &str) -> PathBuf {
    let profile_root = root.join("profiles").join(profile);
    if space_id == DEFAULT_SPACE_ID {
        profile_root.join("space.ron")
    } else {
        profile_root.join("spaces").join(space_id).join("space.ron")
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

pub fn unique_space_id<'a>(
    records: impl IntoIterator<Item = &'a SpaceRecord>,
    name: &str,
) -> String {
    let base = normalize_space_id(name);
    let existing: std::collections::HashSet<&str> = records
        .into_iter()
        .map(|record| record.id.as_str())
        .collect();
    if !existing.contains(base.as_str()) {
        return base;
    }
    for idx in 2usize.. {
        let candidate = format!("{base}-{idx}");
        if !existing.contains(candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_space_uses_profile_space_file() {
        let root = PathBuf::from("/tmp/vmux");
        assert_eq!(
            space_layout_path_for(&root, "default", "default"),
            root.join("profiles").join("default").join("space.ron")
        );
    }

    #[test]
    fn named_space_is_scoped_under_attached_profile() {
        let root = PathBuf::from("/tmp/vmux");
        assert_eq!(
            space_layout_path_for(&root, "work", "client-a"),
            root.join("profiles")
                .join("client-a")
                .join("spaces")
                .join("work")
                .join("space.ron")
        );
    }

    #[test]
    fn space_ids_are_slugged() {
        assert_eq!(normalize_space_id("Client A!"), "client-a");
        assert_eq!(normalize_space_id("  "), "space");
    }

    #[test]
    fn space_ids_are_unique() {
        let records = vec![
            SpaceRecord {
                id: "work".to_string(),
                name: "Work".to_string(),
                profile: "default".to_string(),
            },
            SpaceRecord {
                id: "work-2".to_string(),
                name: "Work 2".to_string(),
                profile: "default".to_string(),
            },
        ];
        assert_eq!(unique_space_id(&records, "Work"), "work-3");
    }
}
