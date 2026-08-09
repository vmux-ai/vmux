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
