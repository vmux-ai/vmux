use std::path::{Path, PathBuf};

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

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpaceRegistry {
    pub spaces: Vec<SpaceRecord>,
    #[serde(default)]
    pub active: Option<String>,
}

pub fn bootstrap_space_record() -> SpaceRecord {
    SpaceRecord {
        id: BOOTSTRAP_SPACE_ID.to_string(),
        name: BOOTSTRAP_SPACE_NAME.to_string(),
        profile: BOOTSTRAP_PROFILE_NAME.to_string(),
    }
}

pub fn select_active_record(registry: &SpaceRegistry) -> SpaceRecord {
    registry
        .active
        .as_deref()
        .and_then(|id| registry.spaces.iter().find(|space| space.id == id))
        .or_else(|| registry.spaces.first())
        .cloned()
        .unwrap_or_else(bootstrap_space_record)
}

pub fn registry_path(root: &Path) -> PathBuf {
    root.join("spaces.ron")
}

pub fn space_layout_path_for(root: &Path, space_id: &str, profile: &str) -> PathBuf {
    root.join("profiles")
        .join(normalize_space_id(profile))
        .join("spaces")
        .join(space_id)
        .join("space.ron")
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
    fn space_layouts_are_scoped_under_profiles() {
        let root = PathBuf::from("/tmp/vmux");
        assert_eq!(
            space_layout_path_for(&root, "space-1", "Personal"),
            root.join("profiles")
                .join("personal")
                .join("spaces")
                .join("space-1")
                .join("space.ron")
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
                profile: BOOTSTRAP_PROFILE_NAME.to_string(),
            },
            SpaceRecord {
                id: "work-2".to_string(),
                name: "Work 2".to_string(),
                profile: BOOTSTRAP_PROFILE_NAME.to_string(),
            },
        ];
        assert_eq!(unique_space_id(&records, "Work"), "work-3");
    }

    fn record(id: &str) -> SpaceRecord {
        SpaceRecord {
            id: id.to_string(),
            name: id.to_uppercase(),
            profile: BOOTSTRAP_PROFILE_NAME.to_string(),
        }
    }

    #[test]
    fn select_active_uses_active_id_when_present() {
        let registry = SpaceRegistry {
            spaces: vec![record("a"), record("b")],
            active: Some("b".to_string()),
        };
        assert_eq!(select_active_record(&registry).id, "b");
    }

    #[test]
    fn select_active_falls_back_to_first_when_active_none() {
        let registry = SpaceRegistry {
            spaces: vec![record("a"), record("b")],
            active: None,
        };
        assert_eq!(select_active_record(&registry).id, "a");
    }

    #[test]
    fn select_active_falls_back_to_first_when_active_id_unknown() {
        let registry = SpaceRegistry {
            spaces: vec![record("a")],
            active: Some("missing".to_string()),
        };
        assert_eq!(select_active_record(&registry).id, "a");
    }

    #[test]
    fn select_active_uses_bootstrap_when_empty() {
        let registry = SpaceRegistry {
            spaces: vec![],
            active: Some("anything".to_string()),
        };
        assert_eq!(select_active_record(&registry), bootstrap_space_record());
    }

    #[test]
    fn registry_without_active_field_parses() {
        let body = r#"(spaces: [(id: "a", name: "A", profile: "Personal")])"#;
        let registry: SpaceRegistry = ron::de::from_str(body).expect("parse legacy registry");
        assert_eq!(registry.active, None);
        assert_eq!(registry.spaces.len(), 1);
    }
}
