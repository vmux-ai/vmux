//! Reading and writing the knowledge vault on disk.
//!
//! Split from the parent so the wire types and markdown parsing stay compilable on web,
//! where there is no filesystem to reach. Everything here is gated once, at the module.
//!
//! [`KnowledgeVault`] is the root every path is derived from, and [`KnowledgeVault::user`] opens
//! the one under the active profile. Every path the vault hands out is derived from that root, so
//! a test can run the same code against a temporary one.

use super::{KnowledgePropertyKind, markdown_metadata};
use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

const MAX_SKILLS: usize = 64;
const MAX_EMBEDDED_BYTES: usize = 24 * 1024;
const SKILLS_PROMPT_MARKER: &str = "vmux Knowledge skill instructions are already loaded";
const MEMORIES_PROMPT_MARKER: &str = "vmux Knowledge memories are user-owned context";
const KNOWLEDGE_SECTIONS: [&str; 5] = ["skills", "memories", "projects", "meetings", "handbook"];
const MAX_NOTE_BYTES: usize = 2 * 1024 * 1024;

/// One knowledge vault: a root directory holding the sections a user's notes, skills and
/// memories live under.
///
/// Every path the vault hands out is derived from `root`, so [`KnowledgeVault::at`] can point one
/// at a temporary directory and exercise exactly the code the user's vault runs.
pub struct KnowledgeVault {
    root: PathBuf,
}

impl KnowledgeVault {
    /// The vault under the active profile's config directory.
    pub fn user() -> Self {
        Self::at(crate::profile::config_dir().join("knowledge"))
    }

    /// A vault rooted anywhere.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn into_root(self) -> PathBuf {
        self.root
    }

    /// Where skills live.
    pub fn skills(&self) -> SkillsDir {
        SkillsDir(self.root.join("skills"))
    }

    /// Where memories imported from external agents live.
    pub fn memories(&self) -> MemoriesDir {
        MemoriesDir(self.root.join("memories"))
    }

    /// Write `content` as a markdown note titled `title`, at `path` when the caller named one and
    /// under `projects/` otherwise.
    ///
    /// The note is private to the user, and every directory created on the way is too. A path
    /// that is absolute, escapes the root, traverses a symlink, names a section the vault does
    /// not have, or is not markdown is refused rather than followed.
    pub fn write_note(
        &self,
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
        let note = NotePath::parse(path, title)?;
        let destination = self.create_parents(&note)?.join(note.file_name());
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
        Privacy::File
            .apply(&destination)
            .map_err(|error| error.to_string())?;
        Ok(destination)
    }

    /// Create every directory `note` sits under, returning the canonical parent of the file.
    ///
    /// Each level is checked before it is descended into and canonicalized after, so a symlink
    /// planted mid-path cannot redirect the write outside the vault.
    fn create_parents(&self, note: &NotePath) -> Result<PathBuf, String> {
        if std::fs::symlink_metadata(&self.root)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err("knowledge root cannot be a symlink".to_string());
        }
        std::fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        Privacy::Directory
            .apply(&self.root)
            .map_err(|error| error.to_string())?;
        let canonical_root = self
            .root
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let mut parent = canonical_root.clone();
        for component in note.parents() {
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
                    Privacy::Directory
                        .apply(&parent)
                        .map_err(|error| error.to_string())?;
                }
                Err(error) => return Err(error.to_string()),
            }
            let canonical = parent.canonicalize().map_err(|error| error.to_string())?;
            if !canonical.starts_with(&canonical_root) {
                return Err("knowledge path escapes the knowledge root".to_string());
            }
        }
        Ok(parent)
    }
}

/// A vault-relative note path, already validated: `<section>/…/<name>.md`, built only from plain
/// components so nothing in it can climb out of the vault.
struct NotePath {
    /// Directories between the vault root and the file, outermost first. The first is the
    /// vault section.
    parents: Vec<OsString>,
    file_name: OsString,
}

impl NotePath {
    /// The path the caller asked for, or one derived from `title` under `projects/`.
    fn parse(requested: Option<&str>, title: &str) -> Result<Self, String> {
        let relative = match requested.map(str::trim).filter(|path| !path.is_empty()) {
            Some(path) => PathBuf::from(path),
            None => PathBuf::from("projects").join(format!("{}.md", NoteSlug::of(title))),
        };
        if relative.is_absolute() {
            return Err("knowledge path must be relative".to_string());
        }
        let mut components = Vec::new();
        for component in relative.components() {
            let Component::Normal(value) = component else {
                return Err("knowledge path contains an invalid component".to_string());
            };
            components.push(value.to_os_string());
        }
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
        if !MarkdownPath(&relative).is_note() {
            return Err("knowledge file must use .md, .markdown, or .mdx".to_string());
        }
        let Some(file_name) = components.pop() else {
            return Err("knowledge path must include a section and file name".to_string());
        };
        Ok(Self {
            parents: components,
            file_name,
        })
    }

    fn parents(&self) -> &[OsString] {
        &self.parents
    }

    fn file_name(&self) -> &OsStr {
        &self.file_name
    }
}

/// A file-name slug derived from a note title.
struct NoteSlug(String);

impl NoteSlug {
    /// Lower-case alphanumerics joined by dashes, capped at 80 characters. A title with nothing
    /// usable left becomes `note`, so the file always has a name.
    fn of(title: &str) -> Self {
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
            return Self("note".to_string());
        }
        Self(slug.to_string())
    }
}

impl std::fmt::Display for NoteSlug {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A path judged by its extension.
#[derive(Clone, Copy)]
struct MarkdownPath<'a>(&'a Path);

impl MarkdownPath<'_> {
    const EXTENSIONS: [&'static str; 3] = ["md", "markdown", "mdx"];

    /// Whether this names a note the vault reads or writes.
    fn is_note(self) -> bool {
        let Some(extension) = self.0.extension().and_then(|value| value.to_str()) else {
            return false;
        };
        for candidate in Self::EXTENSIONS {
            if extension.eq_ignore_ascii_case(candidate) {
                return true;
            }
        }
        false
    }
}

/// The permissions the vault keeps on what it creates.
///
/// It holds a user's private notes and the memories imported from their local agents, so nothing
/// it writes is left readable by another account.
#[derive(Clone, Copy)]
enum Privacy {
    Directory,
    File,
}

impl Privacy {
    /// Tighten `path` to this level.
    #[cfg(unix)]
    fn apply(self, path: &Path) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let mode = match self {
            Self::Directory => 0o700,
            Self::File => 0o600,
        };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
    }

    /// A no-op off unix, where there is no mode to set.
    #[cfg(not(unix))]
    fn apply(self, _path: &Path) -> io::Result<()> {
        Ok(())
    }
}

/// The vault's `skills/` directory: one sub-directory per skill, each holding a `SKILL.md`.
pub struct SkillsDir(PathBuf);

impl SkillsDir {
    /// A skills directory anywhere.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn into_path(self) -> PathBuf {
        self.0
    }

    /// The directories directly under the root that hold a `SKILL.md`, sorted by path.
    ///
    /// A symlinked entry is skipped: a skill the vault publishes to external agents has to be a
    /// real directory inside it, or the link would decide what those agents load.
    pub fn skills(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.0) else {
            return Vec::new();
        };
        let mut skills = Vec::new();
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let path = entry.path();
            if path.join("SKILL.md").is_file() {
                skills.push(path);
            }
        }
        skills.sort();
        skills
    }

    /// The `SKILL.md` of every configured skill.
    pub fn skill_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for skill in self.skills() {
            files.push(skill.join("SKILL.md"));
        }
        files
    }

    /// The prompt section inlining every skill body, up to the embed budget.
    ///
    /// Bodies go in whole rather than as a catalog of paths, so the agent applies them without
    /// spending a tool call re-reading files it was just handed. One oversized skill is skipped
    /// instead of ending the section, so a later small one still makes it in.
    pub fn prompt(&self) -> String {
        let mut files = SkillTree(&self.0).files();
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
}

/// Every `SKILL.md` reachable below one directory.
#[derive(Clone, Copy)]
struct SkillTree<'a>(&'a Path);

impl SkillTree<'_> {
    /// The files, sorted, so the prompt they build is stable across runs.
    fn files(self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        self.walk(&mut files);
        files.sort();
        files
    }

    fn walk(self, files: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(self.0) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                SkillTree(&path).walk(files);
                continue;
            }
            if file_type.is_file()
                && path
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
            {
                files.push(path);
            }
        }
    }
}

/// The vault's `memories/` directory: markdown migrated out of the user's local agents.
pub struct MemoriesDir(PathBuf);

impl MemoriesDir {
    pub fn into_path(self) -> PathBuf {
        self.0
    }

    /// Copy in whatever the local agents hold that is not here yet, returning how many files
    /// were new.
    pub fn import_external(&self) -> io::Result<usize> {
        ExternalMemories::from_env(self.0.clone()).import()
    }

    /// The prompt section inlining every memory, labelled by its path inside the vault.
    pub fn prompt(&self) -> String {
        let files = MarkdownTree(&self.0).files();
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
            let label = path
                .strip_prefix(&self.0)
                .unwrap_or(&path)
                .to_string_lossy();
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
}

/// Every markdown file reachable below one directory.
///
/// Symlinks and dot-directories are skipped at each level, so a walk stays inside the tree it was
/// pointed at and never picks up an agent's own scratch state.
#[derive(Clone, Copy)]
struct MarkdownTree<'a>(&'a Path);

impl MarkdownTree<'_> {
    /// The files, sorted, so both the migration order and the prompt they build are stable.
    fn files(self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        self.walk(&mut files);
        files.sort();
        files
    }

    fn walk(self, files: &mut Vec<PathBuf>) {
        let Ok(metadata) = std::fs::symlink_metadata(self.0) else {
            return;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(self.0) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                MarkdownTree(&path).walk(files);
                continue;
            }
            if file_type.is_file() && MarkdownPath(&path).is_note() {
                files.push(path);
            }
        }
    }
}

/// Where the local agents keep their memories, and where the vault puts them.
struct ExternalMemories {
    destination: PathBuf,
    claude_projects: PathBuf,
    codex_memories: PathBuf,
    codex_extensions: PathBuf,
}

impl ExternalMemories {
    /// The directories the installed Claude and Codex configs point at.
    fn from_env(destination: PathBuf) -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"));
        let codex = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"));
        Self {
            destination,
            claude_projects: claude.join("projects"),
            codex_memories: codex.join("memories"),
            codex_extensions: codex.join("memories_extensions"),
        }
    }

    /// Copy every memory that is not already in the vault, returning how many were new.
    ///
    /// Existing files are never overwritten, so a memory the user has since edited inside vmux
    /// survives re-running this against an unchanged source.
    fn import(&self) -> io::Result<usize> {
        std::fs::create_dir_all(&self.destination)?;
        Privacy::Directory.apply(&self.destination)?;

        let mut imported = self.import_claude()?;
        imported += MemoryTree {
            source: self.codex_memories.clone(),
            destination: self.destination.join("codex").join("local"),
        }
        .import()?;
        imported += MemoryTree {
            source: self.codex_extensions.clone(),
            destination: self.destination.join("codex").join("extensions"),
        }
        .import()?;
        Ok(imported)
    }

    /// Claude keeps one memory tree per project, so each project directory imports separately and
    /// keeps its own name in the vault.
    fn import_claude(&self) -> io::Result<usize> {
        let destination = self.destination.join("claude").join("projects");
        let Ok(entries) = std::fs::read_dir(&self.claude_projects) else {
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
            imported += MemoryTree {
                source: entry.path().join("memory"),
                destination: destination.join(entry.file_name()),
            }
            .import()?;
        }
        Ok(imported)
    }
}

/// One markdown tree being copied into the vault, keeping its shape.
struct MemoryTree {
    source: PathBuf,
    destination: PathBuf,
}

impl MemoryTree {
    /// Copy every markdown file the source holds, returning how many were new.
    fn import(&self) -> io::Result<usize> {
        let mut imported = 0;
        for file in MarkdownTree(&self.source).files() {
            let Ok(relative) = file.strip_prefix(&self.source) else {
                continue;
            };
            imported += usize::from(self.copy_new(relative)?);
        }
        Ok(imported)
    }

    /// Copy one file unless the vault already has it, reporting whether it was written.
    ///
    /// A copy that fails part way removes what it wrote: a truncated file left behind would read
    /// as already-imported on the next run and never be repaired.
    fn copy_new(&self, relative: &Path) -> io::Result<bool> {
        let destination = self.destination.join(relative);
        let Some(parent) = destination.parent() else {
            return Ok(false);
        };
        std::fs::create_dir_all(parent)?;
        let mut output = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(output) => output,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
            Err(error) => return Err(error),
        };
        match self.fill(relative, &mut output) {
            Ok(()) => Ok(true),
            Err(error) => {
                let _ = std::fs::remove_file(&destination);
                Err(error)
            }
        }
    }

    fn fill(&self, relative: &Path, output: &mut std::fs::File) -> io::Result<()> {
        let mut input = std::fs::File::open(self.source.join(relative))?;
        io::copy(&mut input, output)?;
        output.flush()
    }
}

/// A system prompt with the user vault's skills and memories sections appended.
///
/// Each section opens with a marker sentence and is skipped when the base already carries it, so
/// a launcher that composes prompts in layers cannot embed the vault twice.
pub struct AgentPrompt(String);

impl AgentPrompt {
    /// `base` with both vault sections appended.
    ///
    /// An empty `base` carries no marker, so it yields the two sections alone — what a launcher
    /// with no prompt of its own wants.
    pub fn of(base: &str) -> Self {
        Self(base.to_string()).with_skills().with_memories()
    }

    pub fn into_string(self) -> String {
        self.0
    }

    fn with_skills(mut self) -> Self {
        if !self.0.contains(SKILLS_PROMPT_MARKER) {
            self.push(KnowledgeVault::user().skills().prompt());
        }
        self
    }

    fn with_memories(mut self) -> Self {
        if !self.0.contains(MEMORIES_PROMPT_MARKER) {
            self.push(KnowledgeVault::user().memories().prompt());
        }
        self
    }

    fn push(&mut self, section: String) {
        if section.is_empty() {
            return;
        }
        if self.0.is_empty() {
            self.0 = section;
            return;
        }
        self.0.push_str("\n\n");
        self.0.push_str(&section);
    }
}

/// The YAML block at the top of a markdown note.
#[derive(Clone, Copy)]
pub struct Frontmatter<'a>(&'a str);

impl<'a> Frontmatter<'a> {
    /// The frontmatter of one note's source.
    pub fn of(source: &'a str) -> Self {
        Self(source)
    }

    /// The note's source with `edit` applied: the property renamed, retyped, replaced, removed,
    /// or added — creating the frontmatter block itself when the note has none.
    pub fn apply(&self, edit: &crate::event::FilePropertyEdit) -> Result<String, String> {
        let text = self.0;
        let original = edit.original_key.trim();
        let key = edit.key.trim();
        if !edit.remove && !original.eq_ignore_ascii_case(key) && self.has_property(key) {
            return Err(format!("property already exists: {key}"));
        }
        let Some(slot) = self.slot(original) else {
            if edit.remove {
                return Ok(text.to_string());
            }
            let source = PropertySource::from(edit).render()?;
            return Ok(format!("---\n{source}---\n\n{text}"));
        };
        match slot {
            PropertySlot::Present { start, end } => {
                let mut output = text.to_string();
                if edit.remove {
                    output.replace_range(start..end, "");
                    return Ok(output);
                }
                let source = PropertySource::from(edit).render()?;
                output.replace_range(start..end, &source);
                Ok(output)
            }
            PropertySlot::Absent { close } => {
                if edit.remove {
                    return Ok(text.to_string());
                }
                let source = PropertySource::from(edit).render()?;
                let mut output = text.to_string();
                output.insert_str(close, &source);
                Ok(output)
            }
        }
    }

    /// Whether a property named `key` is already here, so a rename onto it would collide.
    fn has_property(&self, key: &str) -> bool {
        for property in markdown_metadata(self.0).properties {
            if property.key.eq_ignore_ascii_case(key) {
                return true;
            }
        }
        false
    }

    /// Where `key` sits, or `None` when the note has no closed frontmatter block to edit.
    fn slot(&self, key: &str) -> Option<PropertySlot> {
        let mut lines = self.0.split_inclusive('\n');
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
                if let Some(start) = property_start {
                    found = Some((start, offset));
                }
                if key.is_empty() {
                    return Some(PropertySlot::Absent { close: offset });
                }
                let Some((start, end)) = found else {
                    return Some(PropertySlot::Absent { close: offset });
                };
                return Some(PropertySlot::Present { start, end });
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
}

/// Where one property's lines sit inside a note's frontmatter.
#[derive(Clone, Copy)]
enum PropertySlot {
    /// The key is there, spanning `start..end` including the lines its list items occupy.
    Present { start: usize, end: usize },
    /// The key is not there. A new property is inserted at `close`, where the closing `---`
    /// begins, so it lands last inside the block.
    Absent { close: usize },
}

/// One frontmatter property on its way back to YAML.
struct PropertySource<'a> {
    key: &'a str,
    kind: KnowledgePropertyKind,
    values: &'a [String],
}

impl<'a> From<&'a crate::event::FilePropertyEdit> for PropertySource<'a> {
    fn from(edit: &'a crate::event::FilePropertyEdit) -> Self {
        Self {
            key: &edit.key,
            kind: edit.kind,
            values: &edit.values,
        }
    }
}

impl PropertySource<'_> {
    /// The lines this property occupies, or why it cannot be written.
    ///
    /// The rendering is driven by the declared kind rather than by what the values look like, so
    /// a property the user retyped keeps its new shape instead of being re-sniffed on the way out.
    fn render(&self) -> Result<String, String> {
        let key = self.key.trim();
        if key.is_empty()
            || key.len() > 100
            || key.contains([':', '\r', '\n'])
            || key.starts_with(['-', '#'])
        {
            return Err("property name is invalid".to_string());
        }
        let clean = self.clean_values();
        match self.kind {
            KnowledgePropertyKind::Tags | KnowledgePropertyKind::List => {
                if clean.is_empty() {
                    return Ok(format!("{key}: []\n"));
                }
                let mut source = format!("{key}:\n");
                for value in &clean {
                    source.push_str(&format!("  - {}\n", YamlScalar(value)));
                }
                Ok(source)
            }
            KnowledgePropertyKind::Checkbox => {
                let checked = clean
                    .first()
                    .is_some_and(|value| value.eq_ignore_ascii_case("true"));
                Ok(format!("{key}: {checked}\n"))
            }
            KnowledgePropertyKind::Number => {
                let value = clean.first().map(String::as_str).unwrap_or("0");
                if value.parse::<f64>().is_err() {
                    return Err("number property requires a valid number".to_string());
                }
                Ok(format!("{key}: {value}\n"))
            }
            KnowledgePropertyKind::Link => {
                let value = clean.first().cloned().unwrap_or_default();
                let value =
                    if value.is_empty() || (value.starts_with("[[") && value.ends_with("]]")) {
                        value
                    } else {
                        format!("[[{value}]]")
                    };
                Ok(format!("{key}: {}\n", YamlScalar(&value)))
            }
            KnowledgePropertyKind::Date | KnowledgePropertyKind::Text => {
                let value = clean.first().map(String::as_str).unwrap_or_default();
                Ok(format!("{key}: {}\n", YamlScalar(value)))
            }
        }
    }

    /// The values with line breaks flattened and blanks dropped, so one value stays one line.
    fn clean_values(&self) -> Vec<String> {
        let mut clean = Vec::new();
        for value in self.values {
            let value = value.trim().replace(['\r', '\n'], " ");
            if !value.is_empty() {
                clean.push(value);
            }
        }
        clean
    }
}

/// A YAML scalar: always quoted, with backslashes and quotes escaped and line breaks flattened,
/// so no value a user types can end the frontmatter block early.
struct YamlScalar<'a>(&'a str);

impl std::fmt::Display for YamlScalar<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let escaped = self
            .0
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace(['\r', '\n'], " ");
        write!(formatter, "\"{escaped}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_frontmatter_properties_without_touching_body() {
        let text = "---\ntitle: Old\ntags:\n  - alpha\nstatus: draft\n---\n\nBody\n";
        let edited = Frontmatter::of(text)
            .apply(&crate::event::FilePropertyEdit {
                original_key: "tags".into(),
                key: "tags".into(),
                kind: KnowledgePropertyKind::Tags,
                values: vec!["alpha".into(), "beta".into()],
                remove: false,
            })
            .unwrap();
        let renamed = Frontmatter::of(&edited)
            .apply(&crate::event::FilePropertyEdit {
                original_key: "status".into(),
                key: "stage".into(),
                kind: KnowledgePropertyKind::Text,
                values: vec!["ready".into()],
                remove: false,
            })
            .unwrap();
        assert!(renamed.contains("tags:\n  - \"alpha\"\n  - \"beta\"\n"));
        assert!(renamed.contains("stage: \"ready\"\n"));
        assert!(!renamed.contains("status:"));
        assert!(renamed.ends_with("\nBody\n"));
        assert!(
            Frontmatter::of(&renamed)
                .apply(&crate::event::FilePropertyEdit {
                    original_key: "stage".into(),
                    key: "title".into(),
                    kind: KnowledgePropertyKind::Text,
                    values: vec!["Duplicate".into()],
                    remove: false,
                })
                .is_err()
        );

        let with_link = Frontmatter::of(&renamed)
            .apply(&crate::event::FilePropertyEdit {
                original_key: String::new(),
                key: "related".into(),
                kind: KnowledgePropertyKind::Link,
                values: vec!["Roadmap".into()],
                remove: false,
            })
            .unwrap();
        let with_date = Frontmatter::of(&with_link)
            .apply(&crate::event::FilePropertyEdit {
                original_key: String::new(),
                key: "due".into(),
                kind: KnowledgePropertyKind::Date,
                values: vec!["2026-07-25".into()],
                remove: false,
            })
            .unwrap();
        let metadata = markdown_metadata(&with_date);
        let related = metadata
            .properties
            .iter()
            .find(|property| property.key == "related")
            .unwrap();
        assert_eq!(related.kind, KnowledgePropertyKind::Link);
        assert_eq!(related.values, ["[[Roadmap]]"]);
        let due = metadata
            .properties
            .iter()
            .find(|property| property.key == "due")
            .unwrap();
        assert_eq!(due.kind, KnowledgePropertyKind::Date);
        assert_eq!(due.values, ["2026-07-25"]);
    }

    #[test]
    fn writes_private_markdown_note_under_projects_by_default() {
        let temp = tempfile::tempdir().unwrap();
        let path = KnowledgeVault::at(temp.path())
            .write_note(None, "YC Startup School", "Useful content")
            .unwrap();
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
        let vault = KnowledgeVault::at(temp.path());

        assert!(
            vault
                .write_note(Some("../outside.md"), "Title", "Body")
                .is_err()
        );
        assert!(
            vault
                .write_note(Some("projects/note.txt"), "Title", "Body")
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_inside_knowledge_root() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("projects")).unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("projects/linked")).unwrap();

        assert!(
            KnowledgeVault::at(temp.path())
                .write_note(Some("projects/linked/note.md"), "Title", "Body")
                .is_err()
        );
    }

    #[test]
    fn loads_sorted_skill_bodies_without_file_catalog() {
        let temp = tempfile::tempdir().unwrap();
        let vault = KnowledgeVault::at(temp.path());
        let skills = vault.skills().into_path();
        let beta = skills.join("beta");
        let alpha = skills.join("alpha");
        std::fs::create_dir_all(&beta).unwrap();
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::write(beta.join("SKILL.md"), "# Beta").unwrap();
        std::fs::write(alpha.join("SKILL.md"), "# Alpha").unwrap();
        let prompt = vault.skills().prompt();
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
        let external = ExternalMemories {
            destination: destination.clone(),
            claude_projects: claude.clone(),
            codex_memories: codex,
            codex_extensions: extensions,
        };

        assert_eq!(external.import().unwrap(), 4);
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
        assert_eq!(external.import().unwrap(), 0);
        assert_eq!(std::fs::read_to_string(migrated).unwrap(), "vmux edit");
    }

    #[test]
    fn embeds_every_migrated_memory_in_sorted_order() {
        let temp = tempfile::tempdir().unwrap();
        let vault = KnowledgeVault::at(temp.path());
        let memories = vault.memories().into_path();
        std::fs::create_dir_all(memories.join("nested")).unwrap();
        std::fs::write(memories.join("z.md"), "Zulu").unwrap();
        std::fs::write(memories.join("nested/a.markdown"), "Alpha").unwrap();
        let prompt = vault.memories().prompt();
        assert!(prompt.find("nested/a.markdown").unwrap() < prompt.find("z.md").unwrap());
        assert!(prompt.contains("Alpha"));
        assert!(prompt.contains("Zulu"));
    }
}
