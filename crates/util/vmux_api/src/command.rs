#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExploreStep {
    Read { path: String },
    Search { query: String, path: String },
    List { path: String },
}

impl ExploreStep {
    pub(crate) fn commands(source: &str) -> Option<Vec<Self>> {
        let mut steps = Vec::new();
        let mut directory = std::path::PathBuf::new();
        for command in Source::parts(source) {
            if command.contains("$(") || command.contains('`') {
                return None;
            }
            let words = shlex::split(command)?;
            let Some(program) = words.first() else {
                continue;
            };
            if words
                .iter()
                .any(|word| word.starts_with('>') || word.starts_with('<'))
            {
                return None;
            }
            let program_name = program.rsplit('/').next().unwrap_or(program);
            if matches!(program_name, "bash" | "sh" | "zsh") {
                let argument = words.iter().position(|word| {
                    matches!(word.as_str(), "-c" | "-lc" | "-ic" | "-lic" | "-ilc")
                })?;
                for mut step in Self::commands(words.get(argument + 1)?)? {
                    step.in_directory(&directory);
                    steps.push(step);
                }
                continue;
            }
            if program_name == "cd" {
                let path = words.get(1)?;
                if words.len() != 2 || path == "-" {
                    return None;
                }
                directory = directory.join(path);
                continue;
            }
            if words.iter().any(|word| {
                program.ends_with("sed")
                    && (word.starts_with("-i") || word.starts_with("--in-place"))
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
            let previous = steps.len();
            match program_name {
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
                "sed" => {
                    let first = usize::from(
                        !words
                            .iter()
                            .any(|word| matches!(word.as_str(), "-e" | "-f")),
                    );
                    for path in positional.iter().skip(first) {
                        steps.push(Self::Read { path: path.clone() });
                    }
                }
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
            for step in &mut steps[previous..] {
                step.in_directory(&directory);
            }
        }
        (!steps.is_empty()).then_some(steps)
    }

    pub(crate) fn in_directory(&mut self, directory: &std::path::Path) {
        if directory.as_os_str().is_empty() {
            return;
        }
        let path = match self {
            Self::Read { path } | Self::List { path } | Self::Search { path, .. } => path,
        };
        if !std::path::Path::new(path).is_absolute() {
            *path = directory.join(&*path).to_string_lossy().into_owned();
        }
    }
}

pub(crate) struct Source;

#[derive(Default)]
struct ScriptCalls {
    commands: Vec<String>,
    edits: Vec<String>,
    files: Vec<String>,
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
        if Self::is_script(raw) {
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

    pub(crate) fn is_script(raw: &str) -> bool {
        ["const ", "let ", "await ", "text(", "//"]
            .iter()
            .any(|prefix| raw.trim_start().starts_with(prefix))
    }

    pub(crate) fn file_paths(raw: &str) -> Vec<String> {
        let raw = raw.trim();
        if Self::is_script(raw) {
            return Self::script(raw).files;
        }
        if raw.starts_with('{') {
            let Some(command) = Self::object(raw) else {
                return Vec::new();
            };
            let directory = Self::string_property(raw, &["workdir", "cwd"]);
            return Self::command_files(&command, directory.as_deref());
        }
        Self::command_files(raw, None)
    }

    fn command_files(command: &str, directory: Option<&str>) -> Vec<String> {
        let mut files = Vec::new();
        if Self::is_script(command) {
            for path in Self::file_paths(command) {
                let mut step = ExploreStep::Read { path };
                if let Some(directory) = directory {
                    step.in_directory(std::path::Path::new(directory));
                }
                if let ExploreStep::Read { path } = step {
                    files.push(path);
                }
            }
            return files;
        }
        let Some(steps) = ExploreStep::commands(command) else {
            return files;
        };
        for mut step in steps {
            if let Some(directory) = directory {
                step.in_directory(std::path::Path::new(directory));
            }
            if let ExploreStep::Read { path } = step {
                files.push(path);
            }
        }
        files
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
                        let directory =
                            Self::string_property(&arguments[..length], &["workdir", "cwd"]);
                        calls
                            .files
                            .extend(Self::command_files(&command, directory.as_deref()));
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
        Self::string_property(raw, &["cmd", "command", "script"])
    }

    fn string_property(raw: &str, names: &[&str]) -> Option<String> {
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
            if names.contains(&key) {
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
            "bash -c \"cat input\" > output",
            "sh -c \"cat input\" < source",
            "zsh -c \"cat input\" >> output",
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
