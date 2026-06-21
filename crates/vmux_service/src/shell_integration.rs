use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Nu,
}

pub fn detect_shell(command: &str) -> Option<Shell> {
    let base = Path::new(command).file_name()?.to_str()?;
    match base {
        "bash" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "fish" => Some(Shell::Fish),
        "nu" | "nushell" => Some(Shell::Nu),
        _ => None,
    }
}

const BASH_RC: &str = r#"[ -r "$HOME/.bashrc" ] && . "$HOME/.bashrc"
if [[ $- == *i* ]]; then
  __vmux_at_prompt=1
  __vmux_preexec() { [ -n "$__vmux_at_prompt" ] || return; __vmux_at_prompt=; printf '\033]133;C\007'; }
  __vmux_precmd() { local s=$?; printf '\033]133;D;%s\007' "$s"; __vmux_at_prompt=1; }
  trap '__vmux_preexec' DEBUG
  PROMPT_COMMAND='__vmux_precmd'"${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
"#;

const ZSH_ZSHENV: &str = r#"ZDOTDIR="${__VMUX_ZDOTDIR_ORIG:-$HOME}"
[ -r "$ZDOTDIR/.zshenv" ] && source "$ZDOTDIR/.zshenv"
"#;

const ZSH_ZSHRC: &str = r#"ZDOTDIR="${__VMUX_ZDOTDIR_ORIG:-$HOME}"
[ -r "$ZDOTDIR/.zshrc" ] && source "$ZDOTDIR/.zshrc"
autoload -Uz add-zsh-hook 2>/dev/null
__vmux_pe() { printf '\033]133;C\007' }
__vmux_pc() { printf '\033]133;D;%s\007' "$?" }
add-zsh-hook preexec __vmux_pe 2>/dev/null
add-zsh-hook precmd __vmux_pc 2>/dev/null
"#;

const FISH_INIT: &str = "function __vmux_pe --on-event fish_preexec; printf '\\033]133;C\\007'; end; function __vmux_pc --on-event fish_postexec; printf '\\033]133;D;%s\\007' $status; end";

const NU_HOOKS: &str = r#"$env.config.hooks.pre_execution = ($env.config.hooks.pre_execution? | default [] | append {|| print -rn $"\u{1b}]133;C\u{7}" })
$env.config.hooks.pre_prompt = ($env.config.hooks.pre_prompt? | default [] | append {|| print -rn $"\u{1b}]133;D;($env.LAST_EXIT_CODE)\u{7}" })
"#;

fn set_env(env: &mut Vec<(String, String)>, key: &str, value: String) {
    if let Some(entry) = env.iter_mut().find(|(k, _)| k == key) {
        entry.1 = value;
    } else {
        env.push((key.to_string(), value));
    }
}

fn prepend_args(args: &mut Vec<String>, mut head: Vec<String>) {
    head.append(args);
    *args = head;
}

/// Resolve the user's nushell config dir so the generated config can re-source it.
fn nu_config_dir() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("NU_CONFIG_DIR") {
        return Some(std::path::PathBuf::from(dir));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(std::path::PathBuf::from(xdg).join("nushell"));
    }
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join("Library/Application Support/nushell"))
}

fn nu_config() -> String {
    let mut out = String::new();
    if let Some(user) = nu_config_dir().map(|d| d.join("config.nu")) {
        if user.exists() {
            out.push_str(&format!("source \"{}\"\n", user.display()));
        }
    }
    out.push_str(NU_HOOKS);
    out
}

/// Augment `args`/`env` so the spawned shell emits OSC 133 command-lifecycle
/// markers. No-op for unrecognized shells and for one-shot `-c` invocations
/// (those are command executions, not interactive prompt loops). Snippet files
/// are written under `dir`.
pub fn inject(command: &str, args: &mut Vec<String>, env: &mut Vec<(String, String)>, dir: &Path) {
    if args.iter().any(|a| a == "-c") {
        return;
    }
    let Some(shell) = detect_shell(command) else {
        return;
    };
    let _ = std::fs::create_dir_all(dir);
    match shell {
        Shell::Bash => {
            let rc = dir.join("bashrc");
            if std::fs::write(&rc, BASH_RC).is_ok() {
                prepend_args(
                    args,
                    vec!["--rcfile".to_string(), rc.to_string_lossy().into_owned()],
                );
            }
        }
        Shell::Zsh => {
            let zdir = dir.join("zsh");
            if std::fs::create_dir_all(&zdir).is_ok()
                && std::fs::write(zdir.join(".zshenv"), ZSH_ZSHENV).is_ok()
                && std::fs::write(zdir.join(".zshrc"), ZSH_ZSHRC).is_ok()
            {
                let orig = env
                    .iter()
                    .find(|(k, _)| k == "ZDOTDIR")
                    .map(|(_, v)| v.clone())
                    .or_else(|| std::env::var("ZDOTDIR").ok())
                    .or_else(|| std::env::var("HOME").ok())
                    .unwrap_or_default();
                set_env(env, "__VMUX_ZDOTDIR_ORIG", orig);
                set_env(env, "ZDOTDIR", zdir.to_string_lossy().into_owned());
            }
        }
        Shell::Fish => {
            prepend_args(
                args,
                vec!["--init-command".to_string(), FISH_INIT.to_string()],
            );
        }
        Shell::Nu => {
            let cfg = dir.join("config.nu");
            if std::fs::write(&cfg, nu_config()).is_ok() {
                prepend_args(
                    args,
                    vec!["--config".to_string(), cfg.to_string_lossy().into_owned()],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
}
