use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::AgentRequest;
use vmux_ecs::ProcessAnchor;
use vmux_ecs::manifest::FeaturePlugin;
use vmux_tool::{ToolCommand, ToolDispatchSet};

use crate::host::{AgentReadKnowledge, AgentSearchKnowledge, AgentWriteKnowledge};

pub struct KnowledgeToolPlugin;

impl Plugin for KnowledgeToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_systems(Update, (search, read, write).in_set(ToolDispatchSet));
    }
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchKnowledge {
    query: String,
    limit: Option<u64>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadKnowledge {
    path: String,
    line: Option<u64>,
    limit: Option<u64>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteKnowledge {
    path: Option<String>,
    title: String,
    content: String,
}

fn search(
    mut commands: Commands,
    requests: Query<
        (Entity, &Name, Option<&ProcessAnchor>, &SearchKnowledge),
        Added<SearchKnowledge>,
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
                let query = args.query.trim();
                if query.is_empty() {
                    return Err("search_knowledge.query is empty".to_string());
                }
                let limit = args.limit.unwrap_or(20);
                if !(1..=100).contains(&limit) {
                    return Err("search_knowledge.limit must be between 1 and 100".to_string());
                }
                AgentRequest::encode(&AgentSearchKnowledge {
                    anchor,
                    query: query.to_string(),
                    limit: limit as u16,
                })
            });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn read(
    mut commands: Commands,
    requests: Query<(Entity, &Name, Option<&ProcessAnchor>, &ReadKnowledge), Added<ReadKnowledge>>,
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
                let path = args.path.trim();
                if path.is_empty() {
                    return Err("read_knowledge.path is empty".to_string());
                }
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
                    path: path.to_string(),
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
        (Entity, &Name, Option<&ProcessAnchor>, &WriteKnowledge),
        Added<WriteKnowledge>,
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
                let path = args
                    .path
                    .as_deref()
                    .map(str::trim)
                    .filter(|path| !path.is_empty());
                let title = args.title.trim();
                if title.is_empty() {
                    return Err("write_knowledge.title is empty".to_string());
                }
                let content = args.content.trim();
                if content.is_empty() {
                    return Err("write_knowledge.content is empty".to_string());
                }
                AgentRequest::encode(&AgentWriteKnowledge {
                    anchor,
                    path: path.map(str::to_string),
                    title: title.to_string(),
                    content: content.to_string(),
                })
            });
        commands.entity(entity).insert(ToolCommand(command));
    }
}
