use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::AgentRequest;
use vmux_ecs::ProcessAnchor;
use vmux_feature::manifest::FeaturePlugin;
use vmux_tool::{ToolCommand, ToolDispatchSet};

use super::session::{AgentRequestUserChoice, AgentSetConversationTitle};

pub struct ChatToolPlugin;

impl Plugin for ChatToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_systems(
                Update,
                (request_user_choice, set_conversation_title).in_set(ToolDispatchSet),
            );
    }
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestUserChoiceTool {
    question: String,
    options: Vec<String>,
}

impl RequestUserChoiceTool {
    fn non_empty(value: &str) -> Option<String> {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetConversationTitleTool {
    title: String,
    summary: String,
}

fn request_user_choice(
    mut commands: Commands,
    requests: Query<
        (
            Entity,
            &Name,
            Option<&ProcessAnchor>,
            &RequestUserChoiceTool,
        ),
        Added<RequestUserChoiceTool>,
    >,
) {
    for (entity, name, anchor, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).and_then(|anchor| {
            let question = RequestUserChoiceTool::non_empty(&args.question)
                .ok_or("request_user_choice.question is empty")?;
            let mut options = Vec::with_capacity(args.options.len());
            for option in &args.options {
                let Some(value) = RequestUserChoiceTool::non_empty(option) else {
                    return Err("request_user_choice options must be non-empty strings".into());
                };
                options.push(value);
            }
            if !(2..=9).contains(&options.len()) {
                return Err("request_user_choice requires 2 to 9 options".to_string());
            }
            AgentRequest::encode(&AgentRequestUserChoice {
                anchor,
                question,
                options,
            })
        });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn set_conversation_title(
    mut commands: Commands,
    requests: Query<
        (
            Entity,
            &Name,
            Option<&ProcessAnchor>,
            &SetConversationTitleTool,
        ),
        Added<SetConversationTitleTool>,
    >,
) {
    for (entity, name, anchor, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).and_then(|anchor| {
            let title = RequestUserChoiceTool::non_empty(&args.title)
                .ok_or("set_conversation_title.title is empty")?;
            if title.chars().count() > 120 {
                return Err("set_conversation_title.title exceeds 120 characters".to_string());
            }
            let summary = RequestUserChoiceTool::non_empty(&args.summary)
                .ok_or("set_conversation_title.summary is empty")?;
            if summary.chars().count() > 500 {
                return Err("set_conversation_title.summary exceeds 500 characters".to_string());
            }
            AgentRequest::encode(&AgentSetConversationTitle {
                anchor,
                title,
                summary,
            })
        });
        commands.entity(entity).insert(ToolCommand(command));
    }
}
