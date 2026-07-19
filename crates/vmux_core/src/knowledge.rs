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
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
const MAX_SKILLS: usize = 64;
#[cfg(not(target_arch = "wasm32"))]
const MAX_EMBEDDED_BYTES: usize = 24 * 1024;

#[cfg(not(target_arch = "wasm32"))]
pub fn knowledge_dir() -> PathBuf {
    crate::profile::config_dir().join("knowledge")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn skills_dir() -> PathBuf {
    knowledge_dir().join("skills")
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
pub fn append_agent_skills(base: &str) -> String {
    if base.contains("vmux Knowledge skills are user-owned instructions") {
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
}
