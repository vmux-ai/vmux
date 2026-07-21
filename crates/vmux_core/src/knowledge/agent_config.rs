use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::collections::HashSet;

use super::{agent_memories_prompt, memories_dir, migrate_external_memories, skills_dir};

const KNOWLEDGE_START: &str = "<!-- vmux-knowledge:start -->";
const KNOWLEDGE_END: &str = "<!-- vmux-knowledge:end -->";
const CODEX_SKILLS_START: &str = "# vmux-knowledge-skills:start";
const CODEX_SKILLS_END: &str = "# vmux-knowledge-skills:end";
const MIN_CODEX_PROJECT_DOC_BYTES: usize = 1024 * 1024;
const CODEX_PROJECT_DOC_HEADROOM: usize = 256 * 1024;

struct AgentConfigPaths {
    claude_instructions: PathBuf,
    claude_settings: PathBuf,
    claude_skills: PathBuf,
    codex_instructions: PathBuf,
    codex_config: PathBuf,
    codex_memories: PathBuf,
    codex_extension_memories: PathBuf,
    vibe_instructions: PathBuf,
    vibe_config: PathBuf,
}

impl AgentConfigPaths {
    fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"));
        let codex = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"));
        let vibe = std::env::var_os("VIBE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".vibe"));
        Self {
            claude_instructions: claude.join("CLAUDE.md"),
            claude_settings: claude.join("settings.json"),
            claude_skills: claude.join("skills"),
            codex_instructions: codex.join("AGENTS.md"),
            codex_config: codex.join("config.toml"),
            codex_memories: codex.join("memories"),
            codex_extension_memories: codex.join("memories_extensions"),
            vibe_instructions: vibe.join("AGENTS.md"),
            vibe_config: vibe.join("config.toml"),
        }
    }
}

pub fn sync_external_agent_configs() -> io::Result<()> {
    migrate_external_memories()?;
    let skills = skills_dir();
    let memories = memories_dir();
    std::fs::create_dir_all(&skills)?;
    std::fs::create_dir_all(&memories)?;
    sync_external_agent_configs_from(
        &AgentConfigPaths::from_env(),
        &skills,
        &memories,
        &agent_memories_prompt(),
    )
}

fn sync_external_agent_configs_from(
    paths: &AgentConfigPaths,
    skills_root: &Path,
    memories_root: &Path,
    memories: &str,
) -> io::Result<()> {
    let skills = skill_dirs(skills_root);
    let mut error = None;
    keep_first_error(
        &mut error,
        sync_markdown(&paths.claude_instructions, memories),
    );
    let claude_memories = memories_root.join("claude").join("auto");
    keep_first_error(
        &mut error,
        std::fs::create_dir_all(&claude_memories)
            .and_then(|_| sync_claude_settings(&paths.claude_settings, &claude_memories)),
    );
    keep_first_error(
        &mut error,
        sync_claude_skills(&paths.claude_skills, skills_root, &skills),
    );
    keep_first_error(
        &mut error,
        sync_markdown(&paths.codex_instructions, memories),
    );
    keep_first_error(
        &mut error,
        sync_codex_config(&paths.codex_config, &skills, memories.len()),
    );
    keep_first_error(
        &mut error,
        redirect_empty_directory(
            &paths.codex_memories,
            &memories_root.join("codex").join("local"),
        ),
    );
    keep_first_error(
        &mut error,
        redirect_empty_directory(
            &paths.codex_extension_memories,
            &memories_root.join("codex").join("extensions"),
        ),
    );
    keep_first_error(
        &mut error,
        sync_markdown(&paths.vibe_instructions, memories),
    );
    keep_first_error(
        &mut error,
        sync_vibe_config(&paths.vibe_config, skills_root),
    );
    error.map_or(Ok(()), Err)
}

fn keep_first_error(first: &mut Option<io::Error>, result: io::Result<()>) {
    if first.is_none()
        && let Err(error) = result
    {
        *first = Some(error);
    }
}

fn sync_markdown(path: &Path, memories: &str) -> io::Result<()> {
    let existing = read_optional(path)?;
    let updated = merge_managed_section(&existing, memories, KNOWLEDGE_START, KNOWLEDGE_END)?;
    write_changed(path, &existing, &updated)
}

fn merge_managed_section(
    existing: &str,
    body: &str,
    start_marker: &str,
    end_marker: &str,
) -> io::Result<String> {
    let start = existing.find(start_marker);
    let end = existing.find(end_marker);
    if start.is_some() != end.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("incomplete {start_marker} section"),
        ));
    }
    let without_managed = match (start, end) {
        (Some(start), Some(end)) if end >= start => {
            let end = end + end_marker.len();
            let before = &existing[..start];
            let after = existing[end..]
                .strip_prefix('\n')
                .unwrap_or(&existing[end..]);
            format!("{before}{after}")
        }
        (Some(_), Some(_)) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid {start_marker} section"),
            ));
        }
        (None, None) => existing.to_string(),
        _ => unreachable!(),
    };
    if body.is_empty() {
        return Ok(without_managed);
    }
    let separator = if without_managed.is_empty() || without_managed.ends_with("\n\n") {
        ""
    } else if without_managed.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    Ok(format!(
        "{without_managed}{separator}{start_marker}\n{}\n{end_marker}\n",
        body.trim_end()
    ))
}

fn skill_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut skills = entries
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let path = entry.path();
            (!file_type.is_symlink() && file_type.is_dir() && path.join("SKILL.md").is_file())
                .then_some(path)
        })
        .collect::<Vec<_>>();
    skills.sort();
    skills
}

fn sync_claude_settings(path: &Path, memories: &Path) -> io::Result<()> {
    let existing = read_optional(path)?;
    let mut settings = if existing.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str::<serde_json::Value>(&existing)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
    };
    let object = settings.as_object_mut().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Claude settings root must be an object",
        )
    })?;
    object.insert(
        "autoMemoryDirectory".to_string(),
        serde_json::Value::String(memories.to_string_lossy().into_owned()),
    );
    let mut updated = serde_json::to_string_pretty(&settings)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    updated.push('\n');
    write_changed(path, &existing, &updated)
}

#[cfg(unix)]
fn sync_claude_skills(
    destination: &Path,
    skills_root: &Path,
    skills: &[PathBuf],
) -> io::Result<()> {
    use std::os::unix::fs::symlink;

    std::fs::create_dir_all(destination)?;
    let desired = skills.iter().cloned().collect::<HashSet<_>>();
    for entry in std::fs::read_dir(destination)?.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_symlink() {
            continue;
        }
        let Ok(target) = std::fs::read_link(entry.path()) else {
            continue;
        };
        let target = if target.is_absolute() {
            target
        } else {
            destination.join(target)
        };
        if target.starts_with(skills_root) && !desired.contains(&target) {
            std::fs::remove_file(entry.path())?;
        }
    }
    for skill in skills {
        let Some(name) = skill.file_name() else {
            continue;
        };
        let link = destination.join(name);
        match std::fs::symlink_metadata(&link) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => symlink(skill, link)?,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn redirect_empty_directory(path: &Path, target: &Path) -> io::Result<()> {
    use std::os::unix::fs::symlink;

    std::fs::create_dir_all(target)?;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let link = std::fs::read_link(path)?;
            let link = if link.is_absolute() {
                link
            } else {
                path.parent().unwrap_or_else(|| Path::new("/")).join(link)
            };
            if link == target {
                return Ok(());
            }
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} points to {}", path.display(), link.display()),
            ));
        }
        Ok(metadata) if metadata.is_dir() => {
            if std::fs::read_dir(path)?.next().is_some() {
                return Ok(());
            }
            std::fs::remove_dir(path)?;
        }
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    symlink(target, path)
}

#[cfg(not(unix))]
fn redirect_empty_directory(_path: &Path, _target: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn sync_claude_skills(
    _destination: &Path,
    _skills_root: &Path,
    _skills: &[PathBuf],
) -> io::Result<()> {
    Ok(())
}

fn sync_codex_config(path: &Path, skills: &[PathBuf], memories_bytes: usize) -> io::Result<()> {
    let existing = read_optional(path)?;
    let minimum =
        MIN_CODEX_PROJECT_DOC_BYTES.max(memories_bytes.saturating_add(CODEX_PROJECT_DOC_HEADROOM));
    let with_limit = ensure_root_integer(&existing, "project_doc_max_bytes", minimum);
    let block = skills
        .iter()
        .map(|skill| {
            format!(
                "[[skills.config]]\npath = {}\nenabled = true",
                toml_string(&skill.to_string_lossy())
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let updated = merge_managed_section(&with_limit, &block, CODEX_SKILLS_START, CODEX_SKILLS_END)?;
    write_changed(path, &existing, &updated)
}

fn ensure_root_integer(existing: &str, key: &str, minimum: usize) -> String {
    if let Some(range) = root_assignment_line(existing, key) {
        let line = &existing[range.clone()];
        let value = line
            .split_once('=')
            .map(|(_, value)| value)
            .unwrap_or_default()
            .split('#')
            .next()
            .unwrap_or_default()
            .trim()
            .parse::<usize>()
            .unwrap_or_default();
        if value >= minimum {
            return existing.to_string();
        }
        return format!(
            "{}{} = {}{}",
            &existing[..range.start],
            key,
            minimum,
            &existing[range.end..]
        );
    }
    if existing.is_empty() {
        format!("{key} = {minimum}\n")
    } else {
        format!("{key} = {minimum}\n\n{existing}")
    }
}

fn sync_vibe_config(path: &Path, skills_root: &Path) -> io::Result<()> {
    let existing = read_optional(path)?;
    let mut values = if existing.is_empty() {
        Vec::new()
    } else {
        let parsed = existing
            .parse::<toml::Value>()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        match parsed.get("skill_paths") {
            Some(value) => value
                .as_array()
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "skill_paths must be an array")
                })?
                .iter()
                .map(|value| {
                    value.as_str().map(str::to_string).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "skill_paths entries must be strings",
                        )
                    })
                })
                .collect::<io::Result<Vec<_>>>()?,
            None => Vec::new(),
        }
    };
    let knowledge = skills_root.to_string_lossy().into_owned();
    if !values.contains(&knowledge) {
        values.push(knowledge);
    }
    let rendered = format!(
        "[{}]",
        values
            .iter()
            .map(|value| toml_string(value))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let updated = replace_root_array(&existing, "skill_paths", &rendered);
    write_changed(path, &existing, &updated)
}

fn replace_root_array(existing: &str, key: &str, rendered: &str) -> String {
    if let Some(range) = root_array_assignment(existing, key) {
        return format!(
            "{}{} = {}{}",
            &existing[..range.start],
            key,
            rendered,
            &existing[range.end..]
        );
    }
    if existing.is_empty() {
        format!("{key} = {rendered}\n")
    } else {
        format!("{key} = {rendered}\n\n{existing}")
    }
}

fn root_assignment_line(text: &str, key: &str) -> Option<Range<usize>> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            return None;
        }
        if assignment_value_offset(trimmed, key).is_some() {
            let indentation = line.len() - trimmed.len();
            let end = offset + line.trim_end_matches('\n').len();
            return Some(offset + indentation..end);
        }
        offset += line.len();
    }
    None
}

fn root_array_assignment(text: &str, key: &str) -> Option<Range<usize>> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            return None;
        }
        if let Some(value_offset) = assignment_value_offset(trimmed, key) {
            let start = offset + line.len() - trimmed.len();
            let value_start = start + value_offset;
            let suffix = &text[value_start..];
            let array_start = suffix.find('[')?;
            let array_end = array_end(&suffix[array_start..])?;
            return Some(start..value_start + array_start + array_end);
        }
        offset += line.len();
    }
    None
}

fn assignment_value_offset(line: &str, key: &str) -> Option<usize> {
    let rest = line.strip_prefix(key)?;
    if rest
        .chars()
        .next()
        .is_some_and(|character| character.is_alphanumeric() || character == '_')
    {
        return None;
    }
    let equals = rest.find('=')?;
    Some(key.len() + equals + 1)
}

fn array_end(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut quoted = None;
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if let Some(quote) = quoted {
            if escaped {
                escaped = false;
            } else if quote == '"' && character == '\\' {
                escaped = true;
            } else if character == quote {
                quoted = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => quoted = Some(character),
            '[' => depth += 1,
            ']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index + character.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn read_optional(path: &Path) -> io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error),
    }
}

fn write_changed(path: &Path, existing: &str, updated: &str) -> io::Result<()> {
    if existing == updated {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(root: &Path) -> AgentConfigPaths {
        AgentConfigPaths {
            claude_instructions: root.join("claude/CLAUDE.md"),
            claude_settings: root.join("claude/settings.json"),
            claude_skills: root.join("claude/skills"),
            codex_instructions: root.join("codex/AGENTS.md"),
            codex_config: root.join("codex/config.toml"),
            codex_memories: root.join("codex/memories"),
            codex_extension_memories: root.join("codex/memories_extensions"),
            vibe_instructions: root.join("vibe/AGENTS.md"),
            vibe_config: root.join("vibe/config.toml"),
        }
    }

    #[test]
    fn syncs_native_agent_config_and_preserves_user_content() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths(temp.path());
        let skills = temp.path().join("knowledge/skills");
        let memories = temp.path().join("knowledge/memories");
        std::fs::create_dir_all(skills.join("caveman")).unwrap();
        std::fs::create_dir_all(&paths.codex_memories).unwrap();
        std::fs::write(skills.join("caveman/SKILL.md"), "skill").unwrap();
        std::fs::create_dir_all(paths.claude_instructions.parent().unwrap()).unwrap();
        std::fs::create_dir_all(paths.codex_config.parent().unwrap()).unwrap();
        std::fs::create_dir_all(paths.vibe_config.parent().unwrap()).unwrap();
        std::fs::write(&paths.claude_instructions, "claude user\n").unwrap();
        std::fs::write(&paths.claude_settings, "{\"userSetting\":true}\n").unwrap();
        std::fs::write(&paths.codex_instructions, "codex user\n").unwrap();
        std::fs::write(&paths.codex_config, "model = \"gpt\"\n").unwrap();
        std::fs::write(
            &paths.vibe_config,
            "skill_paths = [\n  \"/existing\",\n]\nactive_model = \"model\"\n",
        )
        .unwrap();

        sync_external_agent_configs_from(&paths, &skills, &memories, "memory one").unwrap();
        sync_external_agent_configs_from(&paths, &skills, &memories, "memory two").unwrap();

        for path in [
            &paths.claude_instructions,
            &paths.codex_instructions,
            &paths.vibe_instructions,
        ] {
            let text = std::fs::read_to_string(path).unwrap();
            assert!(text.contains("memory two"));
            assert!(!text.contains("memory one"));
            assert_eq!(text.matches(KNOWLEDGE_START).count(), 1);
        }
        assert!(
            std::fs::read_to_string(&paths.claude_instructions)
                .unwrap()
                .starts_with("claude user\n")
        );
        assert!(
            std::fs::read_to_string(&paths.codex_instructions)
                .unwrap()
                .starts_with("codex user\n")
        );
        let claude_settings = std::fs::read_to_string(&paths.claude_settings)
            .unwrap()
            .parse::<serde_json::Value>()
            .unwrap();
        assert_eq!(
            claude_settings
                .get("userSetting")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            claude_settings
                .get("autoMemoryDirectory")
                .and_then(|value| value.as_str()),
            Some(memories.join("claude/auto").to_string_lossy().as_ref())
        );
        let codex = std::fs::read_to_string(&paths.codex_config).unwrap();
        assert!(codex.contains("model = \"gpt\""));
        assert!(codex.contains(&skills.join("caveman").to_string_lossy().to_string()));
        assert_eq!(codex.matches(CODEX_SKILLS_START).count(), 1);
        assert!(
            codex
                .parse::<toml::Value>()
                .unwrap()
                .get("project_doc_max_bytes")
                .and_then(toml::Value::as_integer)
                .is_some_and(|value| value >= MIN_CODEX_PROJECT_DOC_BYTES as i64)
        );
        let vibe = std::fs::read_to_string(&paths.vibe_config)
            .unwrap()
            .parse::<toml::Value>()
            .unwrap();
        assert_eq!(
            vibe.get("skill_paths")
                .and_then(toml::Value::as_array)
                .unwrap()
                .iter()
                .filter_map(toml::Value::as_str)
                .collect::<Vec<_>>(),
            vec!["/existing", skills.to_string_lossy().as_ref()]
        );
        #[cfg(unix)]
        {
            assert_eq!(
                std::fs::read_link(paths.claude_skills.join("caveman")).unwrap(),
                skills.join("caveman")
            );
            assert_eq!(
                std::fs::read_link(&paths.codex_memories).unwrap(),
                memories.join("codex/local")
            );
            assert_eq!(
                std::fs::read_link(&paths.codex_extension_memories).unwrap(),
                memories.join("codex/extensions")
            );
        }
    }

    #[test]
    fn rejects_incomplete_managed_sections() {
        assert!(
            merge_managed_section(KNOWLEDGE_START, "memory", KNOWLEDGE_START, KNOWLEDGE_END)
                .is_err()
        );
        assert!(
            merge_managed_section(KNOWLEDGE_END, "memory", KNOWLEDGE_START, KNOWLEDGE_END).is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn preserves_nonempty_native_memory_directory() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("codex/memories");
        let target = temp.path().join("knowledge/codex/local");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("existing.md"), "memory").unwrap();

        redirect_empty_directory(&source, &target).unwrap();

        assert!(source.is_dir());
        assert_eq!(
            std::fs::read_to_string(source.join("existing.md")).unwrap(),
            "memory"
        );
    }
}
