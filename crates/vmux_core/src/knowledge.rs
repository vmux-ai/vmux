pub const KNOWLEDGE_TREE_EVENT: &str = "knowledge-tree";
pub const KNOWLEDGE_SEARCH_EVENT: &str = "knowledge-search";
pub const KNOWLEDGE_CREATE_RESULT_EVENT: &str = "knowledge-create-result";

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
    pub git_status: KnowledgeGitStatus,
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
pub enum KnowledgeGitStatus {
    #[default]
    Clean,
    Added,
    Modified,
    Deleted,
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
pub struct KnowledgeCreateRequest {
    pub parent: String,
    pub name: String,
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
pub struct KnowledgeCreateResult {
    pub ok: bool,
    pub path: String,
    pub error: String,
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
        let key_index = index;
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
            title_line = Some(key_index as u32 + 1);
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

#[cfg(host)]
mod agent_config;
#[cfg(host)]
mod index;
#[cfg(host)]
mod store;

#[cfg(host)]
pub use agent_config::sync_external_agent_configs;
#[cfg(host)]
pub use index::{KnowledgeIndex, KnowledgeRenamePlan, KnowledgeResolvedLink, KnowledgeSearchHit};
#[cfg(host)]
pub use store::{AgentPrompt, Frontmatter, KnowledgeVault, MemoriesDir, SkillsDir};

#[cfg(test)]
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
    fn inline_list_title_keeps_the_title_source_line() {
        let metadata = markdown_metadata("---\nstatus: draft\ntitle: [Primary, Alias]\n---\n");
        assert_eq!(metadata.title, "Primary");
        assert_eq!(metadata.title_line, Some(2));
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
}
