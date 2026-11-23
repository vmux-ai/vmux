use bevy::prelude::*;

#[derive(Component, Default)]
#[require(
    vmux_ecs::UiState<vmux_api::space::SpacesUiState>,
    SpaceSelection,
    SpacesPageSnapshot
)]
pub struct Spaces;

#[derive(Component, Default)]
pub(crate) struct SpaceSelection(pub(crate) usize);

#[derive(Component, Default)]
pub(crate) struct SpacesPageSnapshot(pub(crate) vmux_api::space::SpacesListEvent);
