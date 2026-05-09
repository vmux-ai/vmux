use std::fs;
use std::path::PathBuf;

const MARKER_FILENAME: &str = ".cascade_rename_done";

pub fn migrate_legacy_session_files(root: PathBuf) {
    let marker = root.join(MARKER_FILENAME);
    if marker.exists() {
        return;
    }
    if !root.exists() {
        return;
    }

    let mut moved = 0;

    let registry_path = root.join("sessions.ron");
    if registry_path.exists() {
        let bak = root.join("sessions.ron.bak");
        if let Err(err) = fs::rename(&registry_path, &bak) {
            eprintln!(
                "cascade-rename migration: could not move {:?}: {}",
                registry_path, err
            );
        } else {
            moved += 1;
        }
    }

    let profiles_dir = root.join("profiles");
    if profiles_dir.is_dir()
        && let Ok(entries) = fs::read_dir(&profiles_dir)
    {
        for entry in entries.flatten() {
            let profile_dir = entry.path();
            if !profile_dir.is_dir() {
                continue;
            }

            let session_ron = profile_dir.join("session.ron");
            if session_ron.exists() {
                let bak = profile_dir.join("session.ron.bak");
                if let Err(err) = fs::rename(&session_ron, &bak) {
                    eprintln!(
                        "cascade-rename migration: could not move {:?}: {}",
                        session_ron, err
                    );
                } else {
                    moved += 1;
                }
            }

            let sessions_dir = profile_dir.join("sessions");
            if sessions_dir.is_dir() {
                let bak = profile_dir.join("sessions.bak");
                if let Err(err) = fs::rename(&sessions_dir, &bak) {
                    eprintln!(
                        "cascade-rename migration: could not move {:?}: {}",
                        sessions_dir, err
                    );
                } else {
                    moved += 1;
                }
            }
        }
    }

    if moved > 0 {
        eprintln!(
            "cascade-rename migration: moved {} legacy file(s) to *.bak in {:?}",
            moved, root
        );
    }

    if let Err(err) = fs::write(&marker, b"done") {
        eprintln!(
            "cascade-rename migration: could not write marker {:?}: {}",
            marker, err
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn moves_legacy_files_to_bak() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        fs::write(root.join("sessions.ron"), b"legacy").unwrap();
        let profile_dir = root.join("profiles/default");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(profile_dir.join("session.ron"), b"legacy").unwrap();
        fs::create_dir_all(profile_dir.join("sessions")).unwrap();

        migrate_legacy_session_files(root.clone());

        assert!(!root.join("sessions.ron").exists());
        assert!(root.join("sessions.ron.bak").exists());
        assert!(!profile_dir.join("session.ron").exists());
        assert!(profile_dir.join("session.ron.bak").exists());
        assert!(!profile_dir.join("sessions").exists());
        assert!(profile_dir.join("sessions.bak").exists());
        assert!(root.join(".cascade_rename_done").exists());
    }

    #[test]
    fn idempotent_after_marker() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        fs::write(root.join(".cascade_rename_done"), b"done").unwrap();
        fs::write(root.join("sessions.ron"), b"legacy").unwrap();

        migrate_legacy_session_files(root.clone());

        assert!(root.join("sessions.ron").exists());
        assert!(!root.join("sessions.ron.bak").exists());
    }
}
