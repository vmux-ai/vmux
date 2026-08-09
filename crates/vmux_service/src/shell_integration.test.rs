use super::*;

#[test]
fn detects_known_shells_by_basename() {
    assert_eq!(detect_shell("/bin/bash"), Some(Shell::Bash));
    assert_eq!(detect_shell("/usr/bin/zsh"), Some(Shell::Zsh));
    assert_eq!(detect_shell("/opt/homebrew/bin/fish"), Some(Shell::Fish));
    assert_eq!(detect_shell("/opt/homebrew/bin/nu"), Some(Shell::Nu));
    assert_eq!(detect_shell("nu"), Some(Shell::Nu));
    assert_eq!(detect_shell("/bin/sh"), None);
    assert_eq!(detect_shell("/usr/bin/python3"), None);
}

#[test]
fn skips_one_shot_dash_c_invocations() {
    let dir = std::env::temp_dir().join("vmux-si-test-dashc");
    let mut args = vec!["-c".to_string(), "echo hi".to_string()];
    let mut env = vec![];
    inject("/bin/bash", &mut args, &mut env, &dir);
    assert_eq!(args, vec!["-c".to_string(), "echo hi".to_string()]);
    assert!(env.is_empty());
}

#[test]
fn skips_unknown_shell() {
    let dir = std::env::temp_dir().join("vmux-si-test-unknown");
    let mut args: Vec<String> = vec![];
    let mut env = vec![];
    inject("/bin/sh", &mut args, &mut env, &dir);
    assert!(args.is_empty());
    assert!(env.is_empty());
}

#[test]
fn bash_injects_rcfile_arg() {
    let dir = std::env::temp_dir().join(format!("vmux-si-bash-{}", std::process::id()));
    let mut args: Vec<String> = vec![];
    let mut env = vec![];
    inject("/bin/bash", &mut args, &mut env, &dir);
    assert_eq!(args.first().map(String::as_str), Some("--rcfile"));
    assert!(args[1].ends_with("bashrc"));
    assert!(
        std::fs::read_to_string(dir.join("bashrc"))
            .unwrap()
            .contains("133;C")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn zsh_sets_zdotdir_env_and_preserves_original() {
    let dir = std::env::temp_dir().join(format!("vmux-si-zsh-{}", std::process::id()));
    let mut args: Vec<String> = vec![];
    let mut env = vec![("ZDOTDIR".to_string(), "/user/zdot".to_string())];
    inject("/usr/bin/zsh", &mut args, &mut env, &dir);
    assert!(args.is_empty());
    let zdot = env.iter().find(|(k, _)| k == "ZDOTDIR").unwrap();
    assert!(zdot.1.ends_with("zsh"));
    let orig = env
        .iter()
        .find(|(k, _)| k == "__VMUX_ZDOTDIR_ORIG")
        .unwrap();
    assert_eq!(orig.1, "/user/zdot");
    assert!(
        std::fs::read_to_string(dir.join("zsh/.zshrc"))
            .unwrap()
            .contains("add-zsh-hook")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fish_injects_inline_init_command() {
    let dir = std::env::temp_dir().join("vmux-si-fish");
    let mut args: Vec<String> = vec![];
    let mut env = vec![];
    inject("/opt/homebrew/bin/fish", &mut args, &mut env, &dir);
    assert_eq!(args.first().map(String::as_str), Some("--init-command"));
    assert!(args[1].contains("fish_preexec"));
    assert!(args[1].contains("133;C"));
}

#[test]
fn nu_injects_config_arg() {
    let dir = std::env::temp_dir().join(format!("vmux-si-nu-{}", std::process::id()));
    let mut args: Vec<String> = vec![];
    let mut env = vec![];
    inject("/opt/homebrew/bin/nu", &mut args, &mut env, &dir);
    assert_eq!(args.first().map(String::as_str), Some("--config"));
    assert!(args[1].ends_with("config.nu"));
    assert!(
        std::fs::read_to_string(dir.join("config.nu"))
            .unwrap()
            .contains("pre_prompt")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
