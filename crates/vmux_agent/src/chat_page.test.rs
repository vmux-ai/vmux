use super::*;

#[test]
fn approval_details_parse_nested_json() {
    assert_eq!(
        approval_details(
            r#"{"arguments":{"path":"/tmp/SKILL.md"},"server":"vmux","tool":"read_file"}"#
        ),
        vec![
            ApprovalDetail {
                label: "Path".into(),
                value: "/tmp/SKILL.md".into(),
            },
            ApprovalDetail {
                label: "Server".into(),
                value: "vmux".into(),
            },
            ApprovalDetail {
                label: "Tool".into(),
                value: "read_file".into(),
            },
        ]
    );
    assert!(approval_details("{}").is_empty());
}
