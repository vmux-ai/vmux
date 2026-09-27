use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_api::room::RemoteModelState;

use crate::event::ModelState;
use crate::state::{ChatRuntime, ChatUiStatePlugin, ChatUiStateProjection, RepublishChatUiState};

pub struct ChatModelPlugin;

impl Plugin for ChatModelPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ChatUiStatePlugin>() {
            app.add_plugins(ChatUiStatePlugin);
        }
        app.add_message::<Models>().add_systems(
            Update,
            (
                receive_models,
                project_model_picker.in_set(ModelProjection),
                emit_model_picker.after(ModelProjection),
            ),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ModelProjection;

#[derive(Component, Message, Clone, Default, PartialEq)]
pub struct Models(pub RemoteModelState);

#[derive(Component, Default)]
pub struct Picker(pub ModelState);

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

fn project_model_picker(
    mut runtimes: Query<(&Models, &mut Picker), (With<ChatRuntime>, Changed<Models>)>,
) {
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
