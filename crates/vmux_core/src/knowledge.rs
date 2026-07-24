//! User-owned Knowledge Base conventions shared by agent launchers.

pub const KNOWLEDGE_TREE_EVENT: &str = "knowledge-tree";
pub const KNOWLEDGE_SEARCH_EVENT: &str = "knowledge-search";

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KnowledgeTreeEvent {
    pub root: String,
    pub entries: Vec<KnowledgeEntry>,
    pub error: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KnowledgeEntry {
    pub name: String,
    pub title: String,
    pub path: String,
    pub parent: String,
    pub is_directory: bool,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KnowledgeSearchMatch {
    pub title: String,
    pub path: String,
    pub line: u32,
    pub preview: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KnowledgeSearchEvent {
    pub query: String,
    pub matches: Vec<KnowledgeSearchMatch>,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KnowledgeSearchRequest {
    pub query: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KnowledgeReference {
    pub title: String,
    pub path: String,
    pub line: u32,
    pub preview: String,
    pub unlinked: bool,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum KnowledgePropertyKind {
    #[default]
    Text,
    Number,
    Checkbox,
    Date,
    List,
    Link,
    Tags,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KnowledgeProperty {
    pub key: String,
    pub kind: KnowledgePropertyKind,
    pub values: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WikiLink {
    pub start: usize,
    pub end: usize,
    pub note: String,
    pub anchor: Option<String>,
    pub label: Option<String>,
    pub embed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkdownMetadata {
    pub title: String,
    pub aliases: Vec<String>,
    pub properties: Vec<KnowledgeProperty>,
    pub title_line: Option<u32>,
    pub body_offset: usize,
}

fn metadata_scalar(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
        .to_string()
}

fn metadata_values(value: &str) -> Vec<String> {
    let value = value.trim();
    let values = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .map(|value| value.split(',').collect::<Vec<_>>())
        .unwrap_or_else(|| vec![value]);
    values
        .into_iter()
        .map(metadata_scalar)
        .filter(|value| !value.is_empty())
        .collect()
}

fn property_kind(key: &str, raw: &str, values: &[String], list: bool) -> KnowledgePropertyKind {
    if key.eq_ignore_ascii_case("tag") || key.eq_ignore_ascii_case("tags") {
        KnowledgePropertyKind::Tags
    } else if list {
        KnowledgePropertyKind::List
    } else if matches!(raw.trim().to_ascii_lowercase().as_str(), "true" | "false") {
        KnowledgePropertyKind::Checkbox
    } else if raw.trim().parse::<f64>().is_ok() {
        KnowledgePropertyKind::Number
    } else if values.first().is_some_and(|value| {
        let bytes = value.as_bytes();
        bytes.len() >= 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes[..4].iter().all(u8::is_ascii_digit)
            && bytes[5..7].iter().all(u8::is_ascii_digit)
            && bytes[8..10].iter().all(u8::is_ascii_digit)
    }) {
        KnowledgePropertyKind::Date
    } else if values
        .first()
        .is_some_and(|value| value.starts_with("[[") && value.ends_with("]]"))
    {
        KnowledgePropertyKind::Link
    } else {
        KnowledgePropertyKind::Text
    }
}

pub fn markdown_metadata(text: &str) -> MarkdownMetadata {
    let mut lines = text.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return MarkdownMetadata::default();
    };
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return MarkdownMetadata::default();
    }

    let mut offset = first.len();
    let frontmatter = lines.collect::<Vec<_>>();
    let Some(close) = frontmatter
        .iter()
        .position(|line| line.trim_end_matches(['\r', '\n']) == "---")
    else {
        return MarkdownMetadata::default();
    };
    let mut title = String::new();
    let mut aliases = Vec::new();
    let mut properties = Vec::new();
    let mut title_line = None;
    let mut index = 0;
    while index < close {
        let value = frontmatter[index].trim_end_matches(['\r', '\n']);
        if value.trim().is_empty() || value.trim_start().starts_with('#') || value.starts_with(' ')
        {
            index += 1;
            continue;
        }
        let Some((key, raw)) = value.split_once(':') else {
            index += 1;
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            index += 1;
            continue;
        }
        let raw = raw.trim();
        let mut values = metadata_values(raw);
        let inline_list = raw.starts_with('[') && raw.ends_with(']');
        let mut block_list = false;
        if raw.is_empty() {
            let mut next = index + 1;
            while next < close {
                let item = frontmatter[next].trim_end_matches(['\r', '\n']);
                let Some(item) = item.trim().strip_prefix('-').map(str::trim) else {
                    break;
                };
                values.extend(metadata_values(item));
                block_list = true;
                next += 1;
            }
            index = next.saturating_sub(1);
        }
        let kind = property_kind(key, raw, &values, inline_list || block_list);
        if kind == KnowledgePropertyKind::Tags {
            values = values
                .into_iter()
                .map(|value| value.trim_start_matches('#').to_string())
                .filter(|value| !value.is_empty())
                .collect();
        }
        if title.is_empty() && key.eq_ignore_ascii_case("title") {
            title = values.first().cloned().unwrap_or_default();
            title_line = Some(index.saturating_sub(values.len().saturating_sub(1)) as u32 + 1);
        }
        if key.eq_ignore_ascii_case("alias") || key.eq_ignore_ascii_case("aliases") {
            aliases.extend(values.iter().cloned());
        }
        properties.push(KnowledgeProperty {
            key: key.to_string(),
            kind,
            values,
        });
        index += 1;
    }
    offset += frontmatter[..=close]
        .iter()
        .map(|line| line.len())
        .sum::<usize>();
    MarkdownMetadata {
        title,
        aliases,
        properties,
        title_line,
        body_offset: offset,
    }
}

pub fn wiki_links(text: &str) -> Vec<WikiLink> {
    let mut links = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = text[offset..].find("[[") {
        let brackets = offset + relative_start;
        let start = brackets.saturating_sub(1);
        let embed = brackets > 0 && text.as_bytes()[brackets - 1] == b'!';
        let start = if embed { start } else { brackets };
        let content_start = brackets + 2;
        let Some(relative_end) = text[content_start..].find("]]") else {
            break;
        };
        let content_end = content_start + relative_end;
        let content = text[content_start..content_end].trim();
        if !content.is_empty() {
            let (target, label) = content
                .split_once('|')
                .map(|(target, label)| (target.trim(), Some(label.trim().to_string())))
                .unwrap_or((content, None));
            let (note, anchor) = target
                .split_once('#')
                .map(|(note, anchor)| (note.trim(), Some(anchor.trim().to_string())))
                .unwrap_or((target.trim(), None));
            links.push(WikiLink {
                start,
                end: content_end + 2,
                note: note.to_string(),
                anchor: anchor.filter(|anchor| !anchor.is_empty()),
                label: label.filter(|label| !label.is_empty()),
                embed,
            });
        }
        offset = content_end + 2;
    }
    links
}

#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Component, Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
mod agent_config;
#[cfg(not(target_arch = "wasm32"))]
pub use agent_config::sync_external_agent_configs;
#[cfg(not(target_arch = "wasm32"))]
mod index;
#[cfg(not(target_arch = "wasm32"))]
pub use index::{KnowledgeIndex, KnowledgeRenamePlan, KnowledgeResolvedLink, KnowledgeSearchHit};

#[cfg(not(target_arch = "wasm32"))]
const MAX_SKILLS: usize = 64;
#[cfg(not(target_arch = "wasm32"))]
const MAX_EMBEDDED_BYTES: usize = 24 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const SKILLS_PROMPT_MARKER: &str = "vmux Knowledge skill instructions are already loaded";
#[cfg(not(target_arch = "wasm32"))]
const MEMORIES_PROMPT_MARKER: &str = "vmux Knowledge memories are user-owned context";
#[cfg(not(target_arch = "wasm32"))]
const KNOWLEDGE_SECTIONS: [&str; 5] = ["skills", "memories", "projects", "meetings", "handbook"];
#[cfg(not(target_arch = "wasm32"))]
const MAX_NOTE_BYTES: usize = 2 * 1024 * 1024;

#[cfg(not(target_arch = "wasm32"))]
pub fn knowledge_dir() -> PathBuf {
    crate::profile::config_dir().join("knowledge")
}

#[cfg(not(target_arch = "wasm32"))]
fn yaml_scalar(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace(['\r', '\n'], " ")
    )
}

#[cfg(not(target_arch = "wasm32"))]
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
            format!("{key}: {value}\n")
        }
        KnowledgePropertyKind::Link => {
            let value = clean.first().cloned().unwrap_or_default();
            let value = if value.is_empty() || (value.starts_with("[[") && value.ends_with("]]")) {
                value
            } else {
                format!("[[{value}]]")
            };
            format!("{key}: {value}\n")
        }
        KnowledgePropertyKind::Text => {
            let value = clean.first().cloned().unwrap_or_default();
            format!("{key}: {}\n", yaml_scalar(&value))
        }
    };
    Ok(source)
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn write_note(path: Option<&str>, title: &str, content: &str) -> Result<PathBuf, String> {
    write_note_in(&knowledge_dir(), path, title, content)
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(all(not(target_arch = "wasm32"), unix))]
fn set_private_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())
}

#[cfg(all(not(target_arch = "wasm32"), not(unix)))]
fn set_private_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(all(not(target_arch = "wasm32"), unix))]
fn set_private_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())
}

#[cfg(all(not(target_arch = "wasm32"), not(unix)))]
fn set_private_file(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn skills_dir() -> PathBuf {
    knowledge_dir().join("skills")
}

#[cfg(not(target_arch = "wasm32"))]
fn configured_skill_dirs() -> Vec<PathBuf> {
    configured_skill_dirs_from(&skills_dir())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn configured_skill_files() -> Vec<PathBuf> {
    configured_skill_dirs()
        .into_iter()
        .map(|path| path.join("SKILL.md"))
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn memories_dir() -> PathBuf {
    knowledge_dir().join("memories")
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn agent_skills_prompt() -> String {
    agent_skills_prompt_from(&skills_dir())
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn agent_memories_prompt() -> String {
    agent_memories_prompt_from(&memories_dir())
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn agent_context_prompt() -> String {
    [agent_skills_prompt(), agent_memories_prompt()]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn append_agent_context(base: &str) -> String {
    append_agent_memories(&append_agent_skills(base))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn parses_markdown_frontmatter_title_and_body() {
        let text = "---\ntitle: \"Page title\"\ntags: [#one]\n---\n\nBody\n";
        let metadata = markdown_metadata(text);
        assert_eq!(metadata.title, "Page title");
        assert_eq!(metadata.title_line, Some(1));
        assert_eq!(metadata.properties.len(), 2);
        assert_eq!(metadata.properties[1].kind, KnowledgePropertyKind::Tags);
        assert_eq!(metadata.properties[1].values, ["one"]);
        assert_eq!(&text[metadata.body_offset..], "\nBody\n");
    }

    #[test]
    fn parses_quoted_titles_and_alias_lists_without_splitting_commas() {
        let text =
            "---\ntitle: \"Research, Notes\"\naliases:\n  - RN\n  - \"Reading Notes\"\n---\n";
        let metadata = markdown_metadata(text);
        assert_eq!(metadata.title, "Research, Notes");
        assert_eq!(metadata.aliases, ["RN", "Reading Notes"]);
    }

    #[test]
    fn ignores_incomplete_or_non_frontmatter_metadata() {
        assert_eq!(markdown_metadata("# Title\n"), MarkdownMetadata::default());
        assert_eq!(
            markdown_metadata("---\ntitle: Missing close\n"),
            MarkdownMetadata::default()
        );
    }

    #[test]
    fn parses_typed_frontmatter_properties() {
        let metadata = markdown_metadata(
            "---\npublished: true\nrating: 4.5\ndue: 2026-07-25\nrelated: '[[Roadmap]]'\npeople:\n  - Ada\n  - Lin\n---\n",
        );
        let kinds = metadata
            .properties
            .iter()
            .map(|property| property.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                KnowledgePropertyKind::Checkbox,
                KnowledgePropertyKind::Number,
                KnowledgePropertyKind::Date,
                KnowledgePropertyKind::Link,
                KnowledgePropertyKind::List,
            ]
        );
        assert_eq!(metadata.properties[4].values, ["Ada", "Lin"]);
    }

    #[test]
    fn edits_frontmatter_properties_without_touching_body() {
        let text = "---\ntitle: Old\ntags:\n  - alpha\nstatus: draft\n---\n\nBody\n";
        let edited = edit_markdown_property(
            text,
            &crate::event::FilePropertyEdit {
                original_key: "tags".into(),
                key: "tags".into(),
                kind: KnowledgePropertyKind::Tags,
                values: vec!["alpha".into(), "beta".into()],
                remove: false,
            },
        )
        .unwrap();
        let renamed = edit_markdown_property(
            &edited,
            &crate::event::FilePropertyEdit {
                original_key: "status".into(),
                key: "stage".into(),
                kind: KnowledgePropertyKind::Text,
                values: vec!["ready".into()],
                remove: false,
            },
        )
        .unwrap();
        assert!(renamed.contains("tags:\n  - \"alpha\"\n  - \"beta\"\n"));
        assert!(renamed.contains("stage: \"ready\"\n"));
        assert!(!renamed.contains("status:"));
        assert!(renamed.ends_with("\nBody\n"));
        assert!(
            edit_markdown_property(
                &renamed,
                &crate::event::FilePropertyEdit {
                    original_key: "stage".into(),
                    key: "title".into(),
                    kind: KnowledgePropertyKind::Text,
                    values: vec!["Duplicate".into()],
                    remove: false,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn writes_private_markdown_note_under_projects_by_default() {
        let temp = tempfile::tempdir().unwrap();
        let path = write_note_in(temp.path(), None, "YC Startup School", "Useful content").unwrap();
        let source = std::fs::read_to_string(&path).unwrap();

        assert_eq!(
            path,
            temp.path()
                .canonicalize()
                .unwrap()
                .join("projects/yc-startup-school.md")
        );
        assert_eq!(markdown_metadata(&source).title, "YC Startup School");
        assert!(source.ends_with("Useful content\n"));
    }

    #[test]
    fn rejects_knowledge_path_escape_and_non_markdown_files() {
        let temp = tempfile::tempdir().unwrap();

        assert!(write_note_in(temp.path(), Some("../outside.md"), "Title", "Body").is_err());
        assert!(write_note_in(temp.path(), Some("projects/note.txt"), "Title", "Body").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_inside_knowledge_root() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("projects")).unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("projects/linked")).unwrap();

        assert!(
            write_note_in(
                temp.path(),
                Some("projects/linked/note.md"),
                "Title",
                "Body"
            )
            .is_err()
        );
    }

    #[test]
    fn loads_sorted_skill_bodies_without_file_catalog() {
        let temp = tempfile::tempdir().unwrap();
        let beta = temp.path().join("beta");
        let alpha = temp.path().join("alpha");
        std::fs::create_dir_all(&beta).unwrap();
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::write(beta.join("SKILL.md"), "# Beta").unwrap();
        std::fs::write(alpha.join("SKILL.md"), "# Alpha").unwrap();
        let prompt = agent_skills_prompt_from(temp.path());
        assert!(prompt.find("alpha").unwrap() < prompt.find("beta").unwrap());
        assert!(prompt.contains("# Alpha"));
        assert!(prompt.contains("# Beta"));
        assert!(prompt.contains("already loaded below for this conversation"));
        assert!(prompt.contains("Do not read their SKILL.md files again"));
        assert!(!prompt.contains(&temp.path().to_string_lossy().to_string()));
        assert!(!prompt.contains("Skill catalog"));
    }

    #[test]
    fn migrates_all_external_markdown_memories_without_overwriting_edits() {
        let temp = tempfile::tempdir().unwrap();
        let claude = temp.path().join("claude-projects");
        let codex = temp.path().join("codex-memories");
        let extensions = temp.path().join("codex-extensions");
        let destination = temp.path().join("knowledge-memories");
        std::fs::create_dir_all(claude.join("project-a/memory/nested")).unwrap();
        std::fs::create_dir_all(&codex).unwrap();
        std::fs::create_dir_all(extensions.join("chronicle")).unwrap();
        std::fs::write(claude.join("project-a/memory/MEMORY.md"), "claude index").unwrap();
        std::fs::write(
            claude.join("project-a/memory/nested/topic.md"),
            "claude topic",
        )
        .unwrap();
        std::fs::write(claude.join("project-a/memory/ignored.json"), "ignored").unwrap();
        std::fs::write(codex.join("durable.md"), "codex durable").unwrap();
        std::fs::write(extensions.join("chronicle/recent.mdx"), "chronicle").unwrap();

        assert_eq!(
            migrate_external_memories_from(&destination, &claude, &codex, &extensions).unwrap(),
            4
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("claude/projects/project-a/MEMORY.md"))
                .unwrap(),
            "claude index"
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("codex/local/durable.md")).unwrap(),
            "codex durable"
        );
        let migrated = destination.join("claude/projects/project-a/MEMORY.md");
        std::fs::write(&migrated, "vmux edit").unwrap();
        std::fs::write(claude.join("project-a/memory/MEMORY.md"), "source edit").unwrap();
        assert_eq!(
            migrate_external_memories_from(&destination, &claude, &codex, &extensions).unwrap(),
            0
        );
        assert_eq!(std::fs::read_to_string(migrated).unwrap(), "vmux edit");
    }

    #[test]
    fn embeds_every_migrated_memory_in_sorted_order() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("nested")).unwrap();
        std::fs::write(temp.path().join("z.md"), "Zulu").unwrap();
        std::fs::write(temp.path().join("nested/a.markdown"), "Alpha").unwrap();
        let prompt = agent_memories_prompt_from(temp.path());
        assert!(prompt.find("nested/a.markdown").unwrap() < prompt.find("z.md").unwrap());
        assert!(prompt.contains("Alpha"));
        assert!(prompt.contains("Zulu"));
    }
}
