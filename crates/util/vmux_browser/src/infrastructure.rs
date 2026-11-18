use bevy::prelude::*;

#[derive(Component)]
pub struct InfrastructureWebview;

#[derive(Component)]
pub struct RetiredInfrastructureWebview(Entity);

impl RetiredInfrastructureWebview {
    pub fn new(entity: Entity) -> Self {
        Self(entity)
    }

    pub fn contains(&self, entity: Entity) -> bool {
        self.0 == entity
    }
}

#[derive(Component)]
pub struct PopupWebview;
