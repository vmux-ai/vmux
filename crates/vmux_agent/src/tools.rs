use crate::stream::ToolDef;

pub fn mcp_tool_defs() -> Vec<ToolDef> {
    vmux_mcp::tools::tool_definitions()
        .into_iter()
        .map(|d| ToolDef {
            name: Box::leak(d.name.into_boxed_str()),
            description: Box::leak(d.description.into_boxed_str()),
            input_schema: d.input_schema,
            read_only: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
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
}
