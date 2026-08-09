//! Reading and writing the knowledge vault on disk.
//!
//! Split from the parent so the wire types and markdown parsing stay compilable on web,
//! where there is no filesystem to reach. Everything here is gated once, at the module.

use super::{KnowledgePropertyKind, markdown_metadata};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

const MAX_SKILLS: usize = 64;
const MAX_EMBEDDED_BYTES: usize = 24 * 1024;
const SKILLS_PROMPT_MARKER: &str = "vmux Knowledge skill instructions are already loaded";
const MEMORIES_PROMPT_MARKER: &str = "vmux Knowledge memories are user-owned context";
const KNOWLEDGE_SECTIONS: [&str; 5] = ["skills", "memories", "projects", "meetings", "handbook"];
const MAX_NOTE_BYTES: usize = 2 * 1024 * 1024;

pub fn knowledge_dir() -> PathBuf {
    crate::profile::config_dir().join("knowledge")
}

fn yaml_scalar(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace(['\r', '\n'], " ")
    )
}

fn property_source(
    key: &str,
    kind: KnowledgePropertyKind,
    values: &[String],
) -> Result<String, String> {
    let key = key.trim();
    if key.is_empty()
        || key.len() > 100
        || key.contains([':', '\r', '\n'])
        || key.starts_with(['-', '#'])
    {
        return Err("property name is invalid".to_string());
    }
    let clean = values
        .iter()
        .map(|value| value.trim().replace(['\r', '\n'], " "))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let source = match kind {
        KnowledgePropertyKind::Tags | KnowledgePropertyKind::List => {
            if clean.is_empty() {
                format!("{key}: []\n")
            } else {
                let items = clean
                    .iter()
                    .map(|value| format!("  - {}\n", yaml_scalar(value)))
                    .collect::<String>();
                format!("{key}:\n{items}")
            }
        }
        KnowledgePropertyKind::Checkbox => {
            let checked = clean
                .first()
                .is_some_and(|value| value.eq_ignore_ascii_case("true"));
            format!("{key}: {checked}\n")
        }
        KnowledgePropertyKind::Number => {
            let value = clean.first().map(String::as_str).unwrap_or("0");
            if value.parse::<f64>().is_err() {
                return Err("number property requires a valid number".to_string());
            }
            format!("{key}: {value}\n")
        }
        KnowledgePropertyKind::Date => {
            let value = clean.first().cloned().unwrap_or_default();
            format!("{key}: {}\n", yaml_scalar(&value))
        }
        KnowledgePropertyKind::Link => {
            let value = clean.first().cloned().unwrap_or_default();
            let value = if value.is_empty() || (value.starts_with("[[") && value.ends_with("]]")) {
                value
            } else {
                format!("[[{value}]]")
            };
            format!("{key}: {}\n", yaml_scalar(&value))
        }
        KnowledgePropertyKind::Text => {
            let value = clean.first().cloned().unwrap_or_default();
            format!("{key}: {}\n", yaml_scalar(&value))
        }
    };
    Ok(source)
}

fn frontmatter_property_range(text: &str, key: &str) -> Option<(usize, usize, usize)> {
    let mut lines = text.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }
    let mut offset = first.len();
    let mut property_start = None;
    let mut found = None;
    for line in lines {
        let value = line.trim_end_matches(['\r', '\n']);
        if value == "---" {
            if let Some(start) = property_start.take() {
                found = Some((start, offset));
            }
            return found
                .filter(|_| !key.is_empty())
                .map(|(start, end)| (start, end, offset))
                .or(Some((usize::MAX, usize::MAX, offset)));
        }
        if !value.starts_with([' ', '\t'])
            && let Some((candidate, _)) = value.split_once(':')
        {
            if let Some(start) = property_start.take() {
                found = Some((start, offset));
            }
            if candidate.trim().eq_ignore_ascii_case(key.trim()) {
                property_start = Some(offset);
                found = None;
            }
        }
        offset += line.len();
    }
    None
}

pub fn edit_markdown_property(
    text: &str,
    edit: &crate::event::FilePropertyEdit,
) -> Result<String, String> {
    let original = edit.original_key.trim();
    let key = edit.key.trim();
    if !edit.remove
        && !original.eq_ignore_ascii_case(key)
        && markdown_metadata(text)
            .properties
            .iter()
            .any(|property| property.key.eq_ignore_ascii_case(key))
    {
        return Err(format!("property already exists: {key}"));
    }
    if let Some((start, end, close)) = frontmatter_property_range(text, original) {
        if edit.remove {
            if start == usize::MAX {
                return Ok(text.to_string());
            }
            let mut output = text.to_string();
            output.replace_range(start..end, "");
            return Ok(output);
        }
        let source = property_source(&edit.key, edit.kind, &edit.values)?;
        let mut output = text.to_string();
        if start == usize::MAX {
            output.insert_str(close, &source);
        } else {
            output.replace_range(start..end, &source);
        }
        return Ok(output);
    }
    if edit.remove {
        return Ok(text.to_string());
    }
    let source = property_source(&edit.key, edit.kind, &edit.values)?;
    Ok(format!("---\n{source}---\n\n{text}"))
}

pub fn write_note(path: Option<&str>, title: &str, content: &str) -> Result<PathBuf, String> {
    write_note_in(&knowledge_dir(), path, title, content)
}

fn write_note_in(
    root: &Path,
    path: Option<&str>,
    title: &str,
    content: &str,
) -> Result<PathBuf, String> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err("knowledge title must be 1 to 200 characters".to_string());
    }
    let content = content.trim();
    if content.is_empty() {
        return Err("knowledge content is empty".to_string());
    }
    if content.len() > MAX_NOTE_BYTES {
        return Err("knowledge content exceeds 2 MiB".to_string());
    }
    let relative = match path.map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from("projects").join(format!("{}.md", knowledge_slug(title))),
    };
    if relative.is_absolute() {
        return Err("knowledge path must be relative".to_string());
    }
    let components = relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => Ok(value.to_os_string()),
            _ => Err("knowledge path contains an invalid component".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.len() < 2 {
        return Err("knowledge path must include a section and file name".to_string());
    }
    let section = components[0].to_string_lossy();
    if !KNOWLEDGE_SECTIONS.contains(&section.as_ref()) {
        return Err(
            "knowledge path must start with skills, memories, projects, meetings, or handbook"
                .to_string(),
        );
    }
    let extension = relative.extension().and_then(|value| value.to_str());
    if !extension.is_some_and(|extension| {
        extension.eq_ignore_ascii_case("md")
            || extension.eq_ignore_ascii_case("markdown")
            || extension.eq_ignore_ascii_case("mdx")
    }) {
        return Err("knowledge file must use .md, .markdown, or .mdx".to_string());
    }

    if std::fs::symlink_metadata(root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("knowledge root cannot be a symlink".to_string());
    }
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    set_private_directory(root)?;
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let mut parent = canonical_root.clone();
    for component in &components[..components.len() - 1] {
        parent.push(component);
        match std::fs::symlink_metadata(&parent) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("knowledge path cannot traverse a symlink".to_string());
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err("knowledge parent path is not a directory".to_string());
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                std::fs::create_dir(&parent).map_err(|error| error.to_string())?;
                set_private_directory(&parent)?;
            }
            Err(error) => return Err(error.to_string()),
        }
        let canonical = parent.canonicalize().map_err(|error| error.to_string())?;
        if !canonical.starts_with(&canonical_root) {
            return Err("knowledge path escapes the knowledge root".to_string());
        }
    }
    let destination = parent.join(&components[components.len() - 1]);
    match std::fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("knowledge file cannot be a symlink".to_string());
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err("knowledge destination is not a file".to_string());
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let source = format!(
        "---\ntitle: {}\n---\n\n{}\n",
        serde_json::to_string(title).map_err(|error| error.to_string())?,
        content
    );
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&destination)
        .map_err(|error| error.to_string())?;
    file.write_all(source.as_bytes())
        .map_err(|error| error.to_string())?;
    set_private_file(&destination)?;
    Ok(destination)
}

fn knowledge_slug(title: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character);
            separator = false;
        } else {
            separator = true;
        }
        if slug.chars().count() >= 80 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "note".to_string()
    } else {
        slug.to_string()
    }
}

#[cfg(all(not(web), unix))]
fn set_private_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())
}

#[cfg(all(not(web), not(unix)))]
fn set_private_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(all(not(web), unix))]
fn set_private_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())
}

#[cfg(all(not(web), not(unix)))]
fn set_private_file(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub fn skills_dir() -> PathBuf {
    knowledge_dir().join("skills")
}

fn configured_skill_dirs() -> Vec<PathBuf> {
    configured_skill_dirs_from(&skills_dir())
}

pub fn configured_skill_files() -> Vec<PathBuf> {
    configured_skill_dirs()
        .into_iter()
        .map(|path| path.join("SKILL.md"))
        .collect()
}

pub(super) fn configured_skill_dirs_from(root: &Path) -> Vec<PathBuf> {
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

pub fn memories_dir() -> PathBuf {
    knowledge_dir().join("memories")
}

pub fn migrate_external_memories() -> io::Result<usize> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    let codex = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    migrate_external_memories_from(
        &memories_dir(),
        &claude.join("projects"),
        &codex.join("memories"),
        &codex.join("memories_extensions"),
    )
}

fn migrate_external_memories_from(
    destination: &Path,
    claude_projects: &Path,
    codex_memories: &Path,
    codex_extensions: &Path,
) -> io::Result<usize> {
    std::fs::create_dir_all(destination)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o700))?;
    }

    let mut imported = migrate_claude_memories(
        claude_projects,
        &destination.join("claude").join("projects"),
    )?;
    imported += migrate_memory_tree(codex_memories, &destination.join("codex").join("local"))?;
    imported += migrate_memory_tree(
        codex_extensions,
        &destination.join("codex").join("extensions"),
    )?;
    Ok(imported)
}

fn migrate_claude_memories(projects: &Path, destination: &Path) -> io::Result<usize> {
    let Ok(entries) = std::fs::read_dir(projects) else {
        return Ok(0);
    };
    let mut imported = 0;
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        imported += migrate_memory_tree(
            &entry.path().join("memory"),
            &destination.join(entry.file_name()),
        )?;
    }
    Ok(imported)
}

fn migrate_memory_tree(source: &Path, destination: &Path) -> io::Result<usize> {
    let mut files = Vec::new();
    collect_markdown_files(source, &mut files);
    files.sort();
    let mut imported = 0;
    for source_file in files {
        let Ok(relative) = source_file.strip_prefix(source) else {
            continue;
        };
        imported += usize::from(copy_new_file(&source_file, &destination.join(relative))?);
    }
    Ok(imported)
}

fn copy_new_file(source: &Path, destination: &Path) -> io::Result<bool> {
    let Some(parent) = destination.parent() else {
        return Ok(false);
    };
    std::fs::create_dir_all(parent)?;
    let mut output = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
    {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error),
    };
    let result = std::fs::File::open(source)
        .and_then(|mut input| io::copy(&mut input, &mut output))
        .and_then(|_| output.flush());
    if let Err(error) = result {
        let _ = std::fs::remove_file(destination);
        return Err(error);
    }
    Ok(true)
}

pub fn agent_skills_prompt() -> String {
    agent_skills_prompt_from(&skills_dir())
}

fn collect_skill_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_skill_files(&path, files);
        } else if file_type.is_file()
            && path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
        {
            files.push(path);
        }
    }
}

fn collect_markdown_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(metadata) = std::fs::symlink_metadata(dir) else {
        return;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_markdown_files(&path, files);
        } else if file_type.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("md")
                        || extension.eq_ignore_ascii_case("markdown")
                        || extension.eq_ignore_ascii_case("mdx")
                })
        {
            files.push(path);
        }
    }
}

fn agent_skills_prompt_from(root: &Path) -> String {
    let mut files = Vec::new();
    collect_skill_files(root, &mut files);
    files.sort();
    files.truncate(MAX_SKILLS);
    if files.is_empty() {
        return String::new();
    }

    let mut prompt = String::from(
        "vmux Knowledge skill instructions are already loaded below for this conversation. Apply them directly. Do not read their SKILL.md files again unless explicitly asked to refresh.\n",
    );

    let mut embedded = 0usize;
    for path in files {
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        if embedded + body.len() > MAX_EMBEDDED_BYTES {
            continue;
        }
        embedded += body.len();
        let label = path
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        prompt.push_str("\n<vmux-knowledge-instructions name=\"");
        prompt.push_str(&label);
        prompt.push_str("\">\n");
        prompt.push_str(&body);
        if !body.ends_with('\n') {
            prompt.push('\n');
        }
        prompt.push_str("</vmux-knowledge-instructions>\n");
    }
    prompt
}

pub fn agent_memories_prompt() -> String {
    agent_memories_prompt_from(&memories_dir())
}

fn agent_memories_prompt_from(root: &Path) -> String {
    let mut files = Vec::new();
    collect_markdown_files(root, &mut files);
    files.sort();
    if files.is_empty() {
        return String::new();
    }

    let mut prompt = String::from(
        "vmux Knowledge memories are user-owned context migrated from local agents. Use them as background context. Explicit current instructions and repository guidance win, and memories are not a source for current external facts.\n",
    );
    for path in files {
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        let label = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
        prompt.push_str("\n<vmux-knowledge-memory path=\"");
        prompt.push_str(&label);
        prompt.push_str("\">\n");
        prompt.push_str(&body);
        if !body.ends_with('\n') {
            prompt.push('\n');
        }
        prompt.push_str("</vmux-knowledge-memory>\n");
    }
    prompt
}

pub fn agent_context_prompt() -> String {
    [agent_skills_prompt(), agent_memories_prompt()]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn append_agent_skills(base: &str) -> String {
    if base.contains(SKILLS_PROMPT_MARKER) {
        return base.to_string();
    }
    let knowledge = agent_skills_prompt();
    if knowledge.is_empty() {
        base.to_string()
    } else if base.is_empty() {
        knowledge
    } else {
        format!("{base}\n\n{knowledge}")
    }
}

pub fn append_agent_memories(base: &str) -> String {
    if base.contains(MEMORIES_PROMPT_MARKER) {
        return base.to_string();
    }
    let memories = agent_memories_prompt();
    if memories.is_empty() {
        base.to_string()
    } else if base.is_empty() {
        memories
    } else {
        format!("{base}\n\n{memories}")
    }
}

pub fn append_agent_context(base: &str) -> String {
    append_agent_memories(&append_agent_skills(base))
}

#[cfg(test)]
#[path = "store.test.rs"]
mod tests;
