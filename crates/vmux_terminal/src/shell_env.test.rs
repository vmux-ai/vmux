use super::*;

#[test]
fn parse_env_splits_on_first_equals() {
    let out = b"PATH=/usr/bin:/bin\nKEY=a=b=c\nEMPTY=\nnovalue\n=novalue\n";
    assert_eq!(
        parse_env(out),
        vec![
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
            ("KEY".to_string(), "a=b=c".to_string()),
            ("EMPTY".to_string(), String::new()),
        ]
    );
}

#[test]
fn merge_overrides_existing_keys_keeping_order() {
    // Simulate a base env (daemon) merged with a login env via dedup.
    let mut env = vec![
        ("VIBE_MCP_SERVERS".to_string(), "[...]".to_string()),
        ("ANTHROPIC_FOUNDRY_API_KEY".to_string(), "stale".to_string()),
        // login env appended (would come from login_shell_env):
        ("ANTHROPIC_FOUNDRY_API_KEY".to_string(), "fresh".to_string()),
        ("PATH".to_string(), "/login/bin".to_string()),
    ];
    dedup_env_keep_last(&mut env);
    assert_eq!(
        env,
        vec![
            ("VIBE_MCP_SERVERS".to_string(), "[...]".to_string()),
            ("ANTHROPIC_FOUNDRY_API_KEY".to_string(), "fresh".to_string()),
            ("PATH".to_string(), "/login/bin".to_string()),
        ]
    );
}

#[test]
fn extract_env_ignores_noise_outside_sentinels() {
    let raw = format!(
        "Welcome banner\r\nuser@host prompt $\r\n{ENV_BEGIN}\r\nPATH=/usr/bin:/bin\r\nFOO=bar\r\n{ENV_END}\r\nexit noise\r\n"
    );
    assert_eq!(
        extract_env_between_sentinels(raw.as_bytes()),
        vec![
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
            ("FOO".to_string(), "bar".to_string()),
        ]
    );
}

#[test]
fn extract_env_finds_marker_with_prompt_prefix() {
    let raw = format!("host% {ENV_BEGIN}\nKEY=val\n{ENV_END}\n");
    assert_eq!(
        extract_env_between_sentinels(raw.as_bytes()),
        vec![("KEY".to_string(), "val".to_string())]
    );
}

#[test]
fn extract_env_without_markers_is_empty() {
    let raw = b"PATH=/usr/bin\nFOO=bar\n";
    assert!(extract_env_between_sentinels(raw).is_empty());
}

#[cfg(unix)]
#[test]
fn pty_capture_reads_env_from_login_shell() {
    let shell = ["/bin/zsh", "/bin/bash", "/bin/sh"]
        .into_iter()
        .find(|path| Path::new(path).exists())
        .expect("a POSIX login shell should exist on unix");
    let env = capture_login_shell_env(shell);
    assert!(
        env.iter().any(|(key, _)| key == "PATH"),
        "expected PATH in env captured from {shell}, got {} vars",
        env.len()
    );
}
