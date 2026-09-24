use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_api::room::RemoteModelState;

use crate::event::ModelState;
use crate::state::{ChatUiStatePlugin, ChatUiStateProjection};

pub struct ChatModelPlugin;

impl Plugin for ChatModelPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ChatUiStatePlugin>() {
            app.add_plugins(ChatUiStatePlugin);
        }
        app.init_resource::<Models>()
            .init_resource::<Picker>()
            .add_systems(
                Update,
                (
                    Picker::project
                        .in_set(ModelProjection)
                        .run_if(resource_changed::<Models>),
                    Picker::emit
                        .after(ModelProjection)
                        .run_if(resource_changed::<Picker>),
                ),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ModelProjection;

#[derive(Resource, Default, PartialEq)]
pub struct Models(pub RemoteModelState);

#[derive(Resource, Default)]
pub struct Picker(pub ModelState);

impl Picker {
    fn project(models: Res<Models>, mut picker: ResMut<Picker>) {
        picker.0 = ModelState {
            current_model_id: models.0.selected_id.clone(),
            models: models.0.models.clone(),
            effort_current: models.0.effort.clone(),
            effort_levels: models.0.effort_levels.clone(),
            ..ModelState::default()
        };
    }

    fn emit(picker: Res<Picker>, mut projection: ResMut<ChatUiStateProjection>) {
        projection.write(&picker.0);
    }
}
