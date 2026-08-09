use super::*;

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

    let with_link = edit_markdown_property(
        &renamed,
        &crate::event::FilePropertyEdit {
            original_key: String::new(),
            key: "related".into(),
            kind: KnowledgePropertyKind::Link,
            values: vec!["Roadmap".into()],
            remove: false,
        },
    )
    .unwrap();
    let with_date = edit_markdown_property(
        &with_link,
        &crate::event::FilePropertyEdit {
            original_key: String::new(),
            key: "due".into(),
            kind: KnowledgePropertyKind::Date,
            values: vec!["2026-07-25".into()],
            remove: false,
        },
    )
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
        std::fs::read_to_string(destination.join("claude/projects/project-a/MEMORY.md")).unwrap(),
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
