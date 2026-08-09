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
use vmux_core::agent::{
    PageAgentAttachDefaultRequest, PageAgentAttachRequest, PageAgentSpawnDefaultRequest,
    PageAgentSpawnStackRequest,
};

/// Urls that name "whichever agent is default" rather than a page that exists.
///
/// Kept from before agent urls carried an id. The command bar cannot open these — there is nothing
/// at them until this crate picks one — so it hands them back instead.
const DEFAULT_AGENT_URLS: [&str; 2] = ["vmux://agent/", "vmux://agent"];

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
    contributions.claimed_urls = DEFAULT_AGENT_URLS.map(str::to_string).to_vec();
}

/// Act on a row or url the command bar handed back.
pub(crate) fn claim_chosen_command(
    mut reader: MessageReader<vmux_layout::ContributedCommandChosen>,
    mut attach: MessageWriter<PageAgentAttachRequest>,
    mut spawn: MessageWriter<PageAgentSpawnStackRequest>,
    mut attach_default: MessageWriter<PageAgentAttachDefaultRequest>,
    mut spawn_default: MessageWriter<PageAgentSpawnDefaultRequest>,
) {
    for chosen in reader.read() {
        if DEFAULT_AGENT_URLS.contains(&chosen.id.as_str()) {
            if let Some(stack) = chosen.stack {
                attach_default.write(PageAgentAttachDefaultRequest { stack });
            } else if let Some(pane) = chosen.pane {
                spawn_default.write(PageAgentSpawnDefaultRequest { pane });
            }
            continue;
        }
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
#[path = "command_bar.test.rs"]
mod tests;
