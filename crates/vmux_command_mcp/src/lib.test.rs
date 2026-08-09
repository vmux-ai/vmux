use super::*;

#[test]
fn every_entry_dispatches() {
    for (id, _, schema) in tool_entries() {
        let has_required_arguments = schema
            .get("required")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|required| !required.is_empty());
        assert!(
            accepts_id(id) || !has_required_arguments && accepts_call(id, serde_json::json!({})),
            "{id}"
        );
    }
}

#[test]
fn schema_matches_desktop_commands() {
    assert_eq!(tool_entries(), vmux_command::AppCommand::mcp_tool_entries());
}
