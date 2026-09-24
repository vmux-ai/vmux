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

impl vmux_command::CommandRequest for CopyModeRequest {
    fn definitions() -> Vec<vmux_command::CommandDefinition> {
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
}

impl TryFrom<&vmux_command::CommandInvocation> for CopyModeRequest {
    type Error = ();

    fn try_from(invocation: &vmux_command::CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "terminal_copy_mode")
            .then_some(Self)
            .ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_command::CommandRequest;

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
            let dispatches = CloseRequest::try_from(&invocation).is_ok()
                || NextRequest::try_from(&invocation).is_ok()
                || PrevRequest::try_from(&invocation).is_ok()
                || ClearRequest::try_from(&invocation).is_ok()
                || CopyModeRequest::try_from(&invocation).is_ok();
            assert!(dispatches);
        }
    }
}
