use super::*;

#[test]
fn secure_comparison_requires_exact_token() {
    assert!(secure_eq("abc", "abc"));
    assert!(!secure_eq("abc", "abd"));
    assert!(!secure_eq("abc", "ab"));
}

#[test]
fn client_operation_ids_are_bounded() {
    assert!(valid_client_op_id(&ClientOpId::new("mobile:1:1")));
    assert!(!valid_client_op_id(&ClientOpId::new("  ")));
    assert!(!valid_client_op_id(&ClientOpId::new(
        "x".repeat(MAX_CLIENT_OP_ID_BYTES + 1)
    )));
}

#[test]
fn remote_state_requires_enabled_marker() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("remote-state");
    assert!(!remote_enabled_at(&path));
    std::fs::write(&path, b"disabled\n").unwrap();
    assert!(!remote_enabled_at(&path));
    std::fs::write(&path, b"enabled\n").unwrap();
    assert!(remote_enabled_at(&path));
}

#[test]
fn media_query_paths_decode_percent_escapes() {
    assert_eq!(
        decode_media_query_path("Pictures/My%20Photo.png"),
        std::path::PathBuf::from("Pictures/My Photo.png")
    );
}

#[test]
fn remote_attachments_are_count_limited_before_file_access() {
    let attachments = (0..=MAX_ATTACHMENTS)
        .map(|index| AgentAttachment {
            path: format!("/missing/{index}"),
            name: format!("{index}.png"),
            mime_type: "image/png".into(),
            size: 1,
        })
        .collect();
    assert!(validate_remote_attachments(attachments).is_none());
}

#[test]
fn client_operation_deduplication_is_bounded_and_releasable() {
    let mut deduper = ClientOpDeduper::default();
    let first = ClientOpId::new("first");
    assert!(deduper.claim(first.clone()));
    assert!(!deduper.claim(first.clone()));
    deduper.release(&first);
    assert!(deduper.claim(first));

    for index in 0..=MAX_CLIENT_OP_IDS {
        assert!(deduper.claim(ClientOpId::new(format!("op-{index}"))));
    }
    assert_eq!(deduper.order.len(), MAX_CLIENT_OP_IDS);
    assert_eq!(deduper.seen.len(), MAX_CLIENT_OP_IDS);
}
