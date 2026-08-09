use super::*;

#[test]
fn returns_at_least_one_tool() {
    let defs = mcp_tool_defs();
    assert!(!defs.is_empty(), "vmux_mcp must expose at least one tool");
    for d in &defs {
        assert!(!d.name.is_empty(), "tool name must not be empty");
        assert!(
            d.input_schema.is_object(),
            "tool schema must be a JSON object"
        );
    }
}
