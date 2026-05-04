use std::path::{Path, PathBuf};

pub const DEFAULT_SESSION_ID: &str = "default";
pub const DEFAULT_PROFILE_ID: &str = "default";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub name: String,
    pub profile: String,
}

impl Default for SessionRecord {
    fn default() -> Self {
        default_session_record()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionRegistry {
    pub sessions: Vec<SessionRecord>,
}

pub fn default_session_record() -> SessionRecord {
    SessionRecord {
        id: DEFAULT_SESSION_ID.to_string(),
        name: "Default".to_string(),
        profile: DEFAULT_PROFILE_ID.to_string(),
    }
}

pub fn registry_path(root: &Path) -> PathBuf {
    root.join("sessions.ron")
}

pub fn session_layout_path_for(root: &Path, session_id: &str, profile: &str) -> PathBuf {
    let profile_root = root.join("profiles").join(profile);
    if session_id == DEFAULT_SESSION_ID {
        profile_root.join("session.ron")
    } else {
        profile_root
            .join("sessions")
            .join(session_id)
            .join("session.ron")
    }
}

pub fn normalize_session_id(input: &str) -> String {
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
        "session".to_string()
    } else {
        out
    }
}

pub fn unique_session_id<'a>(
    records: impl IntoIterator<Item = &'a SessionRecord>,
    name: &str,
) -> String {
    let base = normalize_session_id(name);
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
    fn default_session_uses_legacy_profile_session_file() {
        let root = PathBuf::from("/tmp/vmux");
        assert_eq!(
            session_layout_path_for(&root, "default", "default"),
            root.join("profiles").join("default").join("session.ron")
        );
    }

    #[test]
    fn named_session_is_scoped_under_attached_profile() {
        let root = PathBuf::from("/tmp/vmux");
        assert_eq!(
            session_layout_path_for(&root, "work", "client-a"),
            root.join("profiles")
                .join("client-a")
                .join("sessions")
                .join("work")
                .join("session.ron")
        );
    }

    #[test]
    fn session_ids_are_slugged() {
        assert_eq!(normalize_session_id("Client A!"), "client-a");
        assert_eq!(normalize_session_id("  "), "session");
    }

    #[test]
    fn session_ids_are_unique() {
        let records = vec![
            SessionRecord {
                id: "work".to_string(),
                name: "Work".to_string(),
                profile: "default".to_string(),
            },
            SessionRecord {
                id: "work-2".to_string(),
                name: "Work 2".to_string(),
                profile: "default".to_string(),
            },
        ];
        assert_eq!(unique_session_id(&records, "Work"), "work-3");
    }
}
