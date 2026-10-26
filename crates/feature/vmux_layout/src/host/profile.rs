use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use moonshine_save::prelude::*;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::profile"]
#[require(Save)]
pub struct Profile;

impl Profile {
    pub fn owned_by(name: impl Into<String>, owner: Entity) -> impl Bundle {
        (Self, Name::new(name.into()), ChildOf(owner))
    }
}

#[derive(SystemParam)]
pub struct Profiles<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    profiles: Query<'w, 's, &'static Name, With<Profile>>,
}

impl Profiles<'_, '_> {
    pub fn name(&self, owner: Entity) -> Option<&str> {
        let children = self.children.get(owner).ok()?;
        for child in children.iter() {
            if let Ok(name) = self.profiles.get(child) {
                return Some(name.as_str());
            }
        }
        None
    }
}
