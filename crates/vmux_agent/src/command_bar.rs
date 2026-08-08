//! What this crate offers the command bar, and what it does when one of those rows is chosen.
//!
//! The command bar is a launcher: it lists things and reports which was picked. It has no idea
//! what an agent is, so agents are described here as [`ContributedCommand`]s and claimed back here
//! when chosen. Keeping both halves in one file is the point — the id format is a private contract
//! between them, and splitting it would let the two drift.

use bevy::prelude::*;
use vmux_command::snapshot::{
    CommandBarAgentsSnapshot, CommandBarContributions, ContributedCommand,
};
use vmux_core::agent::{PageAgentAttachRequest, PageAgentSpawnStackRequest};

/// Publish the agents to launch, and a row per model.
pub(crate) fn publish_contributions(
    agents: Res<CommandBarAgentsSnapshot>,
    mut contributions: ResMut<CommandBarContributions>,
) {
    if !agents.is_changed() {
        return;
    }
    contributions.pages = agents.launcher_pages();
    contributions.commands = agents
        .strategies
        .iter()
        .map(|strategy| ContributedCommand {
            id: app_agent_id(&strategy.provider, &strategy.model),
            message_id: "command-new-app-chat".to_string(),
            args: vec![
                ("provider".to_string(), strategy.provider.clone()),
                ("model".to_string(), strategy.model.clone()),
            ],
        })
        .collect();
}

/// Start the chat a chosen row named, in the stack the command bar offered or a new one.
pub(crate) fn claim_chosen_command(
    mut reader: MessageReader<vmux_layout::ContributedCommandChosen>,
    mut attach: MessageWriter<PageAgentAttachRequest>,
    mut spawn: MessageWriter<PageAgentSpawnStackRequest>,
) {
    for chosen in reader.read() {
        let Some((provider, model)) = parse_app_agent_id(&chosen.id) else {
            continue;
        };
        let sid = uuid::Uuid::new_v4().to_string();
        if let Some(stack) = chosen.stack {
            attach.write(PageAgentAttachRequest {
                stack,
                provider,
                model,
                sid,
            });
        } else if let Some(pane) = chosen.pane {
            spawn.write(PageAgentSpawnStackRequest {
                pane,
                provider,
                model,
                sid,
            });
        }
    }
}

/// Command-bar row id for starting a chat with one provider and model.
fn app_agent_id(provider: &str, model: &str) -> String {
    format!("app_{provider}_{model}_new")
}

/// The provider and model an [`app_agent_id`] names, or `None` when the row is someone else's.
fn parse_app_agent_id(id: &str) -> Option<(String, String)> {
    let body = id.strip_prefix("app_")?.strip_suffix("_new")?;
    let (provider, model) = body.split_once('_')?;
    Some((provider.to_string(), model.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The id is a private round trip between the two halves of this file. A row whose id does not
    /// survive it is published and then silently ignored when the user picks it.
    #[test]
    fn a_published_row_id_parses_back_to_what_named_it() {
        let id = app_agent_id("anthropic", "claude-opus-4");
        assert_eq!(
            parse_app_agent_id(&id),
            Some(("anthropic".to_string(), "claude-opus-4".to_string())),
            "model names contain the separator, so only the first underscore may split"
        );
    }

    /// Rows contributed by other crates land in the same reader; claiming them would start an
    /// agent for something entirely unrelated.
    #[test]
    fn another_crates_row_is_left_alone() {
        assert_eq!(parse_app_agent_id("browser_open_history"), None);
        assert_eq!(parse_app_agent_id("app_new"), None);
        assert_eq!(parse_app_agent_id("app_onlyprovider_new"), None);
    }
}
