use bevy::prelude::*;

#[derive(Component, Default)]
#[require(super::SpacesUiStateUpdates, SpaceSelection, SpacesPageSnapshot)]
pub struct Spaces;

#[derive(Component, Default)]
pub(crate) struct SpaceSelection(pub(crate) usize);

#[derive(Component, Default)]
pub(crate) struct SpacesPageSnapshot(pub(crate) vmux_api::space::SpacesListEvent);
