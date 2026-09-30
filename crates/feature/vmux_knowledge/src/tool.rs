use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::AgentRequest;
use vmux_core::ProcessAnchor;
use vmux_core::host::manifest::FeatureManifestPlugin;
use vmux_tool::{ToolAppExt, ToolCommand, ToolDispatchSet};

use crate::host::{AgentReadKnowledge, AgentSearchKnowledge, AgentWriteKnowledge};

pub struct KnowledgeToolPlugin;

impl Plugin for KnowledgeToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeatureManifestPlugin::new(include_str!("feature.ron")))
            .register_tool::<SearchKnowledgeArgs>()
            .register_tool::<ReadKnowledgeArgs>()
            .register_tool::<WriteKnowledgeArgs>()
            .add_systems(Update, (search, read, write).in_set(ToolDispatchSet));
    }
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchKnowledgeArgs {
    query: String,
    limit: Option<u64>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadKnowledgeArgs {
    path: String,
    line: Option<u64>,
    limit: Option<u64>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteKnowledgeArgs {
    path: Option<String>,
    title: String,
    content: String,
}

fn search(
    mut commands: Commands,
    requests: Query<
        (Entity, &Name, Option<&ProcessAnchor>, &SearchKnowledgeArgs),
        Added<SearchKnowledgeArgs>,
    >,
) {
    for (entity, name, anchor, args) in &requests {
        let command = anchor
            .map(|anchor| anchor.0)
            .ok_or_else(|| {
                format!(
                    "{} requires an agent anchor (not available to this client)",
                    name.as_str()
                )
            })
            .and_then(|anchor| {
                let query = Text::required(args.query.clone(), "search_knowledge.query is empty")?;
                let limit = args.limit.unwrap_or(20);
                if !(1..=100).contains(&limit) {
                    return Err("search_knowledge.limit must be between 1 and 100".to_string());
                }
                AgentRequest::encode(&AgentSearchKnowledge {
                    anchor,
                    query,
                    limit: limit as u16,
                })
            });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn read(
    mut commands: Commands,
    requests: Query<
        (Entity, &Name, Option<&ProcessAnchor>, &ReadKnowledgeArgs),
        Added<ReadKnowledgeArgs>,
    >,
) {
    for (entity, name, anchor, args) in &requests {
        let command = anchor
            .map(|anchor| anchor.0)
            .ok_or_else(|| {
                format!(
                    "{} requires an agent anchor (not available to this client)",
                    name.as_str()
                )
            })
            .and_then(|anchor| {
                let path = Text::required(args.path.clone(), "read_knowledge.path is empty")?;
                let line = args.line.unwrap_or(1);
                let limit = args.limit.unwrap_or(200);
                if line == 0 || line > u32::MAX as u64 {
                    return Err("read_knowledge.line must be at least 1".to_string());
                }
                if !(1..=2_000).contains(&limit) {
                    return Err("read_knowledge.limit must be between 1 and 2000".to_string());
                }
                AgentRequest::encode(&AgentReadKnowledge {
                    anchor,
                    path,
                    line: line as u32,
                    limit: limit as u32,
                })
            });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn write(
    mut commands: Commands,
    requests: Query<
        (Entity, &Name, Option<&ProcessAnchor>, &WriteKnowledgeArgs),
        Added<WriteKnowledgeArgs>,
    >,
) {
    for (entity, name, anchor, args) in &requests {
        let command = anchor
            .map(|anchor| anchor.0)
            .ok_or_else(|| {
                format!(
                    "{} requires an agent anchor (not available to this client)",
                    name.as_str()
                )
            })
            .and_then(|anchor| {
                let path = args.path.clone().and_then(Text::trimmed);
                let title = Text::required(args.title.clone(), "write_knowledge.title is empty")?;
                let content =
                    Text::required(args.content.clone(), "write_knowledge.content is empty")?;
                AgentRequest::encode(&AgentWriteKnowledge {
                    anchor,
                    path,
                    title,
                    content,
                })
            });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

struct Text;

impl Text {
    fn trimmed(value: String) -> Option<String> {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    }

    fn required(value: String, error: &str) -> Result<String, String> {
        Self::trimmed(value).ok_or_else(|| error.to_string())
    }
}
