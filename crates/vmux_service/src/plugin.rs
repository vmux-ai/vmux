use bevy::prelude::*;

pub struct ServicePlugin;

impl Plugin for ServicePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
    }
}
