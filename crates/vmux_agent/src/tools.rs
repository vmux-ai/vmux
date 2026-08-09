use crate::stream::ToolDef;

pub fn mcp_tool_defs() -> Vec<ToolDef> {
    vmux_mcp::tools::tool_definitions()
        .into_iter()
        .map(|d| ToolDef {
            name: d.name,
            description: d.description,
            input_schema: d.input_schema,
            read_only: false,
        })
        .collect()
}

#[cfg(test)]
#[path = "tools.test.rs"]
mod tests;
