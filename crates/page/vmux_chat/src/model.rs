//! Which model the conversation runs on, and how hard its agent is asked to think.
//!
//! The relay answers with [`RemoteModelState`], which is the same list the picker draws but not the
//! shape it draws it in. That mapping is the whole of this module, and it belongs beside the page
//! that renders it rather than in whichever app happened to make the request.
//!
//! Fetched per session rather than carried on the session row: the list arrives from the agent
//! after the session exists, so a copy taken with the row would offer models it has since dropped.
//! The app owns the asking — it is the half that can reach the link — and writes what came back
//! here.

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_wire::page::PageEmit;
use vmux_wire::room::RemoteModelState;

use crate::event::{MODEL_STATE_EVENT, ModelState};

/// Keeps [`Picker`] current with what the Mac last said the session can run on.
pub struct ChatModelPlugin;

impl Plugin for ChatModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Models>()
            .init_resource::<Picker>()
            .add_message::<PageEmit>()
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

/// When [`Picker`] is rebuilt, so the emit ordered after it carries this turn's list.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ModelProjection;

/// What the Mac last said about the open session's models. Written by the app, read by nothing
/// else.
#[derive(Resource, Default, PartialEq)]
pub struct Models(pub RemoteModelState);

/// The model picker, as the page expects to be told about it.
#[derive(Resource, Default)]
pub struct Picker(pub ModelState);

impl Picker {
    /// Describe the models the way the shared picker expects to be told about them.
    ///
    /// `current_model_name` and `agent_key` are left empty: the relay does not answer for either,
    /// and the picker reads an absent name as "show the id" rather than rendering a blank row.
    fn project(models: Res<Models>, mut picker: ResMut<Picker>) {
        picker.0 = ModelState {
            current_model_id: models.0.selected_id.clone(),
            models: models.0.models.clone(),
            effort_current: models.0.effort.clone(),
            effort_levels: models.0.effort_levels.clone(),
            ..ModelState::default()
        };
    }

    /// Hand the rebuilt picker to whichever page is listening for it.
    fn emit(picker: Res<Picker>, mut emits: MessageWriter<PageEmit>) {
        let Some(emit) = PageEmit::of(MODEL_STATE_EVENT, &picker.0) else {
            return;
        };
        emits.write(emit);
    }
}
