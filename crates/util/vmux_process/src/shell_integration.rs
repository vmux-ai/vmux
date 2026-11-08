use std::collections::VecDeque;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shell {
    Bash,
    Zsh,
    Fish,
    Nu,
}

impl Shell {
    fn detect(command: &str) -> Option<Self> {
        let base = Path::new(command).file_name()?.to_str()?;
        match base {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            "nu" | "nushell" => Some(Self::Nu),
            _ => None,
        }
    }
}

const BASH_RC: &str = r#"[ -r "$HOME/.bashrc" ] && . "$HOME/.bashrc"
if [[ $- == *i* ]]; then
  __vmux_at_prompt=1
  __vmux_preexec() { [ -n "$__vmux_at_prompt" ] || return; __vmux_at_prompt=; printf '\033]2;%s\007\033]133;C\007' "$1"; }
  __vmux_precmd() { local s=$? p=$PWD; p=${p//%/%25}; p=${p//\\/%5C}; p=${p//#/%23}; p=${p//\?/%3F}; printf '\033]133;D;%s\007\033]7;file://localhost%s\007' "$s" "$p"; __vmux_at_prompt=1; }
  trap '__vmux_preexec "$BASH_COMMAND"' DEBUG
  PROMPT_COMMAND="${PROMPT_COMMAND:+$PROMPT_COMMAND;}"'__vmux_precmd'
fi
"#;

const ZSH_ZSHENV: &str = r#"ZDOTDIR="${__VMUX_ZDOTDIR_ORIG:-$HOME}"
[ -r "$ZDOTDIR/.zshenv" ] && source "$ZDOTDIR/.zshenv"
"#;

const ZSH_ZSHRC: &str = r#"ZDOTDIR="${__VMUX_ZDOTDIR_ORIG:-$HOME}"
[ -r "$ZDOTDIR/.zshrc" ] && source "$ZDOTDIR/.zshrc"
autoload -Uz add-zsh-hook 2>/dev/null
__vmux_pe() { printf '\033]2;%s\007\033]133;C\007' "$1" }
__vmux_pc() { local s=$? p=$PWD; p=${p//%/%25}; p=${p//\\/%5C}; p=${p//#/%23}; p=${p//\?/%3F}; printf '\033]133;D;%s\007\033]7;file://localhost%s\007' "$s" "$p" }
add-zsh-hook preexec __vmux_pe 2>/dev/null
add-zsh-hook precmd __vmux_pc 2>/dev/null
"#;

const FISH_INIT: &str = r#"function __vmux_pe --on-event fish_preexec; printf '\033]2;%s\007\033]133;C\007' $argv[1]; end
function __vmux_pc --on-event fish_postexec
    set -l s $status
    set -l p (string replace -a '%' '%25' -- $PWD | string replace -a '\\' '%5C' | string replace -a '#' '%23' | string replace -a '?' '%3F')
    printf '\033]133;D;%s\007\033]7;file://localhost%s\007' $s $p
end
__vmux_pc
"#;

const NU_HOOKS: &str = r#"$env.config.hooks.pre_execution = ($env.config.hooks.pre_execution? | default [] | append {|| print -rn $"\u{1b}]2;(commandline)\u{7}\u{1b}]133;C\u{7}" })
$env.config.hooks.pre_prompt = ($env.config.hooks.pre_prompt? | default [] | append {|| print -rn $"\u{1b}]133;D;($env.LAST_EXIT_CODE)\u{7}\u{1b}]7;file://localhost($env.PWD | str replace --all '%' '%25' | str replace --all '\' '%5C' | str replace --all '#' '%23' | str replace --all '?' '%3F')\u{7}" })
"#;

pub(crate) struct ShellIntegration;

impl ShellIntegration {
    pub(crate) fn directory_command(command: &str, path: &Path) -> Result<String, String> {
        let path = path.to_str().ok_or("directory path is not UTF-8")?;
        if path.chars().any(char::is_control) {
            return Err("directory path contains control characters".into());
        }
        match Shell::detect(command) {
            Some(Shell::Nu) => {
                let path = path.replace('\\', "\\\\").replace('"', "\\\"");
                Ok(format!("cd \"{path}\"\r"))
            }
            Some(Shell::Bash | Shell::Zsh | Shell::Fish) => {
                let path = path.replace('\'', "'\\''");
                Ok(format!("cd -- '{path}'\r"))
            }
            None => Err("directory switching requires a supported shell".into()),
        }
    }

    pub(crate) fn inject(
        command: &str,
        args: &mut Vec<String>,
        env: &mut Vec<(String, String)>,
        dir: &Path,
    ) {
        if args.iter().any(|argument| argument == "-c") {
            return;
        }
        let Some(shell) = Shell::detect(command) else {
            return;
        };
        let _ = std::fs::create_dir_all(dir);
        match shell {
            Shell::Bash => {
                let rc = dir.join("bashrc");
                if std::fs::write(&rc, BASH_RC).is_ok() {
                    Self::prepend_args(
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
                        .find(|(key, _)| key == "ZDOTDIR")
                        .map(|(_, value)| value.clone())
                        .or_else(|| std::env::var("ZDOTDIR").ok())
                        .or_else(|| std::env::var("HOME").ok())
                        .unwrap_or_default();
                    Self::set_env(env, "__VMUX_ZDOTDIR_ORIG", orig);
                    Self::set_env(env, "ZDOTDIR", zdir.to_string_lossy().into_owned());
                }
            }
            Shell::Fish => {
                Self::prepend_args(
                    args,
                    vec!["--init-command".to_string(), FISH_INIT.to_string()],
                );
            }
            Shell::Nu => {
                let config = dir.join("config.nu");
                if std::fs::write(&config, Self::nu_config()).is_ok() {
                    Self::prepend_args(
                        args,
                        vec![
                            "--config".to_string(),
                            config.to_string_lossy().into_owned(),
                        ],
                    );
                }
            }
        }
    }

    fn set_env(env: &mut Vec<(String, String)>, key: &str, value: String) {
        if let Some(entry) = env.iter_mut().find(|(current, _)| current == key) {
            entry.1 = value;
        } else {
            env.push((key.to_string(), value));
        }
    }

    fn prepend_args(args: &mut Vec<String>, mut head: Vec<String>) {
        head.append(args);
        *args = head;
    }

    fn nu_config_dir() -> Option<std::path::PathBuf> {
        if let Some(dir) = std::env::var_os("NU_CONFIG_DIR") {
            return Some(std::path::PathBuf::from(dir));
        }
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(std::path::PathBuf::from(xdg).join("nushell"));
        }
        let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
        let base = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else {
            home.join(".config")
        };
        Some(base.join("nushell"))
    }

    fn nu_config() -> String {
        let mut config = String::new();
        if let Some(user) = Self::nu_config_dir().map(|directory| directory.join("config.nu"))
            && user.exists()
        {
            config.push_str(&format!("source \"{}\"\n", user.display()));
        }
        config.push_str(NU_HOOKS);
        config
    }
}

#[derive(Default)]
pub(crate) struct ShellInput {
    revision: u64,
    submitted: VecDeque<u64>,
    executing: Option<u64>,
    ready: bool,
    overflowed: bool,
}

impl ShellInput {
    pub(crate) fn typed(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.revision = self.revision.wrapping_add(1).max(1);
        self.ready = false;
        for byte in bytes {
            if !matches!(*byte, b'\r' | b'\n') {
                continue;
            }
            if self.submitted.len() == 256 {
                self.overflowed = true;
                return;
            }
            self.submitted.push_back(self.revision);
        }
    }

    pub(crate) fn started(&mut self) {
        self.ready = false;
        self.executing = self.submitted.pop_front();
    }

    pub(crate) fn finished(&mut self) {
        let revision = self
            .executing
            .take()
            .or_else(|| self.submitted.pop_front())
            .unwrap_or(0);
        self.ready = !self.overflowed && self.submitted.is_empty() && revision == self.revision;
    }

    pub(crate) fn ready(&self) -> bool {
        self.ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DirectoryFixture {
        root: std::path::PathBuf,
        path: std::path::PathBuf,
    }

    impl DirectoryFixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("vmux-directory-{}", vmux_api::ProcessId::new()));
            let path = root.join("space ' $name $(false) `false` \"quoted\" % # ? \\");
            std::fs::create_dir_all(&path).unwrap();
            Self { root, path }
        }

        fn enter(&self, shell: &str) -> String {
            let command = ShellIntegration::directory_command(shell, &self.path)
                .unwrap()
                .replace('\r', "\n");
            let output = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(format!("{command}printf '%s' \"$PWD\""))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        }
    }

    impl Drop for DirectoryFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn directory_commands_keep_shell_metacharacters_inside_the_path() {
        let fixture = DirectoryFixture::new();
        for shell in ["/bin/bash", "/bin/zsh"] {
            assert_eq!(fixture.enter(shell), fixture.path.to_string_lossy());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn directory_commands_preserve_literal_metacharacters_in_nushell() {
        let fixture = DirectoryFixture::new();
        let command = ShellIntegration::directory_command("nu", &fixture.path)
            .unwrap()
            .replace('\r', "\n");
        let output = std::process::Command::new("/opt/homebrew/bin/nu")
            .args(["-n", "-c"])
            .arg(format!("{command}print -n (pwd)"))
            .output()
            .expect("Nushell is required on the development host");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            fixture.path.to_string_lossy()
        );
    }

    #[test]
    fn directory_switching_requires_an_empty_completed_prompt() {
        let mut input = ShellInput::default();
        assert!(!input.ready());
        input.finished();
        assert!(input.ready());
        input.typed(b"sleep 1\r");
        input.started();
        assert!(!input.ready());
        input.typed(b"draft");
        input.finished();
        assert!(!input.ready());
        input.typed(b"\r");
        input.started();
        input.finished();
        assert!(input.ready());
    }

    #[test]
    fn queued_commands_cannot_be_interrupted_by_a_directory_change() {
        let mut input = ShellInput::default();
        input.finished();
        input.typed(b"one\rtwo\r");
        input.started();
        input.finished();
        assert!(!input.ready());
        input.started();
        input.finished();
        assert!(input.ready());
    }

    #[test]
    fn directory_commands_refuse_control_characters_and_unknown_shells() {
        assert!(
            ShellIntegration::directory_command("bash", Path::new("/tmp/path\rcommand")).is_err()
        );
        assert!(
            ShellIntegration::directory_command("nu", Path::new("/tmp/path\ncommand")).is_err()
        );
        assert!(ShellIntegration::directory_command("python3", Path::new("/tmp")).is_err());
    }

    #[test]
    fn detects_known_shells_by_basename() {
        assert_eq!(Shell::detect("/bin/bash"), Some(Shell::Bash));
        assert_eq!(Shell::detect("/usr/bin/zsh"), Some(Shell::Zsh));
        assert_eq!(Shell::detect("/opt/homebrew/bin/fish"), Some(Shell::Fish));
        assert_eq!(Shell::detect("/opt/homebrew/bin/nu"), Some(Shell::Nu));
        assert_eq!(Shell::detect("nu"), Some(Shell::Nu));
        assert_eq!(Shell::detect("/bin/sh"), None);
        assert_eq!(Shell::detect("/usr/bin/python3"), None);
    }

    #[test]
    fn skips_one_shot_dash_c_invocations() {
        let dir = std::env::temp_dir().join("vmux-si-test-dashc");
        let mut args = vec!["-c".to_string(), "echo hi".to_string()];
        let mut env = vec![];
        ShellIntegration::inject("/bin/bash", &mut args, &mut env, &dir);
        assert_eq!(args, vec!["-c".to_string(), "echo hi".to_string()]);
        assert!(env.is_empty());
    }

    #[test]
    fn skips_unknown_shell() {
        let dir = std::env::temp_dir().join("vmux-si-test-unknown");
        let mut args: Vec<String> = vec![];
        let mut env = vec![];
        ShellIntegration::inject("/bin/sh", &mut args, &mut env, &dir);
        assert!(args.is_empty());
        assert!(env.is_empty());
    }

    #[test]
    fn bash_injects_rcfile_arg() {
        let dir = std::env::temp_dir().join(format!("vmux-si-bash-{}", std::process::id()));
        let mut args: Vec<String> = vec![];
        let mut env = vec![];
        ShellIntegration::inject("/bin/bash", &mut args, &mut env, &dir);
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
        ShellIntegration::inject("/usr/bin/zsh", &mut args, &mut env, &dir);
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
        ShellIntegration::inject("/opt/homebrew/bin/fish", &mut args, &mut env, &dir);
        assert_eq!(args.first().map(String::as_str), Some("--init-command"));
        assert!(args[1].contains("fish_preexec"));
        assert!(args[1].contains("133;C"));
    }

    #[test]
    fn nu_injects_config_arg() {
        let dir = std::env::temp_dir().join(format!("vmux-si-nu-{}", std::process::id()));
        let mut args: Vec<String> = vec![];
        let mut env = vec![];
        ShellIntegration::inject("/opt/homebrew/bin/nu", &mut args, &mut env, &dir);
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
