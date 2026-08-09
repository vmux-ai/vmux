use super::*;

#[test]
fn inline_media_query_requires_a_token_boundary_and_open_tail() {
    assert_eq!(
        inline_media_query("inspect @Pictures/scr"),
        Some(InlineMediaQuery {
            start: 8,
            query: "Pictures/scr",
        })
    );
    assert_eq!(
        inline_media_query("@"),
        Some(InlineMediaQuery {
            start: 0,
            query: "",
        })
    );
    assert_eq!(inline_media_query("mail@example.com"), None);
    assert_eq!(inline_media_query("inspect @image.png next"), None);
}

#[test]
fn inline_media_replacement_preserves_prompt_prefix() {
    let draft = "inspect @Pictures/scr";
    let query = inline_media_query(draft).unwrap();
    assert_eq!(
        replace_inline_media_query(draft, query, "@Pictures/photo.png "),
        "inspect @Pictures/photo.png "
    );
    assert_eq!(replace_inline_media_query(draft, query, ""), "inspect ");
}

#[test]
fn media_display_path_includes_entry_name() {
    let entry = ChatMediaEntry {
        name: "Accessibility".into(),
        parent: "~/Library".into(),
        ..Default::default()
    };
    assert_eq!(entry.display_path(), "~/Library/Accessibility");

    let root_entry = ChatMediaEntry {
        name: "Pictures".into(),
        parent: "~".into(),
        ..Default::default()
    };
    assert_eq!(root_entry.display_path(), "~/Pictures");
}

#[test]
fn attachment_batches_append_new_files_and_refresh_existing_metadata() {
    let first = ChatAttachment {
        path: "/tmp/one.png".into(),
        name: "one.png".into(),
        mime_type: "image/png".into(),
        size: 1,
        preview_data_url: "data:image/png;base64,preview".into(),
    };
    let second = ChatAttachment {
        path: "/tmp/two.png".into(),
        name: "two.png".into(),
        mime_type: "image/png".into(),
        size: 2,
        preview_data_url: String::new(),
    };
    let refreshed = ChatAttachment {
        size: 3,
        preview_data_url: String::new(),
        ..first.clone()
    };

    let merged = merge_chat_attachments(std::slice::from_ref(&first), &[second.clone(), refreshed]);

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].size, 3);
    assert_eq!(merged[0].preview_data_url, first.preview_data_url);
    assert_eq!(merged[1], second);
}
