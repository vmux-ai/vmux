//! User-owned Knowledge Base conventions shared by agent launchers.

pub const KNOWLEDGE_TREE_EVENT: &str = "knowledge-tree";

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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkdownMetadata {
    pub title: String,
    pub title_line: Option<u32>,
    pub body_offset: usize,
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
    let mut title = String::new();
    let mut title_line = None;
    for (index, line) in lines.enumerate() {
        let value = line.trim_end_matches(['\r', '\n']);
        if value == "---" {
            return MarkdownMetadata {
                title,
                title_line,
                body_offset: offset + line.len(),
            };
        }
        if title.is_empty()
            && let Some((key, value)) = value.split_once(':')
            && key.trim().eq_ignore_ascii_case("title")
        {
            let value = value.trim();
            title = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .or_else(|| {
                    value
                        .strip_prefix('\'')
                        .and_then(|value| value.strip_suffix('\''))
                })
                .unwrap_or(value)
                .to_string();
            title_line = Some(index as u32 + 1);
        }
        offset += line.len();
    }
    MarkdownMetadata::default()
}

#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
const MAX_SKILLS: usize = 64;
#[cfg(not(target_arch = "wasm32"))]
const MAX_EMBEDDED_BYTES: usize = 24 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const SKILLS_PROMPT_MARKER: &str = "vmux Knowledge skills are user-owned instructions";
#[cfg(not(target_arch = "wasm32"))]
const MEMORIES_PROMPT_MARKER: &str = "vmux Knowledge memories are user-owned context";

#[cfg(not(target_arch = "wasm32"))]
pub fn knowledge_dir() -> PathBuf {
    crate::profile::config_dir().join("knowledge")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn skills_dir() -> PathBuf {
    knowledge_dir().join("skills")
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
        "vmux Knowledge skills are user-owned instructions stored under ~/.vmux/knowledge/skills. Apply relevant skills automatically. Skill catalog:\n",
    );
    for path in &files {
        let label = path
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        prompt.push_str("- ");
        prompt.push_str(&label);
        prompt.push_str(": ");
        prompt.push_str(&path.to_string_lossy());
        prompt.push('\n');
    }

    let mut embedded = 0usize;
    for path in files {
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        if embedded + body.len() > MAX_EMBEDDED_BYTES {
            continue;
        }
        embedded += body.len();
        prompt.push_str("\n<vmux-knowledge-skill path=\"");
        prompt.push_str(&path.to_string_lossy());
        prompt.push_str("\">\n");
        prompt.push_str(&body);
        if !body.ends_with('\n') {
            prompt.push('\n');
        }
        prompt.push_str("</vmux-knowledge-skill>\n");
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
        let text = "---\ntitle: \"Page title\"\ntags: [one]\n---\n\nBody\n";
        let metadata = markdown_metadata(text);
        assert_eq!(metadata.title, "Page title");
        assert_eq!(metadata.title_line, Some(1));
        assert_eq!(&text[metadata.body_offset..], "\nBody\n");
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
    fn loads_sorted_skill_catalog_and_bodies() {
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
