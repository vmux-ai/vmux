use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::{AgentRequest, AgentRequestUserChoice};
use vmux_core::ProcessAnchor;
use vmux_tool::{ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin};

pub struct ChatToolPlugin;

impl Plugin for ChatToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::from_feature(
            include_str!("feature.ron"),
            "default",
        ))
        .register_tool::<RequestUserChoiceArgs>("request_user_choice")
        .add_systems(Update, request_user_choice.in_set(ToolDispatchSet));
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestUserChoiceArgs {
    question: String,
    options: Vec<String>,
}

fn request_user_choice(
    mut commands: Commands,
    requests: Query<
        (
            Entity,
            &Name,
            Option<&ProcessAnchor>,
            &RequestUserChoiceArgs,
        ),
        Added<RequestUserChoiceArgs>,
    >,
) {
    for (entity, name, anchor, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).and_then(|anchor| {
            let question = Text::into_option(args.question.clone())
                .ok_or("request_user_choice.question is empty")?;
            let options = args
                .options
                .iter()
                .cloned()
                .map(Text::into_option)
                .collect::<Option<Vec<_>>>()
                .ok_or("request_user_choice options must be non-empty strings")?;
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

struct Text;

impl Text {
    fn into_option(value: String) -> Option<String> {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    }
}
