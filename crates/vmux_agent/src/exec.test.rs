use super::*;

#[test]
fn command_lookup_finds_executable_on_path() {
    let temp = std::env::temp_dir().join(format!("vmux-agent-exec-path-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let exe = temp.join("fake-cli");
    std::fs::write(&exe, b"").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let found = find_executable_in_path("fake-cli", temp.to_string_lossy().as_ref());
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_dir(&temp);
    assert_eq!(found, Some(exe));
}
