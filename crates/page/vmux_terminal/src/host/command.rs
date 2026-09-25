#[derive(vmux_macro::CommandBar)]
#[menu(label = "Close Terminal")]
#[mcp(agent)]
pub(super) struct TerminalCloseRequest;

#[derive(vmux_macro::CommandBar)]
#[menu(label = "Next Terminal")]
#[mcp]
pub(super) struct TerminalNextRequest;

#[derive(vmux_macro::CommandBar)]
#[menu(label = "Previous Terminal")]
#[mcp]
pub(super) struct TerminalPrevRequest;

#[derive(vmux_macro::CommandBar)]
#[menu(label = "Clear Terminal")]
#[mcp(agent)]
pub(super) struct TerminalClearRequest;

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
        definitions.extend(TerminalCloseRequest::definitions());
        definitions.extend(TerminalNextRequest::definitions());
        definitions.extend(TerminalPrevRequest::definitions());
        definitions.extend(TerminalClearRequest::definitions());
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
            let dispatches = TerminalCloseRequest::try_from(&invocation).is_ok()
                || TerminalNextRequest::try_from(&invocation).is_ok()
                || TerminalPrevRequest::try_from(&invocation).is_ok()
                || TerminalClearRequest::try_from(&invocation).is_ok()
                || CopyModeRequest::try_from(&invocation).is_ok();
            assert!(dispatches);
        }
    }
}
