use vmux_command::CommandInvocation;

#[derive(bevy::prelude::Message)]
pub(super) struct TerminalCloseRequest;

impl TryFrom<&CommandInvocation> for TerminalCloseRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "terminal_close")
            .then_some(Self)
            .ok_or(())
    }
}

#[derive(bevy::prelude::Message)]
pub(super) struct TerminalNextRequest;

impl TryFrom<&CommandInvocation> for TerminalNextRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "terminal_next").then_some(Self).ok_or(())
    }
}

#[derive(bevy::prelude::Message)]
pub(super) struct TerminalPrevRequest;

impl TryFrom<&CommandInvocation> for TerminalPrevRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "terminal_prev").then_some(Self).ok_or(())
    }
}

#[derive(bevy::prelude::Message)]
pub(super) struct TerminalClearRequest;

impl TryFrom<&CommandInvocation> for TerminalClearRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "terminal_clear")
            .then_some(Self)
            .ok_or(())
    }
}

#[derive(bevy::prelude::Message)]
pub(super) struct CopyModeRequest;

impl TryFrom<&CommandInvocation> for CopyModeRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "terminal_copy_mode")
            .then_some(Self)
            .ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_mcp_definitions_are_the_dispatchable_command_set() {
        let definitions = vmux_command::CommandManifest::for_feature::<crate::Feature>().into_vec();
        let tools = definitions
            .iter()
            .filter_map(vmux_command::CommandDefinition::agent_tool)
            .filter(|tool| {
                let invocation = vmux_command::CommandInvocation::new(
                    bevy::prelude::Entity::PLACEHOLDER,
                    &tool.name,
                );
                TerminalCloseRequest::try_from(&invocation).is_ok()
                    || TerminalNextRequest::try_from(&invocation).is_ok()
                    || TerminalPrevRequest::try_from(&invocation).is_ok()
                    || TerminalClearRequest::try_from(&invocation).is_ok()
                    || CopyModeRequest::try_from(&invocation).is_ok()
            })
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
