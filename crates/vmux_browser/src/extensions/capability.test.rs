use super::*;

#[test]
fn embedded_matrix_has_unique_entries_for_chromium_148() {
    let matrix = CapabilityMatrix::embedded().unwrap();
    assert_eq!(matrix.chromium_major, 148);
    assert_eq!(
        matrix
            .lookup("macos", "tabs", "query", CapabilityKind::Method)
            .unwrap()
            .status,
        CapabilityStatus::Untested
    );
    matrix.validate().unwrap();
}

#[test]
fn advertised_entries_require_scenarios() {
    let matrix = CapabilityMatrix {
        chromium_major: 148,
        entries: vec![CapabilityEntry {
            platform: "macos".into(),
            namespace: "runtime".into(),
            member: "sendMessage".into(),
            kind: CapabilityKind::Method,
            status: CapabilityStatus::Native,
            owner: Some("cef".into()),
            scenario: None,
        }],
    };
    assert_eq!(
        matrix.validate().unwrap_err(),
        "runtime.sendMessage on macos is Native without a scenario"
    );
}
