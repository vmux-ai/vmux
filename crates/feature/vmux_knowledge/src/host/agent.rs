use bevy::prelude::*;
use vmux_api::protocol::{AgentCommandResult, ProcessId};
use vmux_core::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
    AgentSession,
};
use vmux_core::knowledge::{KnowledgeIndex, KnowledgeVault};

#[vmux_api::agent]
pub(crate) struct AgentWriteKnowledge {
    pub anchor: ProcessId,
    pub path: Option<String>,
    pub title: String,
    pub content: String,
}

#[vmux_api::agent]
pub(crate) struct AgentSearchKnowledge {
    pub anchor: ProcessId,
    pub query: String,
    pub limit: u16,
}

#[vmux_api::agent]
pub(crate) struct AgentReadKnowledge {
    pub anchor: ProcessId,
    pub path: String,
    pub line: u32,
    pub limit: u32,
}

pub(super) struct KnowledgeAgentPlugin;

impl Plugin for KnowledgeAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_agent_request::<AgentSearchKnowledge>()
            .add_agent_request::<AgentReadKnowledge>()
            .add_agent_request::<AgentWriteKnowledge>()
            .add_systems(
                Update,
                (search_knowledge, read_knowledge, write_knowledge).after(AgentRequestRouteSet),
            );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct AgentPaneQuery<'w, 's> {
    agents: Query<'w, 's, (&'static ProcessId, &'static ChildOf), With<AgentSession>>,
    child_of: Query<'w, 's, &'static ChildOf>,
}

impl AgentPaneQuery<'_, '_> {
    fn find(&self, anchor: ProcessId) -> Option<Entity> {
        let (_, stack) = self.agents.iter().find(|(id, _)| **id == anchor)?;
        self.child_of.get(stack.parent()).ok().map(ChildOf::parent)
    }
}

fn search_knowledge(
    mut requests: MessageReader<AgentRequestMessage<AgentSearchKnowledge>>,
    agents: AgentPaneQuery,
    indexes: Query<&KnowledgeIndex>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    let index = indexes.single().ok();
    for request in requests.read() {
        let command = &request.payload;
        let result = if agents.find(command.anchor).is_none() {
            AgentCommandResult::Error("agent pane not found".to_string())
        } else {
            match index {
                Some(index) if index.loaded() => {
                    let matches = index.search(&command.query, usize::from(command.limit));
                    if matches.is_empty() {
                        AgentCommandResult::Text(format!(
                            "No Knowledge matches for: {}",
                            command.query.trim()
                        ))
                    } else {
                        let root = index.root();
                        let mut rows = Vec::new();
                        for item in matches {
                            let path = item
                                .path
                                .strip_prefix(root)
                                .unwrap_or(&item.path)
                                .to_string_lossy()
                                .replace('\\', "/");
                            rows.push(format!(
                                "{}:{}: {} — {}",
                                path,
                                item.line + 1,
                                item.title,
                                item.preview
                            ));
                        }
                        AgentCommandResult::Text(rows.join("\n"))
                    }
                }
                Some(_) => AgentCommandResult::Error(
                    "Knowledge index is still loading; retry shortly.".to_string(),
                ),
                None => AgentCommandResult::Error(
                    "Knowledge is unavailable in this vmux session.".to_string(),
                ),
            }
        };
        responses.write(request.reply.response(result));
    }
}

fn read_knowledge(
    mut requests: MessageReader<AgentRequestMessage<AgentReadKnowledge>>,
    agents: AgentPaneQuery,
    indexes: Query<&KnowledgeIndex>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    let index = indexes.single().ok();
    for request in requests.read() {
        let command = &request.payload;
        let result = if agents.find(command.anchor).is_none() {
            AgentCommandResult::Error("agent pane not found".to_string())
        } else {
            match index {
                Some(index) if index.loaded() => match index.note_by_query(&command.path) {
                    Some((note_path, title, text)) => {
                        let lines = text.lines().collect::<Vec<_>>();
                        let start = command.line.saturating_sub(1) as usize;
                        if start >= lines.len() && !lines.is_empty() {
                            AgentCommandResult::Error(format!(
                                "Knowledge line {} exceeds note length {}",
                                command.line,
                                lines.len()
                            ))
                        } else {
                            let end = start
                                .saturating_add(command.limit as usize)
                                .min(lines.len());
                            let source = note_path
                                .strip_prefix(index.root())
                                .unwrap_or(&note_path)
                                .to_string_lossy()
                                .replace('\\', "/");
                            let mut body = Vec::new();
                            for (offset, value) in lines[start..end].iter().enumerate() {
                                body.push(format!("{} | {}", start + offset + 1, value));
                            }
                            AgentCommandResult::Text(format!(
                                "Source: {source}\nTitle: {title}\nLines {}-{}\n\n{}",
                                start + 1,
                                end,
                                body.join("\n")
                            ))
                        }
                    }
                    None => AgentCommandResult::Error(format!(
                        "Knowledge note not found: {}",
                        command.path.trim()
                    )),
                },
                Some(_) => AgentCommandResult::Error(
                    "Knowledge index is still loading; retry shortly.".to_string(),
                ),
                None => AgentCommandResult::Error(
                    "Knowledge is unavailable in this vmux session.".to_string(),
                ),
            }
        };
        responses.write(request.reply.response(result));
    }
}

fn write_knowledge(
    mut requests: MessageReader<AgentRequestMessage<AgentWriteKnowledge>>,
    agents: AgentPaneQuery,
    mut open: MessageWriter<vmux_layout::OpenBesideRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let command = &request.payload;
        let result = match agents.find(command.anchor) {
            None => AgentCommandResult::Error("agent pane not found".to_string()),
            Some(pane) => match KnowledgeVault::user().write_note(
                command.path.as_deref(),
                &command.title,
                &command.content,
            ) {
                Ok(path) => {
                    open.write(vmux_layout::OpenBesideRequest {
                        pane,
                        direction: None,
                        url: vmux_core::file_url::FileUrl::from_path(&path, None, None, None),
                        request_id: request.reply.request_id.0,
                        focus: false,
                    });
                    AgentCommandResult::Text(format!("Knowledge saved: {}", path.display()))
                }
                Err(error) => AgentCommandResult::Error(error),
            },
        };
        responses.write(request.reply.response(result));
    }
}
