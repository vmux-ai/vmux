use std::collections::{HashMap, HashSet};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use super::{MarkdownMetadata, WikiLink, markdown_metadata, wiki_links};
use crate::event::{MdBlock, MdInline, NoteBlock};

const MAX_NOTES: usize = 2_048;
const MAX_NOTE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DEPTH: usize = 16;

#[derive(Clone, Debug)]
struct IndexedLink {
    link: WikiLink,
    line: u32,
    preview: String,
}

#[derive(Clone, Debug)]
struct KnowledgeNote {
    path: PathBuf,
    relative: String,
    title: String,
    aliases: Vec<String>,
    text: String,
    lowercase_text: String,
    headings: HashMap<String, u32>,
    blocks: HashMap<String, u32>,
    links: Vec<IndexedLink>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeResolvedLink {
    pub path: PathBuf,
    pub title: String,
    pub line: Option<u32>,
    pub exists: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeSearchHit {
    pub path: PathBuf,
    pub title: String,
    pub line: u32,
    pub preview: String,
    pub score: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeBacklink {
    pub path: PathBuf,
    pub title: String,
    pub line: u32,
    pub preview: String,
    pub unlinked: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeBrokenLink {
    pub target: String,
    pub line: u32,
}

#[derive(bevy::prelude::Resource, Clone, Debug, Default)]
pub struct KnowledgeIndex {
    root: PathBuf,
    notes: Vec<KnowledgeNote>,
    keys: HashMap<String, Vec<usize>>,
    backlinks: HashMap<PathBuf, Vec<KnowledgeBacklink>>,
    broken: HashMap<PathBuf, Vec<KnowledgeBrokenLink>>,
    loaded: bool,
}

#[derive(Clone, Debug)]
struct RenameEdit {
    source: PathBuf,
    replacements: Vec<(usize, usize, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct KnowledgeRenamePlan {
    old: PathBuf,
    new: PathBuf,
    edits: Vec<RenameEdit>,
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md")
                || extension.eq_ignore_ascii_case("markdown")
                || extension.eq_ignore_ascii_case("mdx")
        })
}

fn normalized(value: &str) -> String {
    let value = value.trim();
    let value = [".markdown", ".mdx", ".md"]
        .into_iter()
        .find_map(|extension| value.strip_suffix(extension))
        .unwrap_or(value);
    value
        .replace('\\', "/")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn first_heading(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let value = line.trim();
        value
            .strip_prefix("# ")
            .map(|value| value.trim_end_matches('#').trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn line_of(text: &str, byte: usize) -> u32 {
    text[..byte.min(text.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count() as u32
}

fn line_preview(text: &str, line: u32) -> String {
    text.lines()
        .nth(line as usize)
        .unwrap_or_default()
        .trim()
        .chars()
        .take(240)
        .collect()
}

fn anchors(text: &str) -> (HashMap<String, u32>, HashMap<String, u32>) {
    let mut headings = HashMap::new();
    let mut blocks = HashMap::new();
    let mut fenced = false;
    for (line, raw) in text.lines().enumerate() {
        let value = raw.trim();
        if value.starts_with("```") || value.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let hashes = value.bytes().take_while(|byte| *byte == b'#').count();
        if (1..=6).contains(&hashes)
            && value
                .as_bytes()
                .get(hashes)
                .is_some_and(u8::is_ascii_whitespace)
        {
            let heading = value[hashes..].trim().trim_end_matches('#').trim();
            if !heading.is_empty() {
                headings.entry(normalized(heading)).or_insert(line as u32);
            }
        }
        if let Some((prefix, block)) = value.rsplit_once('^')
            && prefix.chars().next_back().is_some_and(char::is_whitespace)
            && !block.is_empty()
            && block.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            blocks.entry(normalized(block)).or_insert(line as u32);
        }
    }
    (headings, blocks)
}

fn collect_files(directory: &Path, depth: usize, output: &mut Vec<PathBuf>) -> io::Result<()> {
    if depth > MAX_DEPTH || output.len() >= MAX_NOTES {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(directory)?.flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());
    for entry in entries {
        if output.len() >= MAX_NOTES {
            break;
        }
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            collect_files(&path, depth + 1, output)?;
        } else if kind.is_file() && is_markdown(&path) {
            output.push(path);
        }
    }
    Ok(())
}

fn read_note(path: &Path) -> io::Result<String> {
    let file = std::fs::File::open(path)?;
    let mut text = String::new();
    file.take(MAX_NOTE_BYTES).read_to_string(&mut text)?;
    Ok(text)
}

fn relative_without_extension(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut relative = relative.to_path_buf();
    relative.set_extension("");
    relative.to_string_lossy().replace('\\', "/")
}

fn normalized_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        path.parent()
            .and_then(|parent| parent.canonicalize().ok())
            .and_then(|parent| path.file_name().map(|name| parent.join(name)))
            .unwrap_or_else(|| path.to_path_buf())
    })
}

fn note_title(path: &Path, metadata: &MarkdownMetadata, text: &str) -> String {
    if !metadata.title.is_empty() {
        metadata.title.clone()
    } else if let Some(title) = first_heading(text) {
        title
    } else {
        path.file_stem()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

fn safe_candidate(root: &Path, source: &Path, target: &str) -> Option<PathBuf> {
    let target = Path::new(target);
    if target.is_absolute()
        || target
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    let base = if target.components().count() > 1 {
        root.to_path_buf()
    } else {
        normalized_path(source)
            .parent()
            .unwrap_or(root)
            .to_path_buf()
    };
    let mut path = base.join(target);
    if path.extension().is_none() {
        path.set_extension("md");
    }
    path.starts_with(root).then_some(path)
}

fn contains_term(line: &str, term: &str) -> bool {
    let line = line.to_lowercase();
    let term = term.to_lowercase();
    let mut offset = 0;
    while let Some(found) = line[offset..].find(&term) {
        let start = offset + found;
        let end = start + term.len();
        let left = line[..start].chars().next_back();
        let right = line[end..].chars().next();
        if left.is_none_or(|character| !character.is_alphanumeric())
            && right.is_none_or(|character| !character.is_alphanumeric())
        {
            return true;
        }
        offset = end;
    }
    false
}

impl KnowledgeIndex {
    pub fn build(root: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(root)?;
        let root = root.canonicalize()?;
        let mut files = Vec::new();
        collect_files(&root, 0, &mut files)?;
        let mut notes = Vec::new();
        for path in files {
            let text = match read_note(&path) {
                Ok(text) => text,
                Err(_) => continue,
            };
            let metadata = markdown_metadata(&text);
            let title = note_title(&path, &metadata, &text);
            let relative = relative_without_extension(&root, &path);
            let (headings, blocks) = anchors(&text);
            let lowercase_text = text.to_lowercase();
            let links = wiki_links(&text)
                .into_iter()
                .map(|link| {
                    let line = line_of(&text, link.start);
                    IndexedLink {
                        preview: line_preview(&text, line),
                        line,
                        link,
                    }
                })
                .collect();
            notes.push(KnowledgeNote {
                path,
                relative,
                title,
                aliases: metadata.aliases,
                text,
                lowercase_text,
                headings,
                blocks,
                links,
            });
        }
        notes.sort_by(|left, right| left.relative.cmp(&right.relative));
        let mut index = Self {
            root,
            notes,
            keys: HashMap::new(),
            backlinks: HashMap::new(),
            broken: HashMap::new(),
            loaded: true,
        };
        index.rebuild_keys();
        index.rebuild_links();
        Ok(index)
    }

    pub fn loaded(&self) -> bool {
        self.loaded
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn rebuild_keys(&mut self) {
        for (index, note) in self.notes.iter().enumerate() {
            let mut values = vec![note.relative.clone(), note.title.clone()];
            if let Some(stem) = note.path.file_stem() {
                values.push(stem.to_string_lossy().into_owned());
            }
            values.extend(note.aliases.clone());
            let mut seen = HashSet::new();
            for value in values {
                let key = normalized(&value);
                if !key.is_empty() && seen.insert(key.clone()) {
                    self.keys.entry(key).or_default().push(index);
                }
            }
        }
    }

    fn note_index(&self, source: &Path, target: &str) -> Option<usize> {
        let source = normalized_path(source);
        if target.trim().is_empty() {
            return self.notes.iter().position(|note| note.path == source);
        }
        let key = normalized(target);
        let candidates = self.keys.get(&key)?;
        let source_parent = source.parent();
        candidates.iter().copied().min_by_key(|index| {
            let note = &self.notes[*index];
            (
                note.path.parent() != source_parent,
                note.relative.matches('/').count(),
                note.relative.len(),
            )
        })
    }

    pub fn resolve(
        &self,
        source: &Path,
        note: &str,
        anchor: Option<&str>,
    ) -> KnowledgeResolvedLink {
        let Some(index) = self.note_index(source, note) else {
            return KnowledgeResolvedLink {
                path: safe_candidate(&self.root, source, note).unwrap_or_default(),
                title: note.trim().to_string(),
                line: None,
                exists: false,
            };
        };
        let target = &self.notes[index];
        let line = anchor.and_then(|anchor| {
            anchor
                .strip_prefix('^')
                .and_then(|anchor| target.blocks.get(&normalized(anchor)).copied())
                .or_else(|| target.headings.get(&normalized(anchor)).copied())
        });
        KnowledgeResolvedLink {
            path: target.path.clone(),
            title: target.title.clone(),
            line,
            exists: true,
        }
    }

    pub fn resolve_blocks(&self, source: &Path, blocks: &mut [NoteBlock]) {
        for block in blocks {
            resolve_block(self, source, &mut block.block);
        }
    }

    fn rebuild_links(&mut self) {
        for source in &self.notes {
            for indexed in &source.links {
                let resolved = self.resolve(
                    &source.path,
                    &indexed.link.note,
                    indexed.link.anchor.as_deref(),
                );
                if resolved.exists {
                    self.backlinks
                        .entry(resolved.path)
                        .or_default()
                        .push(KnowledgeBacklink {
                            path: source.path.clone(),
                            title: source.title.clone(),
                            line: indexed.line,
                            preview: indexed.preview.clone(),
                            unlinked: false,
                        });
                } else {
                    self.broken
                        .entry(source.path.clone())
                        .or_default()
                        .push(KnowledgeBrokenLink {
                            target: indexed.link.note.clone(),
                            line: indexed.line,
                        });
                }
            }
        }
        for references in self.backlinks.values_mut() {
            references.sort_by(|left, right| {
                left.title
                    .to_lowercase()
                    .cmp(&right.title.to_lowercase())
                    .then(left.line.cmp(&right.line))
            });
        }
    }

    pub fn backlinks(&self, path: &Path) -> Vec<KnowledgeBacklink> {
        self.backlinks
            .get(&normalized_path(path))
            .cloned()
            .unwrap_or_default()
    }

    pub fn broken_links(&self, path: &Path) -> Vec<KnowledgeBrokenLink> {
        self.broken
            .get(&normalized_path(path))
            .cloned()
            .unwrap_or_default()
    }

    pub fn unlinked_mentions(&self, path: &Path, limit: usize) -> Vec<KnowledgeBacklink> {
        let path = normalized_path(path);
        let Some(target) = self.notes.iter().find(|note| note.path == path) else {
            return Vec::new();
        };
        let mut terms = vec![target.title.clone()];
        terms.extend(target.aliases.clone());
        terms.retain(|term| term.chars().count() >= 3);
        let linked_sources = self
            .backlinks(&path)
            .into_iter()
            .map(|reference| reference.path)
            .collect::<HashSet<_>>();
        let mut results = Vec::new();
        for note in &self.notes {
            if note.path == path || linked_sources.contains(&note.path) {
                continue;
            }
            for (line, value) in note.text.lines().enumerate() {
                if terms.iter().any(|term| contains_term(value, term)) {
                    results.push(KnowledgeBacklink {
                        path: note.path.clone(),
                        title: note.title.clone(),
                        line: line as u32,
                        preview: value.trim().chars().take(240).collect(),
                        unlinked: true,
                    });
                    break;
                }
            }
            if results.len() >= limit {
                break;
            }
        }
        results
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<KnowledgeSearchHit> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        let terms = query.split_whitespace().collect::<Vec<_>>();
        let mut hits = Vec::new();
        for note in &self.notes {
            let title = note.title.to_lowercase();
            let relative = note.relative.to_lowercase();
            let aliases = note
                .aliases
                .iter()
                .map(|alias| alias.to_lowercase())
                .collect::<Vec<_>>();
            let body = &note.lowercase_text;
            if !terms.iter().all(|term| {
                title.contains(term)
                    || relative.contains(term)
                    || aliases.iter().any(|alias| alias.contains(term))
                    || body.contains(term)
            }) {
                continue;
            }
            let mut score = if title == query {
                1_000
            } else if title.starts_with(&query) {
                750
            } else if title.contains(&query) {
                600
            } else if aliases.iter().any(|alias| alias == &query) {
                550
            } else if relative.contains(&query) {
                400
            } else {
                100
            };
            let mut line = 0;
            let mut preview = note.title.clone();
            for (index, value) in note.text.lines().enumerate() {
                let lower = value.to_lowercase();
                if terms.iter().all(|term| lower.contains(term)) {
                    line = index as u32;
                    preview = value.trim().chars().take(240).collect();
                    score += 100;
                    break;
                }
            }
            hits.push(KnowledgeSearchHit {
                path: note.path.clone(),
                title: note.title.clone(),
                line,
                preview,
                score,
            });
        }
        hits.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
                .then_with(|| left.path.cmp(&right.path))
        });
        hits.truncate(limit);
        hits
    }

    pub fn completions(&self, prefix: &str, limit: usize) -> Vec<(String, String)> {
        let prefix = normalized(prefix);
        let mut values = self
            .notes
            .iter()
            .filter_map(|note| {
                let title = normalized(&note.title);
                let relative = normalized(&note.relative);
                let alias_match = note
                    .aliases
                    .iter()
                    .any(|alias| normalized(alias).contains(&prefix));
                (prefix.is_empty()
                    || title.contains(&prefix)
                    || relative.contains(&prefix)
                    || alias_match)
                    .then(|| {
                        (
                            !title.starts_with(&prefix),
                            note.title.clone(),
                            note.relative.clone(),
                        )
                    })
            })
            .collect::<Vec<_>>();
        values.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.to_lowercase().cmp(&right.1.to_lowercase()))
        });
        values
            .into_iter()
            .take(limit)
            .map(|(_, title, relative)| (title, relative))
            .collect()
    }

    pub fn note_by_query(&self, query: &str) -> Option<(PathBuf, String, String)> {
        let source = self.root.join("index.md");
        let index = self.note_index(&source, query)?;
        let note = &self.notes[index];
        Some((note.path.clone(), note.title.clone(), note.text.clone()))
    }
}

fn resolve_inlines(index: &KnowledgeIndex, source: &Path, inlines: &mut [MdInline]) {
    for inline in inlines {
        match inline {
            MdInline::Strong(children)
            | MdInline::Emph(children)
            | MdInline::Strike(children)
            | MdInline::Link {
                inlines: children, ..
            } => resolve_inlines(index, source, children),
            MdInline::WikiLink {
                target,
                label,
                path,
                line,
                exists,
                ..
            } => {
                let (note, anchor) = target
                    .split_once('#')
                    .map(|(note, anchor)| (note, Some(anchor)))
                    .unwrap_or((target.as_str(), None));
                let resolved = index.resolve(source, note, anchor);
                *path = resolved.path.to_string_lossy().into_owned();
                *line = resolved.line;
                *exists = resolved.exists;
                if label.is_empty() {
                    *label = resolved.title;
                }
            }
            MdInline::Text(_)
            | MdInline::Code(_)
            | MdInline::Image { .. }
            | MdInline::SoftBreak
            | MdInline::HardBreak => {}
        }
    }
}

fn resolve_block(index: &KnowledgeIndex, source: &Path, block: &mut MdBlock) {
    match block {
        MdBlock::Heading { inlines, .. } | MdBlock::Paragraph { inlines } => {
            resolve_inlines(index, source, inlines);
        }
        MdBlock::List { items, .. } => {
            for item in items {
                for block in &mut item.blocks {
                    resolve_block(index, source, block);
                }
            }
        }
        MdBlock::BlockQuote { blocks } => {
            for block in blocks {
                resolve_block(index, source, block);
            }
        }
        MdBlock::Table { header, rows, .. } => {
            for cell in header {
                resolve_inlines(index, source, cell);
            }
            for row in rows {
                for cell in row {
                    resolve_inlines(index, source, cell);
                }
            }
        }
        MdBlock::CodeBlock { .. } | MdBlock::ThematicBreak | MdBlock::Html { .. } => {}
    }
}

fn replacement_text(link: &WikiLink, note: &str) -> String {
    let mut replacement = String::new();
    if link.embed {
        replacement.push('!');
    }
    replacement.push_str("[[");
    replacement.push_str(note);
    if let Some(anchor) = &link.anchor {
        replacement.push('#');
        replacement.push_str(anchor);
    }
    if let Some(label) = &link.label {
        replacement.push('|');
        replacement.push_str(label);
    }
    replacement.push_str("]]");
    replacement
}

impl KnowledgeRenamePlan {
    pub fn build(index: &KnowledgeIndex, old: &Path, new: &Path) -> Self {
        let old = normalized_path(old);
        let new = normalized_path(new);
        let mut edits = Vec::new();
        for source in &index.notes {
            let mut replacements = Vec::new();
            for indexed in &source.links {
                let resolved = index.resolve(
                    &source.path,
                    &indexed.link.note,
                    indexed.link.anchor.as_deref(),
                );
                if !resolved.exists || !resolved.path.starts_with(&old) {
                    continue;
                }
                if indexed.link.note.is_empty() && source.path.starts_with(&old) {
                    continue;
                }
                let suffix = resolved.path.strip_prefix(&old).unwrap_or(Path::new(""));
                let destination = new.join(suffix);
                let note = relative_without_extension(&index.root, &destination);
                replacements.push((
                    indexed.link.start,
                    indexed.link.end,
                    replacement_text(&indexed.link, &note),
                ));
            }
            if !replacements.is_empty() {
                edits.push(RenameEdit {
                    source: source.path.clone(),
                    replacements,
                });
            }
        }
        Self { old, new, edits }
    }

    pub fn apply(self) -> io::Result<()> {
        for edit in self.edits {
            let source = edit
                .source
                .strip_prefix(&self.old)
                .ok()
                .map(|suffix| self.new.join(suffix))
                .unwrap_or(edit.source);
            let mut text = std::fs::read_to_string(&source)?;
            let mut replacements = edit.replacements;
            replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
            for (start, end, replacement) in replacements {
                if start <= end && end <= text.len() {
                    text.replace_range(start..end, &replacement);
                }
            }
            std::fs::write(source, text)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, KnowledgeIndex) {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("projects")).unwrap();
        std::fs::write(
            temp.path().join("projects/alpha.md"),
            "---\ntitle: Alpha Project\naliases: [Alpha, A Project]\n---\n\n# Overview\n\nAlpha body ^alpha-block\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("projects/source.md"),
            "# Source\n\nSee [[Alpha#Overview|the overview]] and [[Missing Note]].\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("projects/mention.md"),
            "Alpha Project appears without a link.\n",
        )
        .unwrap();
        let index = KnowledgeIndex::build(temp.path()).unwrap();
        (temp, index)
    }

    #[test]
    fn resolves_titles_aliases_headings_and_candidates() {
        let (temp, index) = fixture();
        let source = temp.path().join("projects/source.md");
        let resolved = index.resolve(&source, "Alpha", Some("Overview"));
        assert!(resolved.exists);
        assert_eq!(resolved.line, Some(5));
        assert_eq!(resolved.title, "Alpha Project");
        let missing = index.resolve(&source, "Missing Note", None);
        assert!(!missing.exists);
        assert_eq!(
            missing.path,
            normalized_path(&temp.path().join("projects/Missing Note.md"))
        );
    }

    #[test]
    fn block_anchors_require_a_terminal_whitespace_delimited_identifier() {
        let (_, blocks) = anchors("valid ^block-id\n2^10\nx ^ y\n`code ^bad`\n");
        assert_eq!(blocks, HashMap::from([("block-id".to_string(), 0)]));
    }

    #[test]
    fn indexes_backlinks_broken_links_mentions_and_search() {
        let (temp, index) = fixture();
        let alpha = temp.path().join("projects/alpha.md");
        let backlinks = index.backlinks(&alpha);
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].title, "Source");
        let source = temp.path().join("projects/source.md");
        assert_eq!(index.broken_links(&source)[0].target, "Missing Note");
        let mentions = index.unlinked_mentions(&alpha, 10);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].title, "mention");
        assert_eq!(index.search("overview", 10)[0].title, "Alpha Project");
    }

    #[test]
    fn rename_plan_rewrites_resolved_links_and_preserves_aliases() {
        let (temp, index) = fixture();
        let old = temp.path().join("projects/alpha.md");
        let new = temp.path().join("projects/renamed.md");
        let plan = KnowledgeRenamePlan::build(&index, &old, &new);
        std::fs::rename(&old, &new).unwrap();
        plan.apply().unwrap();
        let source = std::fs::read_to_string(temp.path().join("projects/source.md")).unwrap();
        assert!(source.contains("[[projects/renamed#Overview|the overview]]"));
        assert!(source.contains("[[Missing Note]]"));
    }
}
