use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[cfg(all(unix, test))]
use std::os::unix::fs::MetadataExt;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use pulldown_cmark::{Event, Options, Parser, Tag, html};

use crate::event::{NoteReadResponse, NoteSummary};

const NOTE_MAX_BYTES: u64 = 2 * 1024 * 1024;
const FILE_STEM_MAX_BYTES: usize = 180;

#[derive(Clone, Debug)]
pub struct NoteDocument {
    pub summary: NoteSummary,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct NoteIndexEntry {
    pub document: NoteDocument,
    size: u64,
    search_text: String,
}

pub fn vault_dir() -> PathBuf {
    vmux_core::profile::config_dir().join("knowledge")
}

pub fn ensure_vault(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    for directory in [
        "skills",
        "decisions",
        "projects",
        "meetings",
        "runbooks",
        "handbook",
        "research",
        "templates",
    ] {
        std::fs::create_dir_all(root.join(directory))?;
    }
    #[cfg(unix)]
    {
        for directory in std::iter::once(root.to_path_buf()).chain(
            [
                "skills",
                "decisions",
                "projects",
                "meetings",
                "runbooks",
                "handbook",
                "research",
                "templates",
            ]
            .into_iter()
            .map(|directory| root.join(directory)),
        ) {
            let permissions = std::fs::metadata(&directory)?.permissions();
            if permissions.mode() & 0o777 != 0o700 {
                std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
            }
        }
    }
    Ok(())
}

pub fn canonical_vault(root: &Path) -> std::io::Result<PathBuf> {
    ensure_vault(root)?;
    root.canonicalize()
}

pub fn build_index(
    root: &Path,
    previous: &[NoteIndexEntry],
) -> std::io::Result<Vec<NoteIndexEntry>> {
    let root = canonical_vault(root)?;
    let mut paths = Vec::new();
    collect_note_paths(&root, &root, &mut paths)?;
    let previous: HashMap<&str, &NoteIndexEntry> = previous
        .iter()
        .map(|entry| (entry.document.summary.path.as_str(), entry))
        .collect();
    let mut entries = Vec::with_capacity(paths.len());
    for path in paths {
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        let modified_at = modified_millis(&metadata);
        let path_string = path.to_string_lossy();
        if let Some(entry) = previous.get(path_string.as_ref())
            && entry.size == metadata.len()
            && entry.document.summary.modified_at == modified_at
        {
            entries.push((*entry).clone());
            continue;
        }
        let Ok(document) = read_document(&root, &path) else {
            continue;
        };
        entries.push(index_entry(document, metadata.len()));
    }
    entries.sort_by(|a, b| {
        b.document
            .summary
            .modified_at
            .cmp(&a.document.summary.modified_at)
            .then_with(|| {
                a.document
                    .summary
                    .title
                    .to_lowercase()
                    .cmp(&b.document.summary.title.to_lowercase())
            })
    });
    Ok(entries)
}

pub fn query_index(
    entries: &[NoteIndexEntry],
    query: &str,
    offset: u32,
    limit: u32,
) -> (Vec<NoteSummary>, u32, bool) {
    let query = query.trim().to_lowercase();
    let matching: Vec<&NoteIndexEntry> = entries
        .iter()
        .filter(|entry| query.is_empty() || entry.search_text.contains(&query))
        .collect();
    let total = matching.len() as u32;
    let notes = matching
        .into_iter()
        .skip(offset as usize)
        .take(limit.max(1) as usize)
        .map(|entry| entry.document.summary.clone())
        .collect::<Vec<_>>();
    let has_more = offset.saturating_add(notes.len() as u32) < total;
    (notes, total, has_more)
}

pub fn list_notes(root: &Path, query: &str) -> std::io::Result<Vec<NoteSummary>> {
    let entries = build_index(root, &[])?;
    Ok(query_index(&entries, query, 0, u32::MAX).0)
}

pub fn read_note(root: &Path, requested: &Path) -> Result<NoteDocument, String> {
    let root = canonical_vault(root).map_err(|error| error.to_string())?;
    let path = resolve_note_path_in(&root, requested)?;
    read_document(&root, &path).map_err(|error| error.to_string())
}

pub fn resolve_note_path(root: &Path, requested: &Path) -> Result<PathBuf, String> {
    let root = canonical_vault(root).map_err(|error| error.to_string())?;
    resolve_note_path_in(&root, requested)
}

pub fn read_response(document: &NoteDocument, root: &Path, request_id: u64) -> NoteReadResponse {
    let body = note_body(&document.content);
    NoteReadResponse {
        request_id,
        path: document.summary.path.clone(),
        relative_path: document.summary.relative_path.clone(),
        title: document.summary.title.clone(),
        source: document.content.clone(),
        html: render_markdown(body, Path::new(&document.summary.path), root),
        modified_at: document.summary.modified_at,
        word_count: markdown_word_count(body),
    }
}

pub fn write_note(root: &Path, requested: &Path, source: &str) -> Result<NoteDocument, String> {
    if source.len() as u64 > NOTE_MAX_BYTES {
        return Err("note exceeds size limit".to_string());
    }
    let root = canonical_vault(root).map_err(|error| error.to_string())?;
    let path = resolve_note_path_in(&root, requested)?;
    let parent = path
        .parent()
        .ok_or_else(|| "note has no parent directory".to_string())?;
    let file_name = path
        .file_name()
        .ok_or_else(|| "note has no file name".to_string())?
        .to_string_lossy();
    let nonce = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".{file_name}.vmux-{nonce}"));
    let result = (|| -> std::io::Result<()> {
        let mut file = secure_create(&temporary)?;
        file.write_all(source.as_bytes())?;
        file.sync_data()?;
        std::fs::rename(&temporary, &path)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    read_document(&root, &path).map_err(|error| error.to_string())
}

pub fn create_note(root: &Path, title: &str) -> std::io::Result<NoteDocument> {
    let root = canonical_vault(root)?;
    let title = clean_title(title);
    let stem = clean_file_stem(&title);
    let mut index = 1_u32;
    let path = loop {
        let file_name = if index == 1 {
            format!("{stem}.md")
        } else {
            format!("{stem} {index}.md")
        };
        let candidate = root.join(file_name);
        match secure_create(&candidate) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(format!("# {title}\n\n").as_bytes()) {
                    drop(file);
                    let _ = std::fs::remove_file(&candidate);
                    return Err(error);
                }
                break candidate;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => index += 1,
            Err(error) => return Err(error),
        }
    };
    read_document(&root, &path)
}

fn secure_create(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    if let Err(error) = file.set_permissions(std::fs::Permissions::from_mode(0o600)) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    Ok(file)
}

fn secure_read(path: &Path) -> std::io::Result<(std::fs::Metadata, String)> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "note is not a regular file",
        ));
    }
    if metadata.len() > NOTE_MAX_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "note exceeds size limit",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(NOTE_MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > NOTE_MAX_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "note exceeds size limit",
        ));
    }
    let content = String::from_utf8(bytes)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "note is not UTF-8"))?;
    Ok((metadata, content))
}

fn collect_note_paths(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if file_type.is_dir() {
            collect_note_paths(root, &path, out)?;
        } else if file_type.is_file() && is_markdown(&path) && path.starts_with(root) {
            out.push(path);
        }
    }
    Ok(())
}

fn read_document(root: &Path, path: &Path) -> std::io::Result<NoteDocument> {
    let (metadata, content) = secure_read(path)?;
    let relative_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let title = note_title(path, &content);
    let excerpt = note_excerpt(&content);
    Ok(NoteDocument {
        summary: NoteSummary {
            path: path.to_string_lossy().into_owned(),
            relative_path,
            title,
            excerpt,
            modified_at: modified_millis(&metadata),
        },
        content,
    })
}

fn index_entry(document: NoteDocument, size: u64) -> NoteIndexEntry {
    let search_text = format!(
        "{}\n{}\n{}",
        document.summary.title, document.summary.relative_path, document.content
    )
    .to_lowercase();
    NoteIndexEntry {
        document,
        size,
        search_text,
    }
}

fn modified_millis(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn resolve_note_path_in(root: &Path, requested: &Path) -> Result<PathBuf, String> {
    let requested = requested
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !requested.starts_with(root) || !requested.is_file() || !is_markdown(&requested) {
        return Err("note path is outside the vault".to_string());
    }
    Ok(requested)
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extension.eq_ignore_ascii_case("md")
                || extension.eq_ignore_ascii_case("markdown")
                || extension.eq_ignore_ascii_case("mdx")
        })
        .unwrap_or(false)
}

fn note_title(path: &Path, content: &str) -> String {
    if let Some(title) = content
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
    {
        return title.to_string();
    }
    path.file_stem()
        .map(|stem| stem.to_string_lossy().replace(['-', '_'], " "))
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "Untitled".to_string())
}

fn strip_frontmatter(content: &str) -> &str {
    let mut lines = content.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return content;
    };
    if first.trim() != "---" {
        return content;
    }
    let mut consumed = first.len();
    for line in lines {
        consumed += line.len();
        if line.trim() == "---" {
            return content[consumed..].trim_start_matches(['\r', '\n']);
        }
    }
    content
}

fn note_body(content: &str) -> &str {
    let content = strip_frontmatter(content);
    let Some(first_line_end) = content.find('\n') else {
        return if content.trim_start().starts_with("# ") {
            ""
        } else {
            content
        };
    };
    if content[..first_line_end].trim_start().starts_with("# ") {
        content[first_line_end + 1..].trim_start_matches(['\r', '\n'])
    } else {
        content
    }
}

fn note_excerpt(content: &str) -> String {
    let mut text = String::new();
    for line in note_body(content).lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("```") {
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(line);
        if text.chars().count() >= 220 {
            break;
        }
    }
    truncate_chars(text.trim(), 220)
}

fn truncate_chars(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{}…", truncated.trim_end())
    } else {
        truncated
    }
}

fn truncate_utf8_bytes(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn clean_title(title: &str) -> String {
    let title: String = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect();
    if title.is_empty() {
        "Untitled".to_string()
    } else {
        title
    }
}

fn clean_file_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .filter(|character| {
            !character.is_control()
                && !matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        })
        .collect();
    let cleaned = cleaned
        .trim()
        .trim_matches('.')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = truncate_utf8_bytes(&cleaned, FILE_STEM_MAX_BYTES).trim();
    if cleaned.is_empty() {
        "Untitled".to_string()
    } else {
        cleaned.to_string()
    }
}

fn local_destination(destination: &str, note_path: &Path, root: &Path) -> Option<String> {
    let base = url::Url::from_file_path(note_path).ok()?;
    let joined = base.join(destination).ok()?;
    if joined.scheme() != "file" {
        return None;
    }
    let fragment = joined.fragment().map(str::to_string);
    let path = joined.to_file_path().ok()?.canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    if !path.starts_with(root) || !path.is_file() {
        return None;
    }
    let mut safe = url::Url::from_file_path(path).ok()?;
    safe.set_fragment(fragment.as_deref());
    Some(safe.to_string())
}

fn safe_destination(destination: &str, image: bool, note_path: &Path, root: &Path) -> String {
    let trimmed = destination.trim();
    if trimmed.starts_with('#') {
        return trimmed.to_string();
    }
    if let Ok(parsed) = url::Url::parse(trimmed) {
        return match parsed.scheme() {
            "http" | "https" if !image => trimmed.to_string(),
            "mailto" if !image => trimmed.to_string(),
            "file" => local_destination(trimmed, note_path, root).unwrap_or_default(),
            _ => String::new(),
        };
    }
    local_destination(trimmed, note_path, root).unwrap_or_default()
}

pub fn render_markdown(source: &str, note_path: &Path, root: &Path) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    let parser = Parser::new_ext(source, options).filter_map(|event| match event {
        Event::Html(_) | Event::InlineHtml(_) => None,
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Some(Event::Start(Tag::Link {
            link_type,
            dest_url: safe_destination(&dest_url, false, note_path, root).into(),
            title,
            id,
        })),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Some(Event::Start(Tag::Image {
            link_type,
            dest_url: safe_destination(&dest_url, true, note_path, root).into(),
            title,
            id,
        })),
        event => Some(event),
    });
    let mut output = String::new();
    html::push_html(&mut output, parser);
    output
}

fn markdown_word_count(source: &str) -> u32 {
    Parser::new_ext(source, Options::empty())
        .filter_map(|event| match event {
            Event::Text(text) | Event::Code(text) => Some(text.split_whitespace().count() as u32),
            _ => None,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_searches_and_sorts_notes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir(root.join("projects")).unwrap();
        std::fs::write(root.join("alpha.md"), "# Alpha\n\nFirst idea").unwrap();
        std::fs::write(
            root.join("projects/beta.markdown"),
            "# Beta\n\nKnowledge graph",
        )
        .unwrap();
        std::fs::write(root.join("ignored.txt"), "Knowledge graph").unwrap();

        let all = list_notes(root, "").unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|note| note.title == "Alpha"));
        assert!(
            all.iter()
                .any(|note| note.relative_path == "projects/beta.markdown")
        );

        let filtered = list_notes(root, "graph").unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Beta");
    }

    #[test]
    fn index_queries_are_paginated() {
        let temp = tempfile::tempdir().unwrap();
        create_note(temp.path(), "One").unwrap();
        create_note(temp.path(), "Two").unwrap();
        create_note(temp.path(), "Three").unwrap();
        let index = build_index(temp.path(), &[]).unwrap();
        let (first, total, has_more) = query_index(&index, "", 0, 2);
        assert_eq!(first.len(), 2);
        assert_eq!(total, 3);
        assert!(has_more);
        let (last, _, has_more) = query_index(&index, "", 2, 2);
        assert_eq!(last.len(), 1);
        assert!(!has_more);
    }

    #[test]
    fn creates_human_readable_unique_notes() {
        let temp = tempfile::tempdir().unwrap();
        let first = create_note(temp.path(), " Project / idea ").unwrap();
        let second = create_note(temp.path(), "Project / idea").unwrap();
        assert_eq!(first.summary.relative_path, "Project idea.md");
        assert_eq!(second.summary.relative_path, "Project idea 2.md");
        assert_eq!(first.content, "# Project / idea\n\n");
    }

    #[test]
    fn writes_and_renders_existing_note() {
        let temp = tempfile::tempdir().unwrap();
        let created = create_note(temp.path(), "Editable").unwrap();
        let updated = write_note(
            temp.path(),
            Path::new(&created.summary.path),
            "# Editable\n\nChanged **now**.\n",
        )
        .unwrap();
        assert_eq!(updated.content, "# Editable\n\nChanged **now**.\n");
        let response = read_response(&updated, temp.path(), 7);
        assert_eq!(response.request_id, 7);
        assert_eq!(response.source, updated.content);
        assert!(response.html.contains("<strong>now</strong>"));
    }

    #[cfg(unix)]
    #[test]
    fn vault_and_notes_are_private() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        let document = create_note(&root, "Private").unwrap();
        assert_eq!(
            std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(document.summary.path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn ensuring_an_existing_private_vault_does_not_mutate_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        ensure_vault(&root).unwrap();
        let before = std::fs::metadata(&root).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        ensure_vault(&root).unwrap();

        let after = std::fs::metadata(&root).unwrap();
        assert_eq!(
            (before.ctime(), before.ctime_nsec()),
            (after.ctime(), after.ctime_nsec())
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_vault_uses_stable_canonical_paths() {
        let temp = tempfile::tempdir().unwrap();
        let actual = temp.path().join("actual");
        let linked = temp.path().join("linked");
        std::fs::create_dir(&actual).unwrap();
        std::os::unix::fs::symlink(&actual, &linked).unwrap();
        let created = create_note(&linked, "Stable").unwrap();
        let listed = list_notes(&linked, "").unwrap();
        assert_eq!(listed[0].path, created.summary.path);
        assert_eq!(
            read_note(&linked, Path::new(&listed[0].path))
                .unwrap()
                .summary
                .path,
            created.summary.path
        );
    }

    #[test]
    fn rejects_paths_outside_vault() {
        let vault = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        assert!(read_note(vault.path(), outside.path()).is_err());
    }

    #[test]
    fn markdown_preview_drops_unsafe_content_and_remote_images() {
        let temp = tempfile::tempdir().unwrap();
        let note = temp.path().join("note.md");
        std::fs::write(&note, "x").unwrap();
        let rendered = render_markdown(
            "<script>alert(1)</script>\n\n[x](javascript:alert(1)) [ok](https://vmux.ai) ![track](https://tracker.invalid/a.png)",
            &note,
            temp.path(),
        );
        assert!(!rendered.contains("script"));
        assert!(!rendered.contains("javascript:"));
        assert!(!rendered.contains("tracker.invalid"));
        assert!(rendered.contains("https://vmux.ai"));
    }

    #[test]
    fn relative_destinations_resolve_inside_vault() {
        let temp = tempfile::tempdir().unwrap();
        let note = temp.path().join("note.md");
        let linked = temp.path().join("linked.md");
        std::fs::write(&note, "x").unwrap();
        std::fs::write(&linked, "y").unwrap();
        let rendered = render_markdown("[linked](linked.md)", &note, temp.path());
        assert!(rendered.contains("file:///"));
        assert!(rendered.contains("linked.md"));
    }

    #[test]
    fn preview_body_omits_frontmatter_and_title() {
        assert_eq!(note_body("---\ntags: [x]\n---\n# Title\n\nBody"), "Body");
        assert_eq!(note_body("Body"), "Body");
    }

    #[test]
    fn thematic_break_does_not_hide_excerpt_tail() {
        assert_eq!(note_excerpt("First\n\n---\n\nSecond"), "First --- Second");
    }

    #[test]
    fn filename_stem_has_a_byte_limit() {
        let title = "界".repeat(100);
        assert!(clean_file_stem(&title).len() <= FILE_STEM_MAX_BYTES);
    }

    #[test]
    fn word_count_uses_displayed_markdown_text() {
        assert_eq!(markdown_word_count("**two words** `three`"), 3);
    }
}
