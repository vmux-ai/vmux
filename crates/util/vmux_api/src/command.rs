#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExploreStep {
    Read { path: String },
    Search { query: String, path: String },
    List { path: String },
}

impl ExploreStep {
    pub(crate) fn commands(source: &str) -> Option<Vec<Self>> {
        let mut steps = Vec::new();
        for command in Source::parts(source) {
            if command.contains("$(") || command.contains('`') {
                return None;
            }
            let words = shlex::split(command)?;
            let Some(program) = words.first() else {
                continue;
            };
            if words.iter().any(|word| {
                (program.ends_with("sed")
                    && (word.starts_with("-i") || word.starts_with("--in-place")))
                    || word.starts_with('>')
                    || word.starts_with('<')
            }) {
                return None;
            }
            let mut positional = Vec::new();
            let mut skip = false;
            for word in &words[1..] {
                if skip {
                    skip = false;
                    continue;
                }
                if matches!(
                    word.as_str(),
                    "-g" | "--glob"
                        | "-t"
                        | "--type"
                        | "-m"
                        | "--max-count"
                        | "-A"
                        | "-B"
                        | "-C"
                        | "-e"
                        | "-f"
                ) {
                    skip = true;
                    continue;
                }
                if word.starts_with('-') {
                    continue;
                }
                positional.push(word.clone());
            }
            match program.rsplit('/').next().unwrap_or(program) {
                "rg" if words.iter().any(|word| word == "--files") => steps.push(Self::List {
                    path: positional.first().cloned().unwrap_or_else(|| ".".into()),
                }),
                "rg" | "grep" => {
                    let query = positional.first()?.clone();
                    let path = if positional.len() > 1 {
                        positional[1..].join(" ")
                    } else {
                        ".".into()
                    };
                    steps.push(Self::Search { query, path });
                }
                "cat" | "bat" => {
                    for path in positional {
                        steps.push(Self::Read { path });
                    }
                }
                "sed" => steps.push(Self::Read {
                    path: positional.get(1)?.clone(),
                }),
                "head" | "tail"
                    if positional
                        .iter()
                        .any(|word| word.contains('/') || word.contains('.')) =>
                {
                    steps.push(Self::Read {
                        path: positional.last()?.clone(),
                    });
                }
                "ls" | "find" => steps.push(Self::List {
                    path: positional.first().cloned().unwrap_or_else(|| ".".into()),
                }),
                "head" | "tail" | "sort" | "uniq" | "cut" | "wc" => {}
                _ => return None,
            }
        }
        (!steps.is_empty()).then_some(steps)
    }
}

pub(crate) struct Source;

#[derive(Default)]
struct ScriptCalls {
    commands: Vec<String>,
    edits: Vec<String>,
}

impl Source {
    pub(crate) fn from_raw(raw: &str) -> Option<String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        if raw.starts_with('{')
            && let Some(command) = Self::object(raw)
        {
            return Some(command);
        }
        if ["const ", "let ", "await ", "text(", "//"]
            .iter()
            .any(|prefix| raw.starts_with(prefix))
        {
            let calls = Self::script(raw);
            if !calls.commands.is_empty() {
                return Some(calls.commands.join("\n"));
            }
        }
        Some(raw.to_string())
    }

    pub(crate) fn edited_paths(raw: &str) -> Vec<String> {
        Self::script(raw).edits
    }

    fn script(raw: &str) -> ScriptCalls {
        let mut calls = ScriptCalls::default();
        let mut cursor = 0;
        while cursor < raw.len() {
            let remaining = &raw[cursor..];
            if remaining.starts_with("//") {
                cursor += remaining.find('\n').unwrap_or(remaining.len());
                continue;
            }
            if let Some(length) = Self::quoted_length(remaining) {
                cursor += length;
                continue;
            }
            if let Some(arguments) = remaining.strip_prefix("tools.apply_patch(") {
                let arguments = arguments.trim_start();
                if let Some(length) = Self::quoted_length(arguments) {
                    if let Some(patch) = Self::string(&arguments[..length]) {
                        for line in patch.lines() {
                            for prefix in
                                ["*** Update File: ", "*** Add File: ", "*** Delete File: "]
                            {
                                if let Some(path) = line.strip_prefix(prefix) {
                                    calls.edits.push(path.to_string());
                                }
                            }
                        }
                    }
                    cursor = raw.len() - arguments.len() + length;
                    continue;
                }
            }
            if let Some(arguments) = remaining.strip_prefix("tools.exec_command(") {
                let arguments = arguments.trim_start();
                if arguments.starts_with('{') {
                    let length = Self::value_length(arguments);
                    if let Some(command) = Self::object(&arguments[..length]) {
                        calls.commands.push(command);
                    }
                    cursor = raw.len() - arguments.len() + length;
                    continue;
                }
            }
            cursor += remaining.chars().next().unwrap().len_utf8();
        }
        calls
    }

    fn object(raw: &str) -> Option<String> {
        let mut remaining = raw.strip_prefix('{')?.trim_start();
        while !remaining.is_empty() && !remaining.starts_with('}') {
            let key_length = Self::quoted_length(remaining)
                .unwrap_or_else(|| remaining.find(':').unwrap_or(remaining.len()));
            let key = remaining[..key_length].trim().trim_matches(['\'', '"']);
            remaining = remaining[key_length..]
                .trim_start()
                .strip_prefix(':')?
                .trim_start();
            let length = Self::value_length(remaining);
            if matches!(key, "cmd" | "command" | "script") {
                return Self::string(&remaining[..length]);
            }
            remaining = remaining[length..]
                .trim_start()
                .strip_prefix(',')?
                .trim_start();
        }
        None
    }

    fn quoted_length(raw: &str) -> Option<usize> {
        let quote = raw.chars().next()?;
        if !matches!(quote, '\'' | '"' | '`') {
            return None;
        }
        let mut escaped = false;
        for (index, character) in raw.char_indices().skip(1) {
            if escaped {
                escaped = false;
                continue;
            }
            if character == '\\' {
                escaped = true;
                continue;
            }
            if character == quote {
                return Some(index + character.len_utf8());
            }
        }
        None
    }

    fn value_length(raw: &str) -> usize {
        if let Some(length) = Self::quoted_length(raw) {
            return length;
        }
        let mut depth = 0usize;
        let mut cursor = 0;
        while cursor < raw.len() {
            let remaining = &raw[cursor..];
            if let Some(length) = Self::quoted_length(remaining) {
                cursor += length;
                continue;
            }
            let character = remaining.chars().next().unwrap();
            match character {
                '{' | '[' | '(' => depth += 1,
                '}' | ']' | ')' if depth == 0 => break,
                '}' | ']' | ')' => {
                    depth -= 1;
                    if depth == 0 {
                        cursor += 1;
                        break;
                    }
                }
                ',' if depth == 0 => break,
                _ => {}
            }
            cursor += character.len_utf8();
        }
        cursor
    }

    fn string(raw: &str) -> Option<String> {
        if raw.starts_with('"') {
            return serde_json::from_str(raw).ok();
        }
        let content = raw.strip_prefix('\'')?.strip_suffix('\'')?;
        let mut characters = content.chars();
        let mut value = String::new();
        while let Some(character) = characters.next() {
            if character != '\\' {
                value.push(character);
                continue;
            }
            let escaped = characters.next()?;
            value.push(match escaped {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\'' => '\'',
                '"' => '"',
                '\\' => '\\',
                _ => return None,
            });
        }
        Some(value)
    }

    fn parts(source: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut cursor = 0;
        while cursor < source.len() {
            let remaining = &source[cursor..];
            if let Some(length) = Self::quoted_length(remaining) {
                cursor += length;
                continue;
            }
            let character = remaining.chars().next().unwrap();
            if character == '\\' {
                cursor += character.len_utf8();
                if let Some(character) = source[cursor..].chars().next() {
                    cursor += character.len_utf8();
                }
                continue;
            }
            if matches!(character, ';' | '\n' | '|' | '&') {
                let command = source[start..cursor].trim();
                if !command.is_empty() {
                    parts.push(command);
                }
                start = cursor + character.len_utf8();
            }
            cursor += character.len_utf8();
        }
        let command = source[start..].trim();
        if !command.is_empty() {
            parts.push(command);
        }
        parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_real_commands_from_batched_javascript_without_parsing_quoted_examples() {
        let raw = r#"const example = "tools.exec_command({cmd:'cargo build'})";
text(await tools.exec_command({workdir: directory, cmd:"sed -n '1,20p' 'a file.rs'; rg -n 'foo|bar' src -g '*.rs'"}));
text(await tools.exec_command({cmd:'ls tests'}));"#;
        let source = Source::from_raw(raw).unwrap();
        assert_eq!(
            source,
            "sed -n '1,20p' 'a file.rs'; rg -n 'foo|bar' src -g '*.rs'\nls tests"
        );
        assert_eq!(
            ExploreStep::commands(&source).unwrap(),
            [
                ExploreStep::Read {
                    path: "a file.rs".into()
                },
                ExploreStep::Search {
                    query: "foo|bar".into(),
                    path: "src".into()
                },
                ExploreStep::List {
                    path: "tests".into()
                },
            ]
        );
    }

    #[test]
    fn keeps_execution_and_file_changes_out_of_exploration() {
        for source in [
            "cargo test",
            "rg foo src && cargo test",
            "sed -i '' 's/old/new/' src.rs",
            "cat $(secret_command)",
            "cat input > output",
        ] {
            assert!(ExploreStep::commands(source).is_none(), "{source}");
        }
        assert!(ExploreStep::commands("rg -i 'foo;bar' src | head -20").is_some());
    }

    #[test]
    fn identifies_files_changed_by_a_wrapped_patch_without_executing_the_script() {
        let raw = r#"text(await tools.apply_patch("*** Begin Patch\n*** Update File: a.rs\n@@\n-old\n+new\n*** Add File: b.rs\n+new\n*** End Patch"));"#;
        assert_eq!(Source::edited_paths(raw), ["a.rs", "b.rs"]);
    }
}
