use super::*;

#[test]
fn markdown_paths() {
    assert!(is_markdown_path(Path::new("a.md")));
    assert!(is_markdown_path(Path::new("A.MARKDOWN")));
    assert!(is_markdown_path(Path::new("a.mdx")));
    assert!(!is_markdown_path(Path::new("a.rs")));
}

#[test]
fn note_blocks_have_source_ranges() {
    let blocks = parse_note("# Title\n\nParagraph\n\n- one\n- two\n");
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].start_line, 0);
    assert_eq!(blocks[1].start_line, 2);
    assert_eq!(blocks[2].start_line, 4);
}

#[test]
fn note_vertical_navigation_skips_block_separators() {
    let blocks = parse_note("first\nline\n\nsecond\nline\n");
    assert_eq!(note_vertical_target(&blocks, 1, 1), Some(3));
    assert_eq!(note_vertical_target(&blocks, 3, -1), Some(1));
    assert_eq!(note_vertical_target(&blocks, 0, 1), None);
    assert_eq!(note_vertical_target(&blocks, 4, -1), None);
    assert_eq!(note_vertical_target(&blocks, 0, -1), Some(0));
    assert_eq!(note_vertical_target(&blocks, 4, 1), Some(4));
}

#[test]
fn note_vertical_navigation_keeps_blank_code_lines() {
    let blocks = parse_note("```text\none\n\ntwo\n```\n\nafter\n");
    assert_eq!(note_vertical_target(&blocks, 1, 1), None);
    assert_eq!(note_vertical_target(&blocks, 2, 1), None);
    assert_eq!(note_vertical_target(&blocks, 4, 1), Some(6));
}

#[test]
fn note_vertical_navigation_leaves_a_list_for_the_following_paragraph() {
    let blocks = parse_note("- `vmux/` — references.\n\nThis structure is a starting point.\n");
    assert_eq!(blocks[0].start_line, 0);
    assert_eq!(blocks[0].end_line, 2);
    assert_eq!(blocks[1].start_line, 2);
    assert_eq!(note_vertical_target(&blocks, 0, 1), Some(2));
    assert_eq!(note_vertical_target(&blocks, 2, -1), Some(0));
}

#[test]
fn nested_list_items_keep_exact_source_lines() {
    let blocks = parse_note("1. one\n   - nested\n2. two\n");
    let MdBlock::List { items, .. } = &blocks[0].block else {
        panic!("expected list");
    };
    assert_eq!(items[0].source_line, 0);
    assert_eq!(items[1].source_line, 2);
    let MdBlock::List { items, .. } = &items[0].blocks[1] else {
        panic!("expected nested list");
    };
    assert_eq!(items[0].source_line, 1);
}

#[test]
fn frontmatter_title_renders_as_heading_without_metadata_delimiters() {
    let note = parse_note_document("---\ntitle: Welcome Home\n---\n\nBody\n");
    assert_eq!(note.title, "Welcome Home");
    assert_eq!(note.blocks.len(), 2);
    assert_eq!(note.blocks[0].start_line, 1);
    assert_eq!(note.blocks[0].source, "title: Welcome Home");
    assert!(matches!(
        &note.blocks[0].block,
        MdBlock::Heading { level: 1, .. }
    ));
    assert_eq!(note.blocks[1].start_line, 4);
}

#[test]
fn frontmatter_title_does_not_duplicate_matching_heading() {
    let note = parse_note_document("---\ntitle: Welcome Home\n---\n\n# Welcome Home\n\nBody\n");

    assert_eq!(note.title, "Welcome Home");
    assert_eq!(note.blocks.len(), 2);
    assert_eq!(note.blocks[0].start_line, 4);
    assert!(matches!(
        &note.blocks[0].block,
        MdBlock::Heading { level: 1, .. }
    ));
}
