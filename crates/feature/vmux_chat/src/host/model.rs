use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_api::room::RemoteModelState;

use super::state::{ChatRuntime, ChatUiStatePlugin, ChatUiStateProjection, RepublishChatUiState};
use crate::event::{
    ModeState, ModelOptionEntry, ModelState, SlashCommand, SlashCommandEntry, SlashCommands,
};

pub struct ChatModelPlugin;

impl Plugin for ChatModelPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ChatUiStatePlugin>() {
            app.add_plugins(ChatUiStatePlugin);
        }
        app.add_message::<Models>()
            .add_systems(
                Update,
                (
                    receive_models,
                    project_model_picker.in_set(ModelProjection),
                    emit_model_picker.after(ModelProjection),
                ),
            )
            .add_observer(publish_model_state)
            .add_observer(publish_mode_state);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ModelProjection;

#[derive(Component, Message, Clone, Default, PartialEq)]
pub struct Models(pub RemoteModelState);

#[derive(Component, Default)]
pub struct Picker(pub ModelState);

#[derive(Component, Default)]
pub(super) struct ModelPickerProjection(pub ModelState);

impl ModelPickerProjection {
    pub(super) fn filtered(&self, query: &str) -> Vec<ModelOptionEntry> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return self.0.models.clone();
        }
        let mut matching = Vec::new();
        for model in &self.0.models {
            if model.id.to_lowercase().contains(&query)
                || model.name.to_lowercase().contains(&query)
                || model.description.to_lowercase().contains(&query)
            {
                matching.push(model.clone());
            }
        }
        matching
    }
}

#[derive(Component, Default)]
pub(super) struct ModeProjection(pub ModeState);

#[derive(Component, Default)]
pub(super) struct SlashCommandProjection(pub SlashCommands);

impl SlashCommandProjection {
    pub(super) fn filtered(&self, query: &str) -> Vec<SlashCommandEntry> {
        let query = query.to_lowercase();
        let mut matching = Vec::new();
        for command in &self.0.commands {
            if command.command.name().starts_with(&query) {
                matching.push(command.clone());
            }
        }
        matching
    }
}

#[derive(EntityEvent)]
pub struct ChatModelStateChanged {
    #[event_target]
    webview: Entity,
    state: ModelState,
}

impl ChatModelStateChanged {
    pub fn new(webview: Entity, state: ModelState) -> Self {
        Self { webview, state }
    }
}

#[derive(EntityEvent)]
pub struct ChatModeStateChanged {
    #[event_target]
    webview: Entity,
    state: ModeState,
}

impl ChatModeStateChanged {
    pub fn new(webview: Entity, state: ModeState) -> Self {
        Self { webview, state }
    }
}

type ChangedModelPicker<'w, 's> =
    Query<'w, 's, (&'static Models, &'static mut Picker), (With<ChatRuntime>, Changed<Models>)>;

fn receive_models(
    mut messages: MessageReader<Models>,
    mut runtimes: Query<&mut Models, With<ChatRuntime>>,
) {
    let Ok(mut models) = runtimes.single_mut() else {
        return;
    };
    for update in messages.read() {
        if *models != *update {
            *models = update.clone();
        }
    }
}

fn project_model_picker(mut runtimes: ChangedModelPicker) {
    let Ok((models, mut picker)) = runtimes.single_mut() else {
        return;
    };
    picker.0 = ModelState {
        current_model_id: models.0.selected_id.clone(),
        models: models.0.models.clone(),
        effort_current: models.0.effort.clone(),
        effort_levels: models.0.effort_levels.clone(),
        ..ModelState::default()
    };
}

fn emit_model_picker(
    mut refreshes: MessageReader<RepublishChatUiState>,
    mut runtimes: Query<(Ref<Picker>, &mut ChatUiStateProjection), With<ChatRuntime>>,
) {
    let refresh = refreshes.read().next().is_some();
    let Ok((picker, mut projection)) = runtimes.single_mut() else {
        return;
    };
    if refresh || picker.is_changed() {
        projection.write(&picker.0);
    }
}

fn publish_model_state(trigger: On<ChatModelStateChanged>, mut commands: Commands) {
    let event = trigger.event();
    commands
        .entity(event.webview)
        .insert(ModelPickerProjection(event.state.clone()));
    commands.trigger(
        vmux_core::host::UiStateWrite::<super::state::ChatUiState>::from_event(
            event.webview,
            &event.state,
        ),
    );
    let mut commands_list = vec![
        SlashCommandEntry {
            command: SlashCommand::Upload,
            description: "Attach files".to_string(),
        },
        SlashCommandEntry {
            command: SlashCommand::Resume,
            description: "Resume a past session".to_string(),
        },
        SlashCommandEntry {
            command: SlashCommand::Mcp,
            description: String::new(),
        },
    ];
    if !event.state.models.is_empty() {
        commands_list.push(SlashCommandEntry {
            command: SlashCommand::Model,
            description: "Select model".to_string(),
        });
    }
    commands
        .entity(event.webview)
        .insert(SlashCommandProjection(SlashCommands {
            commands: commands_list,
        }));
}

fn publish_mode_state(trigger: On<ChatModeStateChanged>, mut commands: Commands) {
    let event = trigger.event();
    commands
        .entity(event.webview)
        .insert(ModeProjection(event.state.clone()));
    commands.trigger(
        vmux_core::host::UiStateWrite::<super::state::ChatUiState>::from_event(
            event.webview,
            &event.state,
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::host::UiStateWrite;

    #[derive(Resource, Default)]
    struct Published(Vec<super::super::state::ChatUiStatePatch>);

    impl Published {
        fn record(
            trigger: On<UiStateWrite<super::super::state::ChatUiState>>,
            mut published: ResMut<Self>,
        ) {
            published.0.push(trigger.event().patch().clone());
        }
    }

    #[test]
    fn chat_owns_model_and_slash_command_projection() {
        let mut app = App::new();
        app.init_resource::<Published>()
            .add_observer(Published::record)
            .add_observer(publish_model_state);
        let webview = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(ChatModelStateChanged::new(
            webview,
            ModelState {
                models: vec![crate::event::ModelOptionEntry {
                    id: "model".to_string(),
                    name: "Model".to_string(),
                    description: String::new(),
                }],
                ..ModelState::default()
            },
        ));
        app.world_mut().flush();

        let published = &app.world().resource::<Published>().0;
        assert_eq!(published.len(), 1);
        assert!(published[0].model.is_some());
        let commands = &app
            .world()
            .get::<SlashCommandProjection>(webview)
            .unwrap()
            .0;
        assert_eq!(
            commands
                .commands
                .iter()
                .map(|entry| entry.command)
                .collect::<Vec<_>>(),
            [
                SlashCommand::Upload,
                SlashCommand::Resume,
                SlashCommand::Mcp,
                SlashCommand::Model,
            ]
        );
    }

    #[test]
    fn model_filtering_is_host_owned() {
        let models = ModelPickerProjection(ModelState {
            models: vec![
                ModelOptionEntry {
                    id: "claude-sonnet".into(),
                    name: "Sonnet".into(),
                    description: "Balanced".into(),
                },
                ModelOptionEntry {
                    id: "claude-opus".into(),
                    name: "Opus".into(),
                    description: "Most capable".into(),
                },
            ],
            ..Default::default()
        });

        assert_eq!(models.filtered("son")[0].id, "claude-sonnet");
        assert_eq!(models.filtered("capable")[0].id, "claude-opus");
        assert_eq!(models.filtered("claude-opus")[0].name, "Opus");
    }
}
