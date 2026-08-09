use super::*;

#[test]
fn standard_keymap_serializes_with_legacy_vscode_compatibility() {
    assert_eq!(
        serde_json::to_value(KeymapKind::Vscode).unwrap(),
        serde_json::json!("standard")
    );
    assert_eq!(
        serde_json::from_value::<KeymapKind>(serde_json::json!("vscode")).unwrap(),
        KeymapKind::Vscode
    );
}
