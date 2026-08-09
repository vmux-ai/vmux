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
    let text = "---\ntitle: \"Research, Notes\"\naliases:\n  - RN\n  - \"Reading Notes\"\n---\n";
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
