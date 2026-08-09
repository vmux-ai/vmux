pub fn shell_command_input(command: &str) -> Vec<u8> {
    let mut data = command.as_bytes().to_vec();
    data.push(b'\r');
    data
}

/// Build PTY bytes that paste `text` into a TUI composer via bracketed-paste
/// mode (so multiline / control characters insert literally). Any embedded
/// terminator is stripped so a paste can't break out early. With `submit`, a
/// trailing `\r` runs it. Empty (or terminator-only) text yields no bytes.
pub fn bracketed_paste_input(text: &str, submit: bool) -> Vec<u8> {
    let sanitized = text.replace("\x1b[201~", "");
    if sanitized.is_empty() {
        return Vec::new();
    }
    let mut data = Vec::with_capacity(sanitized.len() + 12);
    data.extend_from_slice(b"\x1b[200~");
    data.extend_from_slice(sanitized.as_bytes());
    data.extend_from_slice(b"\x1b[201~");
    if submit {
        data.push(b'\r');
    }
    data
}

#[cfg(test)]
#[path = "shell_input.test.rs"]
mod tests;
