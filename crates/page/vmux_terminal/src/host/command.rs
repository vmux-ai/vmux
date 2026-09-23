#[derive(vmux_macro::CommandBar)]
#[mcp(agent)]
pub(super) struct CloseRequest;

#[derive(vmux_macro::CommandBar)]
#[mcp]
pub(super) struct NextRequest;

#[derive(vmux_macro::CommandBar)]
#[mcp]
pub(super) struct PrevRequest;

#[derive(vmux_macro::CommandBar)]
#[mcp(agent)]
pub(super) struct ClearRequest;

#[derive(bevy::prelude::Message)]
pub(super) struct CopyModeRequest;

impl CopyModeRequest {
    pub fn register(app: &mut bevy::prelude::App) {
        vmux_command::CommandDefinition::register(app, Self::definitions, Self::from_invocation);
    }

    pub fn definitions() -> Vec<vmux_command::CommandDefinition> {
        vec![
            vmux_command::CommandDefinition::new("terminal_copy_mode", "Visual Mode", "Terminal")
                .hidden()
                .chord("Ctrl+b, [")
                .mcp(
                    vmux_command::CommandMcp::new(
                        "Visual Mode",
                        vmux_command::InputSchema::object(),
                    )
                    .allow_agent(),
                ),
        ]
    }

    pub fn from_invocation(invocation: &vmux_command::CommandInvocation) -> Option<Self> {
        (invocation.id == "terminal_copy_mode").then_some(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_mcp_definitions_are_the_dispatchable_command_set() {
        let mut definitions = Vec::new();
        definitions.extend(CloseRequest::definitions());
        definitions.extend(NextRequest::definitions());
        definitions.extend(PrevRequest::definitions());
        definitions.extend(ClearRequest::definitions());
        definitions.extend(CopyModeRequest::definitions());
        let tools = definitions
            .iter()
            .filter_map(vmux_command::CommandDefinition::agent_tool)
            .collect::<Vec<_>>();

        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            [
                "terminal_close",
                "terminal_next",
                "terminal_prev",
                "terminal_clear",
                "terminal_copy_mode",
            ],
        );
        for tool in tools {
            let invocation =
                vmux_command::CommandInvocation::new(bevy::prelude::Entity::PLACEHOLDER, tool.name);
            let dispatches = CloseRequest::from_invocation(&invocation).is_some()
                || NextRequest::from_invocation(&invocation).is_some()
                || PrevRequest::from_invocation(&invocation).is_some()
                || ClearRequest::from_invocation(&invocation).is_some()
                || CopyModeRequest::from_invocation(&invocation).is_some();
            assert!(dispatches);
        }
    }
}
