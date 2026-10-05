pub struct HostQuote;

impl HostQuote {
    pub fn handing_to(host_shell: &str, interpreter: &str, script: &str) -> Result<String, String> {
        let named = interpreter.trim();
        let plain = named
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | '+' | ' '));
        if named.is_empty() || !plain {
            return Err(format!(
                "run.shell is not a plain program name: {interpreter}"
            ));
        }
        let eval = Self::eval_flag(named);
        Ok(format!(
            "{named} {eval} {}",
            Self::quote(host_shell, script)
        ))
    }

    fn eval_flag(named: &str) -> &'static str {
        let first = named.split_whitespace().next().unwrap_or(named);
        match first.rsplit('/').next().unwrap_or(first) {
            "node" | "nodejs" | "bun" | "ruby" | "perl" => "-e",
            "deno" => "eval",
            "php" => "-r",
            _ => "-c",
        }
    }

    fn quote(host_shell: &str, text: &str) -> String {
        let base = host_shell
            .rsplit('/')
            .next()
            .unwrap_or(host_shell)
            .trim()
            .to_ascii_lowercase();
        match base.as_str() {
            "nu" | "nushell" => Self::double_quoted(text, false),
            "fish" => Self::double_quoted(text, true),
            _ => Self::ansi_c_quoted(text),
        }
    }

    fn double_quoted(text: &str, escape_dollar: bool) -> String {
        let mut out = String::with_capacity(text.len() + 2);
        out.push('"');
        for c in text.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '$' if escape_dollar => out.push_str("\\$"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                _ => out.push(c),
            }
        }
        out.push('"');
        out
    }

    fn ansi_c_quoted(text: &str) -> String {
        let mut out = String::with_capacity(text.len() + 3);
        out.push_str("$'");
        for c in text.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '\'' => out.push_str("\\'"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                _ => out.push(c),
            }
        }
        out.push('\'');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_survives_the_host_shell_that_types_it() {
        let script = "print('it\\'s \"quoted\"')\nprint($HOME)";

        let zsh = HostQuote::handing_to("/bin/zsh", "python3", script).expect("zsh");
        assert_eq!(
            zsh, "python3 -c $'print(\\'it\\\\\\'s \"quoted\"\\')\\nprint($HOME)'",
            "a POSIX host needs $'' so the newline arrives as an escape, not as Enter"
        );

        let nu = HostQuote::handing_to("/opt/homebrew/bin/nu", "python3", script).expect("nu");
        assert_eq!(
            nu, "python3 -c \"print('it\\\\'s \\\"quoted\\\"')\\nprint($HOME)\"",
            "nushell has no $'' and does not expand $ inside a plain double-quoted string"
        );

        let fish = HostQuote::handing_to("/usr/local/bin/fish", "python3", script).expect("fish");
        assert!(
            fish.contains("\\$HOME"),
            "fish expands $ inside double quotes, so it has to be escaped: {fish}"
        );
    }

    #[test]
    fn each_interpreter_gets_the_flag_that_makes_it_read_a_script() {
        for (interpreter, expected) in [
            ("bash", "bash -c "),
            ("python3", "python3 -c "),
            ("node", "node -e "),
            ("/usr/local/bin/bun", "/usr/local/bin/bun -e "),
            ("ruby", "ruby -e "),
            ("deno", "deno eval "),
            ("php", "php -r "),
        ] {
            let line = HostQuote::handing_to("/bin/zsh", interpreter, "1").expect(interpreter);
            assert!(
                line.starts_with(expected),
                "{interpreter} reads a script from {expected:?}, got: {line}"
            );
        }
    }

    #[test]
    fn an_interpreter_may_carry_flags_but_never_shell_syntax() {
        assert!(HostQuote::handing_to("/bin/zsh", "python3 -u", "pass").is_ok());
        assert!(HostQuote::handing_to("/bin/zsh", "/usr/bin/env node", "1").is_ok());
        assert!(HostQuote::handing_to("/bin/zsh", "sh; rm -rf /", "1").is_err());
        assert!(HostQuote::handing_to("/bin/zsh", "sh $(id)", "1").is_err());
        assert!(HostQuote::handing_to("/bin/zsh", "  ", "1").is_err());
    }
}
