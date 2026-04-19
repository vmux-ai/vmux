use bevy::prelude::*;
use moonshine_save::prelude::*;

pub(crate) struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Profile>();
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct Profile {
    pub name: String,
    pub color: [f32; 4],
    pub icon: Option<String>,
}

impl Profile {
    pub fn default_profile() -> Self {
        Self {
            name: "default".to_string(),
            color: [0.4, 0.6, 1.0, 1.0],
            icon: None,
        }
    }
}
